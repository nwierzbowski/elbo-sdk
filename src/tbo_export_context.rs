//! TBO Export Context - SDK-side orchestrator for multi-format streaming export.
//!
//! Manages export configuration for multiple formats (LBO, TBO-Points, TBO-Fragments),
//! tracks accumulated memory usage, and orchestrates the push → downsample → drop → flush cycle.
//!
//! Flushing is driven by engine memory threshold (max_memory_mb), not per-format target sizes.
//! Each flush writes all selected formats to disk in a single pass.

use pyo3::prelude::*;
use std::collections::HashMap;

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
    max_memory_mb: f64,
    accumulated_bytes: u64,
    flush_threshold: u64,
    next_batch_number: u32,
    batch_size: usize,
    channel_mask: u32,
    target_point_count: u32,
    pending_downsample: Vec<Vec<u8>>,
    pending_drop: Vec<Vec<u8>>,
    skip_normalization: bool,
}

#[pymethods]
impl TboExportContext {
    #[new]
    fn new() -> Self {
        Self {
            output_dir: String::new(),
            formats: Vec::new(),
            max_memory_mb: 16384.0,
            accumulated_bytes: 0,
            flush_threshold: 0,
            next_batch_number: 0,
            batch_size: 900,
            channel_mask: 0,
            target_point_count: 1024,
            pending_downsample: Vec::new(),
            pending_drop: Vec::new(),
            skip_normalization: false,
        }
    }

    /// Initialize the export context for multi-format export.
    ///
    /// Args:
    ///     output_dir: Base directory for output files
    ///     formats: List of ExportFormat objects (e.g., [ExportFormat("lbo"), ExportFormat("points")])
    ///     max_memory_mb: Engine memory threshold for flush in MB (default 16384 = 16 GB)
    ///     skip_normalization: Skip per-asset centering and unit scaling in transforms
    #[pyo3(text_signature = "(self, output_dir, formats, max_memory_mb, skip_normalization)")]
    fn init(
        &mut self,
        output_dir: String,
        formats: Vec<ExportFormat>,
        max_memory_mb: f64,
        skip_normalization: bool,
    ) -> PyResult<()> {
        self.output_dir = output_dir;
        self.max_memory_mb = max_memory_mb;
        self.skip_normalization = skip_normalization;
        self.formats = formats;
        self.accumulated_bytes = 0;
        self.next_batch_number = 0;
        self.pending_downsample.clear();
        self.pending_drop.clear();

        // Compute flush threshold from max_memory_mb (in bytes)
        self.flush_threshold = (self.max_memory_mb * 1024.0 * 1024.0) as u64;

        // Extract channel_mask and target_point_count if Points mode is selected
        let has_points = self.formats.iter().any(|f| f.format == "points");
        if has_points {
            self.channel_mask = DEFAULT_CHANNEL_MASK;
            self.target_point_count = 1024;
            engine_api::tbo_config_command(self.channel_mask, self.target_point_count)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?;
        }

        // Log configuration
        let format_names: Vec<&str> = self.formats.iter().map(|f| f.format.as_str()).collect();
        eprintln!(
            "[TBO] Config: formats={:?}, max_memory={} MB, flush_threshold={:.0} MB, skip_normalization={}",
            format_names,
            self.max_memory_mb,
            self.flush_threshold as f64 / (1024.0 * 1024.0),
            self.skip_normalization,
        );

        Ok(())
    }

