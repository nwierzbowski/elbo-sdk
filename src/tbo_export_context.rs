//! TBO Export Context - SDK-side orchestrator for streaming export.
//!
//! Manages export configuration via channel flags, tracks accumulated memory usage,
//! and orchestrates the push → flush → drop cycle.
//!
//! Flushing is driven by engine memory threshold (max_memory_mb) or on close().
//! Each flush calls the engine's consolidated tbo_export command.

use pyo3::prelude::*;

use crate::asset_sync_context::AssetSyncContext;
use pivot_com_types::fields::Uuid;

use crate::engine_api;

/// Export format configuration (kept for backward compatibility).
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

/// SDK-side orchestrator for streaming export.
///
/// Holds export configuration via channel flags, tracks accumulated memory usage,
/// and knows when to trigger flush based on max_memory_mb threshold.
#[pyclass(unsendable)]
pub struct TboExportContext {
    output_dir: String,
    scene_transform: bool,
    scene_similarity: bool,
    asset_embedding: bool,
    asset_transform: bool,
    fragment_xyz: bool,
    normal_variance: bool,
    surface_variation: bool,
    combined: bool,
    accumulated_bytes: u64,
    flush_threshold: u64,
    next_batch_number: u32,
    target_point_count: u32,
    pending_asset_ctx: Option<AssetSyncContext>,
    pending_allocated_bytes: u64,
    pending_uuid: Option<Vec<u8>>,
}

#[pymethods]
impl TboExportContext {
    /// Create and initialize the export context.
    ///
    /// Args:
    ///     output_dir: Base directory for output files
    ///     scene_transform: Export scene-level 16 transforms
    ///     scene_similarity: Export scene-level similarity (requires embeddings)
    ///     asset_embedding: Export 256-d asset embeddings
    ///     asset_transform: Export 16 asset transforms
    ///     fragment_xyz: Export XYZ points
    ///     normal_variance: Export normal variance points
    ///     surface_variation: Export surface variation points
    ///     combined: Export combined points
    ///     max_memory_mb: Engine memory threshold for flush in MB (default 16384 = 16 GB)
    ///     target_point_count: Target number of points for downsampling (default 1024)
    #[new]
    #[pyo3(signature = (
        output_dir,
        scene_transform,
        scene_similarity,
        asset_embedding,
        asset_transform,
        fragment_xyz,
        normal_variance,
        surface_variation,
        combined,
        max_memory_mb,
        target_point_count,
    ))]
    fn new(
        output_dir: String,
        scene_transform: bool,
        scene_similarity: bool,
        asset_embedding: bool,
        asset_transform: bool,
        fragment_xyz: bool,
        normal_variance: bool,
        surface_variation: bool,
        combined: bool,
        max_memory_mb: f64,
        target_point_count: u32,
    ) -> PyResult<Self> {
        let flush_threshold = (max_memory_mb * 1024.0 * 1024.0) as u64;

        // Log configuration
        eprintln!(
            "[TBO] Config: scene_transform={}, scene_similarity={}, asset_embedding={}, asset_transform={}, fragment_xyz={}, normal_variance={}, surface_variation={}, combined={}, max_memory={} MB, target_points={}",
            scene_transform, scene_similarity, asset_embedding, asset_transform,
            fragment_xyz, normal_variance, surface_variation, combined,
            max_memory_mb, target_point_count
        );

        Ok(Self {
            output_dir,
            scene_transform,
            scene_similarity,
            asset_embedding,
            asset_transform,
            fragment_xyz,
            normal_variance,
            surface_variation,
            combined,
            accumulated_bytes: 0,
            flush_threshold,
            next_batch_number: 0,
            target_point_count,
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
    /// Flush all selected channels to disk and drop all groups.
    ///
    /// Calls the engine's consolidated tbo_export command which handles
    /// all channel selection and file writing internally.
    ///
    /// Returns filenames written, or empty list if nothing was flushed.
    fn do_flush(&mut self) -> Result<Vec<(String, Vec<String>)>, String> {
        // Call the consolidated TBO export command
        let scene_uuid = engine_api::generate_uuid_bytes();
        let filenames = match engine_api::tbo_export_command(
            &self.output_dir,
            scene_uuid,
            self.scene_transform,
            self.scene_similarity,
            self.asset_embedding,
            self.asset_transform,
            self.fragment_xyz,
            self.normal_variance,
            self.surface_variation,
            self.combined,
            self.target_point_count,
        ) {
            Ok(resp) => {
                let file_names = resp.read_tbo_flush()
                    .map_err(|e| format!("Failed to read flush response: {}", e))?;
                let result_vec: Vec<String> = file_names.into_iter().map(|s| s.to_string()).collect();
                let count = result_vec.len();
                self.next_batch_number += count as u32;
                result_vec
            }
            Err(e) => return Err(format!("tbo_export failed: {}", e)),
        };

        // Drop all groups once (after all formats flushed)
        engine_api::drop_all_groups_command()
            .map_err(|e| format!("drop_all_groups failed: {}", e))?;

        // Reset counters
        self.accumulated_bytes = 0;

        // Convert to Vec for Python
        let result_vec: Vec<(String, Vec<String>)> = filenames.iter()
            .map(|f| ("tbo".to_string(), vec![f.clone()]))
            .collect();
        Ok(result_vec)
    }
}
