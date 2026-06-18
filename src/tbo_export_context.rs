//! TBO Export Context - SDK-side orchestrator for streaming TBO export.
//!
//! Manages export configuration, tracks accumulated mesh count,
//! and orchestrates the push → downsample → drop → flush cycle.
//!
//! Batches downsample and drop calls for efficiency.

use pyo3::prelude::*;

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

/// Count set bits in a 6-bit channel mask.
fn popcount(mask: u32) -> u32 {
    let mut count = 0;
    for i in 0..6 {
        if mask & (1 << i) != 0 {
            count += 1;
        }
    }
    count
}

/// Resolve channel mask from legacy flags value.
fn resolve_channel_mask(flags: u32) -> u32 {
    if flags == 0x1 {
        DEFAULT_CHANNEL_MASK
    } else if flags == 0 {
        CHANNEL_X | CHANNEL_Y | CHANNEL_Z
    } else {
        flags
    }
}

/// Export mode for TBO export.
#[pyclass]
#[derive(Clone)]
pub enum TboExportMode {
    /// Point-based export (mesh TBO) - uses downsample + flush pipeline
    Points,
    /// Mesh-based export (asset TBO) - uses export_all_asset_tbo
    Meshes,
    /// LBO export - uses export_all + drop_all (no downsampling)
    Lbo,
}

/// SDK-side orchestrator for streaming TBO export.
///
/// Holds export configuration, tracks accumulated count, and knows
/// when to trigger flush based on target file size.
#[pyclass(unsendable)]
pub struct TboExportContext {
    output_dir: String,
    target_bytes: u64,
    flags: u32,
    target_point_count: u32,
    channel_mask: u32,
    accumulated_count: u64,
    flush_threshold: u64,
    next_batch_number: u32,
    batch_size: usize,
    pending_downsample: Vec<Vec<u8>>,
    pending_drop: Vec<Vec<u8>>,
    export_mode: TboExportMode,
    exported_count: u64,
    pushed_count: u64,
}

#[pymethods]
impl TboExportContext {
    #[new]
    fn new() -> Self {
        Self {
            output_dir: String::new(),
            target_bytes: 0,
            flags: 0,
            target_point_count: 1024,
            channel_mask: 0,
            accumulated_count: 0,
            flush_threshold: 0,
            next_batch_number: 0,
            batch_size: 900,
            pending_downsample: Vec::new(),
            pending_drop: Vec::new(),
            export_mode: TboExportMode::Points,
            exported_count: 0,
            pushed_count: 0,
        }
    }

