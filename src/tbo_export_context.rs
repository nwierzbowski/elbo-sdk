//! TBO Export Context - SDK-side orchestrator for multi-format streaming export.
//!
//! Manages export configuration for multiple formats (LBO, TBO-Points, TBO-Fragments),
//! tracks accumulated memory usage, and orchestrates the push → flush → drop cycle.
//!
//! Flushing is driven by engine memory threshold (max_memory_mb) or on close().
//! Each flush writes all selected formats to disk in a single pass.

use pyo3::prelude::*;
use std::collections::HashMap;

use crate::asset_sync_context::AssetSyncContext;
use pivot_com_types::fields::Uuid;

use crate::engine_api;

/// Channel bit flags (must match engine constants)
const CHANNEL_X: u32 = 1 << 0;
const CHANNEL_Y: u32 = 1 << 1;
const CHANNEL_Z: u32 = 1 << 2;
const CHANNEL_NORMAL_VARIANCE: u32 = 1 << 3;
const CHANNEL_SURFACE_VARIATION: u32 = 1 << 4;
const CHANNEL_COMBINED: u32 = 1 << 5;
const DEFAULT_CHANNEL_MASK: u32 = CHANNEL_X | CHANNEL_Y | CHANNEL_Z
    | CHANNEL_NORMAL_VARIANCE | CHANNEL_SURFACE_VARIATION | CHANNEL_COMBINED;

/// Export format configuration.
#[pyclass]
#[derive(Clone)]
pub struct ExportFormat {
    /// Format name: "lbo", "points", or "meshes"
    #[pyo3(get)]
    pub format: String,
}

#[pymethods]
impl ExportFormat {
    #[new]
    fn new(format: String) -> Self {
        Self { format }
    }
}

/// SDK-side orchestrator for streaming multi-format export.
///
/// Holds export configuration for multiple formats, tracks accumulated memory usage,
/// and knows when to trigger flush based on max_memory_mb threshold.
#[pyclass(unsendable)]
pub struct TboExportContext {
    output_dir: String,
    formats: Vec<ExportFormat>,
    accumulated_bytes: u64,
    flush_threshold: u64,
    next_batch_number: u32,
    skip_normalization: bool,
    pending_asset_ctx: Option<AssetSyncContext>,
    pending_allocated_bytes: u64,
    pending_uuid: Option<Vec<u8>>,
}

#[pymethods]
impl TboExportContext {
    /// Create and initialize the export context for multi-format export.
    ///
    /// Args:
    ///     output_dir: Base directory for output files
    ///     formats: List of ExportFormat objects (e.g., [ExportFormat("lbo"), ExportFormat("points")])
    ///     max_memory_mb: Engine memory threshold for flush in MB (default 16384 = 16 GB)
    ///     skip_normalization: Skip per-asset centering and unit scaling in transforms
    #[new]
    fn new(
        output_dir: String,
        formats: Vec<ExportFormat>,
        max_memory_mb: f64,
        skip_normalization: bool,
    ) -> PyResult<Self> {
        let flush_threshold = (max_memory_mb * 1024.0 * 1024.0) as u64;

        // Log configuration
        let format_names: Vec<&str> = formats.iter().map(|f| f.format.as_str()).collect();
        eprintln!(
            "[TBO] Config: formats={:?}, max_memory={} MB, flush_threshold={:.0} MB, skip_normalization={}",
            format_names,
            max_memory_mb,
            flush_threshold as f64 / (1024.0 * 1024.0),
            skip_normalization,
        );

        Ok(Self {
            output_dir,
            formats,
            accumulated_bytes: 0,
            flush_threshold,
            next_batch_number: 0,
            skip_normalization,
            pending_asset_ctx: None,
            pending_allocated_bytes: 0,
            pending_uuid: None,
        })
    }

    /// Prepare mesh send for export.
    ///
    /// Creates an AssetSyncContext internally and returns buffer memoryviews
    /// for Python to fill. The allocated_bytes is captured internally.
    ///
    /// Args:
    ///     vert_counts: Vertex counts per object
    ///     edge_counts: Edge counts per object
    ///     loop_counts: Loop counts per object
    ///     total_loop_lengths: Total loop lengths
    ///     object_counts: Object counts
    ///     group_names: Group names
    ///     surface_contexts: Surface contexts
    ///     asset_uuids: Asset UUIDs
    ///
    /// Returns:
    ///     Tuple of (verts, edges, loops, loop_bases, object_loop_counts,
    ///               transforms, vert_counts, edge_counts, object_names, obj_uuids, embeddings)
    fn prepare_mesh_send(
        &mut self,
        py: Python,
        vert_counts: Vec<u32>,
        edge_counts: Vec<u32>,
        loop_counts: Vec<u32>,
        total_loop_lengths: Vec<u32>,
        object_counts: Vec<u32>,
        group_names: Vec<String>,
        surface_contexts: Vec<u16>,
        asset_uuids: Vec<Uuid>,
    ) -> PyResult<(
        Py<PyAny>,
        Py<PyAny>,
        Py<PyAny>,
        Py<PyAny>,
        Py<PyAny>,
        Py<PyAny>,
        Py<PyAny>,
        Py<PyAny>,
        Py<PyAny>,
        Py<PyAny>,
        Py<PyAny>,
    )> {
        let (asset_ctx, allocated_bytes) = engine_api::allocate_memory(
            vert_counts,
            edge_counts,
            loop_counts,
            total_loop_lengths,
            object_counts,
            group_names,
            surface_contexts,
            asset_uuids.clone(),
        )
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?;

        self.pending_asset_ctx = Some(asset_ctx);
        self.pending_allocated_bytes = allocated_bytes;
        self.pending_uuid = Some(asset_uuids[0].bytes.to_vec());

        // Return buffer memoryviews for Python to fill
        if let Some(ref mut ctx) = self.pending_asset_ctx {
            ctx.buffers(py, 0)
        } else {
            unreachable!("just set pending_asset_ctx above")
        }
    }