    /// Add a mesh UUID to the pending batch with size tracking.
    ///
    /// Triggers an automatic flush to disk when either:
    /// - The points batch size is reached (to avoid buffer overflow)
    /// - The memory threshold is exceeded (to free shared memory)
    ///
    /// Args:
    ///     uuid_bytes: UUID bytes (32 bytes)
    ///     size_bytes: Size in bytes of shared memory allocated for this push
    ///
    /// Returns:
    ///     List of (format_name, [filenames]) tuples if a flush occurred, empty list otherwise
    fn accumulate(&mut self, uuid_bytes: Vec<u8>, size_bytes: u64) -> PyResult<Vec<(String, Vec<String>)>> {
        if uuid_bytes.len() != Uuid::SIZE {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!("UUID must be {} bytes, got {}", Uuid::SIZE, uuid_bytes.len()),
            ));
        }

        // Track bytes for memory-based flush
        self.accumulated_bytes += size_bytes;

        // For points mode, track for downsample/drop batching
        if self.formats.iter().any(|f| f.format == "points") {
            self.pending_downsample.push(uuid_bytes.clone());
            self.pending_drop.push(uuid_bytes);

            // Check if batch is full - flush immediately to avoid buffer overflow
            if self.pending_downsample.len() >= self.batch_size {
                return self.flush();
            }
        }

        // Check if memory threshold exceeded - flush to free shared memory
        if self.accumulated_bytes >= self.flush_threshold {
            return self.flush();
        }

        Ok(vec![])
    }

    /// Flush all formats to disk.
    ///
    /// Writes all selected formats in a single pass, then drops all groups from the scene graph.
    ///
    /// Returns:
    ///     List of (format_name, [filenames]) tuples
    fn flush(&mut self) -> PyResult<Vec<(String, Vec<String>)>> {
        let mut result: HashMap<String, Vec<String>> = HashMap::new();

        // Flush Points mode: downsample pending + flush to disk
        if self.formats.iter().any(|f| f.format == "points") {
            // Downsample pending
            if !self.pending_downsample.is_empty() {
                let downsample_uuids = std::mem::take(&mut self.pending_downsample);

                let pivot_downsample: Result<Vec<Uuid>, PyErr> = downsample_uuids
                    .iter()
                    .map(|bytes| {
                        let mut uuid = Uuid { bytes: [0u8; Uuid::SIZE] };
                        uuid.bytes.copy_from_slice(bytes);
                        Ok(uuid)
                    })
                    .collect();

                let pivot_downsample = pivot_downsample?;

                match engine_api::tbo_downsample_command(pivot_downsample) {
                    Ok(_) => {}
                    Err(e) => return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                        format!("tbo_downsample failed: {}", e),
                    )),
                }
            }

            let batch_offset = self.next_batch_number;
            let points_dir = format!("{}/tbo_points", self.output_dir);
            eprintln!("[TBO] Flushing points to: {}", points_dir);
            match engine_api::tbo_flush_command(&points_dir, batch_offset) {
                Ok(resp) => {
                    let filenames = resp.read_tbo_flush()
                        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                            format!("Failed to read flush response: {}", e),
                        ))?;
                    let result_vec: Vec<String> = filenames.into_iter().map(|s| s.to_string()).collect();
                    let count = result_vec.len();
                    result.insert("points".to_string(), result_vec);
                    self.next_batch_number += count as u32;
                }
                Err(e) => return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    format!("tbo_flush failed: {}", e),
                )),
            }
        }

        // Flush Meshes mode (TBO-Fragments)
        if self.formats.iter().any(|f| f.format == "meshes") {
            let meshes_dir = format!("{}/tbo_fragments", self.output_dir);
            eprintln!("[TBO] Flushing meshes to: {}", meshes_dir);
            match engine_api::export_all_asset_tbo_command(&meshes_dir, self.skip_normalization) {
                Ok(resp) => {
                    let filenames = resp.read_tbo_flush()
                        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                            format!("Failed to read flush response: {}", e),
                        ))?;
                    let result_vec: Vec<String> = filenames.into_iter().map(|s| s.to_string()).collect();
                    result.insert("meshes".to_string(), result_vec);
                }
                Err(e) => return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    format!("export_all_asset_tbo failed: {}", e),
                )),
            }
        }

        // Flush LBO mode
        if self.formats.iter().any(|f| f.format == "lbo") {
            let lbo_dir = format!("{}/lbo", self.output_dir);
            eprintln!("[TBO] Flushing lbo to: {}", lbo_dir);
            engine_api::export_all_command(&lbo_dir)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    format!("export_all failed: {}", e),
                ))?;
            result.insert("lbo".to_string(), vec![]);
        }

        // Drop all groups once (after all formats flushed)
        engine_api::drop_all_groups_command()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("drop_all_groups failed: {}", e),
            ))?;

        // Reset counters
        self.accumulated_bytes = 0;

        // Convert HashMap to Vec for Python
        let result_vec: Vec<(String, Vec<String>)> = result.into_iter().collect();
        Ok(result_vec)
    }

}