    /// Initialize the export context and configure the engine.
    ///
    /// Args:
    ///     output_dir: Directory to write .tbo files
    ///     target_bytes: Target size per .tbo file (e.g. 4GB = 4 * 1024^3)
    ///     flags: Channel mask flags (0x1 = all channels, or bit mask)
    ///     target_point_count: Points per mesh after downsampling (default 1024)
    ///     batch_size: Number of UUIDs per downsample+drop batch
    ///     export_mode: Export mode - "points" for mesh TBO, "meshes" for asset TBO
    #[pyo3(text_signature = "(self, output_dir, target_bytes, flags, target_point_count, batch_size, export_mode)")]
    fn init(
        &mut self,
        output_dir: String,
        target_bytes: u64,
        flags: u32,
        target_point_count: u32,
        batch_size: usize,
        export_mode: Option<String>,
    ) -> PyResult<()> {
        self.output_dir = output_dir;
        self.target_bytes = target_bytes;
        self.flags = flags;
        self.target_point_count = target_point_count;
        self.channel_mask = resolve_channel_mask(flags);
        self.batch_size = batch_size;
        self.accumulated_count = 0;
        self.exported_count = 0;
        self.pushed_count = 0;
        self.next_batch_number = 0;
        self.pending_downsample.clear();
        self.pending_drop.clear();

        // Set export mode
        self.export_mode = match export_mode.as_deref() {
            Some("meshes") => TboExportMode::Meshes,
            Some("lbo") => TboExportMode::Lbo,
            _ => TboExportMode::Points,
        };

        // Compute flush threshold: how many objects fill target_bytes
        if matches!(self.export_mode, TboExportMode::Meshes) {
            // Meshes mode: 272 channels per object (256 embedding + 16 transform) * 4 bytes
            let per_object_bytes = 272 * 4; // 1088 bytes
            self.flush_threshold = target_bytes / per_object_bytes;
        } else {
            // Points mode: target_point_count * channel_count * 4 bytes per mesh
            let channel_count = popcount(self.channel_mask) as u64;
            let per_mesh_bytes = (target_point_count as u64) * channel_count * 4;
            self.flush_threshold = if per_mesh_bytes > 0 {
                let threshold = target_bytes / per_mesh_bytes;
                if threshold < 1000 {
                    1000
                } else {
                    threshold
                }
            } else {
                100_000
            };
        }

        let mode_label = match &self.export_mode {
            TboExportMode::Points => "points",
            TboExportMode::Meshes => "meshes",
            TboExportMode::Lbo => "lbo",
        };
        if matches!(self.export_mode, TboExportMode::Meshes) {
            eprintln!(
                "[TBO] Config: target={} MB, mode={}, flush_threshold={} objects, batch_size={}",
                target_bytes as f64 / (1024.0 * 1024.0),
                mode_label,
                self.flush_threshold,
                self.batch_size,
            );
        } else {
            let channel_count = popcount(self.channel_mask) as u64;
            eprintln!(
                "[TBO] Config: target={} GB, channels={}, pts={}, mode={}, flush_threshold={}, batch_size={}",
                target_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                channel_count,
                target_point_count,
                mode_label,
                self.flush_threshold,
                self.batch_size,
            );
        }

        // Configure engine with compute params only (for points mode)
        if let TboExportMode::Points = &self.export_mode {
            engine_api::tbo_config_command(self.channel_mask, self.target_point_count)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?;
        }

        // Reset dedup state at start of asset TBO export
        if let TboExportMode::Meshes = &self.export_mode {
            engine_api::export_dup_reset_command()
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    format!("Failed to reset export dedup state: {}", e),
                ))?;
        }

        Ok(())
    }

    /// Add a mesh UUID to the pending batch.
    ///
    /// When the batch reaches batch_size, automatically flushes
    /// downsample and drop calls to the engine.
    ///
    /// Args:
    ///     uuid_bytes: UUID bytes (32 bytes)
    ///     object_count: Number of objects in this asset (for meshes mode tracking)
    ///
    /// Returns:
    ///     Number of meshes accumulated in this call (1 if batch flushed, 0 if still pending)
    fn accumulate(&mut self, uuid_bytes: Vec<u8>, object_count: u32) -> PyResult<u32> {
        if uuid_bytes.len() != Uuid::SIZE {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!("UUID must be {} bytes, got {}", Uuid::SIZE, uuid_bytes.len()),
            ));
        }

        // For meshes mode, track asset count (1 asset per call) for reporting
        if matches!(self.export_mode, TboExportMode::Meshes) {
            self.pushed_count += 1;
        }

        // For meshes/lbo mode, track object count for flush threshold
        if matches!(self.export_mode, TboExportMode::Meshes | TboExportMode::Lbo) {
            self.accumulated_count += object_count as u64;
            self.pending_downsample.push(uuid_bytes.clone());
            self.pending_drop.push(uuid_bytes);
            return Ok(0);
        }

        self.pending_downsample.push(uuid_bytes.clone());
        self.pending_drop.push(uuid_bytes);

        // Check if batch is full - downsample immediately to avoid buffer overflow
        if self.pending_downsample.len() >= self.batch_size {
            return self.flush_pending();
        }

        Ok(0)
    }

    /// Flush pending downsample and drop calls to the engine.
    fn flush_pending(&mut self) -> PyResult<u32> {
        if self.pending_downsample.is_empty() {
            return Ok(0);
        }

        // For meshes/lbo mode, skip downsample/drop (export does its own)
        if matches!(&self.export_mode, TboExportMode::Meshes | TboExportMode::Lbo) {
            self.pending_downsample.clear();
            self.pending_drop.clear();
            return Ok(0);
        }

        let downsample_uuids = std::mem::take(&mut self.pending_downsample);
        let drop_uuids = std::mem::take(&mut self.pending_drop);

        // Downsample
        let pivot_downsample: Result<Vec<Uuid>, PyErr> = downsample_uuids
            .iter()
            .map(|bytes| {
                let mut uuid = Uuid { bytes: [0u8; Uuid::SIZE] };
                uuid.bytes.copy_from_slice(bytes);
                Ok(uuid)
            })
            .collect();

        let pivot_downsample = pivot_downsample?;
        let count = pivot_downsample.len();

        match engine_api::tbo_downsample_command(pivot_downsample) {
            Ok(resp) => {
                let accumulated = resp.read_tbo_downsample();
                self.accumulated_count += accumulated as u64;

                // Drop
                let pivot_drop: Result<Vec<Uuid>, PyErr> = drop_uuids
                    .iter()
                    .map(|bytes| {
                        let mut uuid = Uuid { bytes: [0u8; Uuid::SIZE] };
               uuid.bytes.copy_from_slice(&bytes);
                        Ok(uuid)
                    })
                    .collect();

                let pivot_drop = pivot_drop.map_err(|e| e)?;
                engine_api::drop_groups_command(pivot_drop)
                    .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?;

                Ok(accumulated)
            }
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("tbo_downsample failed: {}, requested: {}", e, count),
            )),
        }
    }

    /// Downsample a batch of UUIDs from the engine scene graph and accumulate results.
    ///
    /// Args:
    ///     uuids: List of UUID byte arrays (each 32 bytes)
    ///
    /// Returns:
    ///     Number of meshes successfully accumulated
    fn downsample(&mut self, uuids: Vec<Vec<u8>>) -> PyResult<u32> {
        // For meshes/lbo mode, skip downsample (export does its own)
        if matches!(&self.export_mode, TboExportMode::Meshes | TboExportMode::Lbo) {
            return Ok(uuids.len() as u32);
        }

        let pivot_uuids: Result<Vec<Uuid>, PyErr> = uuids
            .into_iter()
            .map(|bytes| {
                if bytes.len() != Uuid::SIZE {
                    return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                        format!("UUID must be {} bytes, got {}", Uuid::SIZE, bytes.len()),
                    ));
                }
                let mut uuid = Uuid { bytes: [0u8; Uuid::SIZE] };
                uuid.bytes.copy_from_slice(&bytes);
                Ok(uuid)
            })
            .collect();

        let pivot_uuids = pivot_uuids?;
        let count = pivot_uuids.len();

        match engine_api::tbo_downsample_command(pivot_uuids) {
            Ok(resp) => {
                let accumulated = resp.read_tbo_downsample();
                self.accumulated_count += accumulated as u64;
                Ok(accumulated)
            }
            Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                format!("tbo_downsample failed: {}, requested: {}", e, count),
            )),
        }
    }

    /// Drop a batch of UUIDs from the engine scene graph and memory bank.
    ///
    /// Args:
    ///     uuids: List of UUID byte arrays (each 32 bytes)
    fn drop(&self, uuids: Vec<Vec<u8>>) -> PyResult<()> {
        // For meshes/lbo mode, skip drop (export does its own)
        if matches!(&self.export_mode, TboExportMode::Meshes | TboExportMode::Lbo) {
            return Ok(());
        }

        let pivot_uuids: Result<Vec<Uuid>, PyErr> = uuids
            .into_iter()
            .map(|bytes| {
                let mut uuid = Uuid { bytes: [0u8; Uuid::SIZE] };
                uuid.bytes.copy_from_slice(&bytes);
                Ok(uuid)
            })
            .collect();

        let pivot_uuids = pivot_uuids.map_err(|e| e)?;

        engine_api::drop_groups_command(pivot_uuids)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?;

        Ok(())
    }

    /// Flush accumulated downsampled data to .tbo files on disk.
    ///
    /// Returns:
    ///     List of written .tbo filenames
    fn flush(&mut self) -> PyResult<Vec<String>> {
        match &self.export_mode {
            TboExportMode::Points => {
                self.flush_pending()?;
                let batch_offset = self.next_batch_number;
                match engine_api::tbo_flush_command(&self.output_dir, self.target_bytes, batch_offset) {
                    Ok(resp) => {
                        let filenames = resp.read_tbo_flush()
                            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                                format!("Failed to read flush response: {}", e),
                            ))?;
                        let result: Vec<String> = filenames.into_iter().map(|s| s.to_string()).collect();
                        // Update batch offset for next flush
                        self.next_batch_number += result.len() as u32;
                        // Reset accumulated count so needs_flush works correctly for next batch
                        self.accumulated_count = 0;
                        // Drop all groups from scene graph to clear memory
                        engine_api::drop_all_groups_command()
                            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                                format!("drop_all_groups failed: {}", e),
                            ))?;
                        Ok(result)
                    }
                    Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                        format!("tbo_flush failed: {}", e),
                    )),
                }
            }
            TboExportMode::Meshes => {
                match engine_api::export_all_asset_tbo_command(&self.output_dir, self.target_bytes) {
                    Ok(resp) => {
                        let filenames = resp.read_tbo_flush()
                            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                                format!("Failed to read flush response: {}", e),
                            ))?;
                        let result: Vec<String> = filenames.into_iter().map(|s| s.to_string()).collect();
                        // Accumulate actual exported count from engine response
                        self.exported_count += resp.header.num_items as u64;
                        self.accumulated_count = 0;
                        // Drop all groups from scene graph to clear memory
                        engine_api::drop_all_groups_command()
                            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                                format!("drop_all_groups failed: {}", e),
                            ))?;
                        Ok(result)
                    }
                    Err(e) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                        format!("export_all_asset_tbo failed: {}", e),
                    )),
                }
            }
            TboExportMode::Lbo => {
                // Export all assets to LBO format
                engine_api::export_all_command(&self.output_dir, self.target_bytes)
                    .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                        format!("export_all failed: {}", e),
                    ))?;
                
                // Drop all groups from scene graph
                engine_api::drop_all_groups_command()
                    .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                        format!("drop_all_groups failed: {}", e),
                    ))?;
                
                self.accumulated_count = 0;
                Ok(vec![])
            }
        }
    }

    /// Finalize: flush any remaining data and return accumulated count.
    ///
    /// Returns:
    ///     Total number of meshes accumulated during this export session
    fn finalize(&mut self) -> PyResult<u64> {
        // Flush any pending downsample/drop calls
        if !self.pending_downsample.is_empty() {
            self.flush_pending()?;
        }

        // Flush to disk
        let files = self.flush()?;
        
        match &self.export_mode {
            TboExportMode::Points => {
                eprintln!(
                    "[TBO] Final flush: {} files, total meshes: {}",
                    files.len(),
                    self.accumulated_count,
                );
            }
            TboExportMode::Meshes => {
                eprintln!(
                    "[TBO] Final flush: {} files, {} assets exported ({} pushed, {} deduped)",
                    files.len(),
                    self.exported_count,
                    self.pushed_count,
                    self.pushed_count - self.exported_count,
                );
            }
            TboExportMode::Lbo => {
                eprintln!(
                    "[LBO] Final flush: exported all assets"
                );
            }
        }
        
        Ok(self.accumulated_count)
    }

    /// Check if accumulated data exceeds flush threshold.
    #[getter]
    fn needs_flush(&self) -> bool {
        self.accumulated_count >= self.flush_threshold
    }

    /// Get current accumulated mesh count.
    #[getter]
    fn accumulated_count(&self) -> u64 {
        self.accumulated_count
    }

    /// Get total number of assets actually exported to TBO files (after dedup).
    #[getter]
    fn exported_count(&self) -> u64 {
        self.exported_count
    }

    /// Get total number of assets pushed to engine (before dedup).
    #[getter]
    fn pushed_count(&self) -> u64 {
        self.pushed_count
    }

    /// Get flush threshold (meshes per flush).
    #[getter]
    fn flush_threshold(&self) -> u64 {
        self.flush_threshold
    }

    /// Get number of pending UUIDs waiting to be flushed.
    #[getter]
    fn pending_count(&self) -> usize {
        self.pending_downsample.len()
    }
}