    /// Send accumulated mesh and track bytes.
    ///
    /// Triggers an automatic flush to disk when the memory threshold is exceeded.
    ///
    /// Returns:
    ///     List of (format_name, [filenames]) tuples if a flush occurred, empty list otherwise
    fn accumulate(&mut self) -> PyResult<Vec<(String, Vec<String>)>> {
        let _uuid_bytes = self.pending_uuid.take().ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("No pending UUID - call prepare_mesh_send first")
        })?;

        // Send the pending asset context
        if let Some(mut asset_ctx) = self.pending_asset_ctx.take() {
            asset_ctx.send();
        }

        // Track bytes for memory-based flush
        self.accumulated_bytes += self.pending_allocated_bytes;
        self.pending_allocated_bytes = 0;

        // Check if memory threshold exceeded - flush to free shared memory
        if self.accumulated_bytes >= self.flush_threshold {
            return self.do_flush().map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e));
        }

        Ok(vec![])
    }

    /// Final flush and cleanup.
    fn close(&mut self) -> PyResult<()> {
        self.do_flush().map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?;
        Ok(())
    }
}

impl TboExportContext {
    /// Flush all formats to disk and drop all groups.
    ///
    /// Returns filenames written by points and meshes exporters, or empty list for lbo.
    fn do_flush(&mut self) -> Result<Vec<(String, Vec<String>)>, String> {
        let mut result: HashMap<String, Vec<String>> = HashMap::new();

        // Flush Points mode: single combined operation from scene graph
        if self.formats.iter().any(|f| f.format == "points") {
            let channel_mask = DEFAULT_CHANNEL_MASK;
            let target_point_count = 1024u32;
            let points_dir = format!("{}/tbo_points", self.output_dir);
            eprintln!("[TBO] Flushing points to: {}", points_dir);
            match engine_api::tbo_points_flush_command(&points_dir, channel_mask, target_point_count) {
                Ok(resp) => {
                    let filenames = resp.read_tbo_flush()
                        .map_err(|e| format!("Failed to read flush response: {}", e))?;
                    let result_vec: Vec<String> = filenames.into_iter().map(|s| s.to_string()).collect();
                    let count = result_vec.len();
                    result.insert("points".to_string(), result_vec);
                    self.next_batch_number += count as u32;
                }
                Err(e) => return Err(format!("tbo_points_flush failed: {}", e)),
            }
        }

        // Flush Meshes mode (TBO-Fragments)
        if self.formats.iter().any(|f| f.format == "meshes") {
            let meshes_dir = format!("{}/tbo_fragments", self.output_dir);
            eprintln!("[TBO] Flushing meshes to: {}", meshes_dir);
            match engine_api::export_all_asset_tbo_command(&meshes_dir, self.skip_normalization) {
                Ok(resp) => {
                    let filenames = resp.read_tbo_flush()
                        .map_err(|e| format!("Failed to read flush response: {}", e))?;
                    let result_vec: Vec<String> = filenames.into_iter().map(|s| s.to_string()).collect();
                    result.insert("meshes".to_string(), result_vec);
                }
                Err(e) => return Err(format!("export_all_asset_tbo failed: {}", e)),
            }
        }

        // Flush LBO mode
        if self.formats.iter().any(|f| f.format == "lbo") {
            let lbo_dir = format!("{}/lbo", self.output_dir);
            eprintln!("[TBO] Flushing lbo to: {}", lbo_dir);
            engine_api::export_all_command(&lbo_dir)
                .map_err(|e| format!("export_all failed: {}", e))?;
            result.insert("lbo".to_string(), vec![]);
        }

        // Drop all groups once (after all formats flushed)
        engine_api::drop_all_groups_command()
            .map_err(|e| format!("drop_all_groups failed: {}", e))?;

        // Reset counters
        self.accumulated_bytes = 0;

        // Convert HashMap to Vec for Python
        let result_vec: Vec<(String, Vec<String>)> = result.into_iter().collect();
        Ok(result_vec)
    }
}
