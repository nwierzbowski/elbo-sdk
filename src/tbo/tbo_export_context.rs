//! Streaming TBO export context (pyo3).
//!
//! Each enabled format (scene/asset/fragment) owns one iceoryx slab laid out as
//! `[24-byte header][channel names][padding][data: grows →] … [offsets: ← grows]`.
//! The engine fills the slabs during a `tbo_export_command` round-trip
//! ([`TboExportContext::do_flush`]); geometry reaches the engine through
//! `prepare_mesh_send` + `accumulate` (allocate-memory path).

use std::sync::Arc;

use pyo3::prelude::*;

use crate::asset_sync_context::AssetSyncContext;
use crate::engine_api;

use super::buffer::{self, FormatBuffer, ShmKeep};
use super::format;
use super::tbo_collection::{DataBacking, FormatPair};
use super::tbo_data_view::{ChannelSet, DataView};
use super::tbo_hierarchy::{build_hierarchy, TBOHierarchy};
use super::tbo_writer;
use super::{py_runtime_error, py_value_error};

/// Identity of one export format, in the engine/protocol order
/// `[scene, asset, fragment, points, faces]` (slab arrays and response counts match it).
#[derive(Clone, Copy, PartialEq, Eq)]
enum FormatKey {
    Scene,
    Asset,
    Fragment,
    Points,
    Faces,
}

impl FormatKey {
    const ALL: [FormatKey; 5] = [FormatKey::Scene, FormatKey::Asset, FormatKey::Fragment, FormatKey::Points, FormatKey::Faces];

    /// Engine overflow-response status value identifying this format.
    fn from_overflow_status(status: u16) -> Option<Self> {
        match status {
            1 => Some(FormatKey::Scene),
            2 => Some(FormatKey::Asset),
            3 => Some(FormatKey::Fragment),
            4 => Some(FormatKey::Points),
            5 => Some(FormatKey::Faces),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            FormatKey::Scene => "scene",
            FormatKey::Asset => "asset",
            FormatKey::Fragment => "fragment",
            FormatKey::Points => "points",
            FormatKey::Faces => "faces",
        }
    }

    /// `format_index` stored in the TBO header.
    fn index(self) -> u32 {
        self as u32
    }
}

/// One format's export slot: its slab name and (if enabled) live buffer.
struct FormatSlot {
    buf: Option<FormatBuffer>,
    name: [u8; 64],
}

impl FormatSlot {
    fn inactive() -> Self {
        Self {
            buf: None,
            name: [0u8; 64],
        }
    }
}

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
    points_original: bool,
    faces: bool,
    target_point_count: u32,
    /// Bytes queued via `accumulate` since the last flush.
    accumulated_bytes: u64,
    /// Auto-flush threshold derived from `max_memory_mb`.
    flush_threshold: u64,
    /// Slots in [scene, asset, fragment, points, faces] order (see [`FormatKey::ALL`]).
    slots: [FormatSlot; 5],
    /// Allocate-memory batch staged by `prepare_mesh_send`, sent on `accumulate`.
    pending_asset_ctx: Option<AssetSyncContext>,
    pending_allocated_bytes: u64,
}

impl TboExportContext {
    fn enabled(&self, key: FormatKey) -> bool {
        match key {
            FormatKey::Scene => self.scene_transform || self.scene_similarity,
            FormatKey::Asset => self.asset_embedding || self.asset_transform,
            FormatKey::Fragment => {
                self.fragment_xyz || self.normal_variance || self.surface_variation || self.combined
            }
            FormatKey::Points => self.points_original,
            FormatKey::Faces => self.faces,
        }
    }

    /// Channel layout for a format, from this context's enable flags.
    fn resolve_channel_names(&self, key: FormatKey) -> Vec<String> {
        let mut names = Vec::new();
        match key {
            FormatKey::Scene => {
                if self.scene_transform {
                    for i in 0..16 {
                        names.push(format!("trans_{:02}", i));
                    }
                }
                if self.scene_similarity {
                    names.push("similarity".to_string());
                }
            }
            FormatKey::Asset => {
                if self.asset_embedding {
                    for i in 0..256 {
                        names.push(format!("emb_{:03}", i));
                    }
                }
                if self.asset_transform {
                    for i in 0..16 {
                        names.push(format!("trans_{:02}", i));
                    }
                }
            }
            FormatKey::Fragment => {
                if self.fragment_xyz {
                    names.push("xyz_x".to_string());
                    names.push("xyz_y".to_string());
                    names.push("xyz_z".to_string());
                }
                if self.normal_variance {
                    names.push("normal_variance".to_string());
                }
                if self.surface_variation {
                    names.push("surface_variation".to_string());
                }
                if self.combined {
                    names.push("combined".to_string());
                }
            }
            FormatKey::Points => {
                if self.points_original {
                    names.push("vert_x".to_string());
                    names.push("vert_y".to_string());
                    names.push("vert_z".to_string());
                }
            }
            FormatKey::Faces => {
                if self.faces {
                    names.push("face_i0".to_string());
                    names.push("face_i1".to_string());
                    names.push("face_i2".to_string());
                }
            }
        }
        names
    }

    /// Create the slab and initial buffer for one enabled format.
    fn enable(&mut self, key: FormatKey, buffer_size: usize) -> PyResult<()> {
        if !self.enabled(key) {
            return Ok(());
        }
        let name = buffer::unique_slab_name(key.name());
        let channels = ChannelSet::from_names(self.resolve_channel_names(key));
        let shm = buffer::create_shm(&name, buffer_size).map_err(py_runtime_error)?;
        let buf = FormatBuffer::new(shm, buffer_size, key.index(), channels).map_err(py_runtime_error)?;
        self.slots[key as usize] = FormatSlot {
            buf: Some(buf),
            name,
        };
        Ok(())
    }

    fn slot(&self, key: FormatKey) -> &FormatSlot {
        &self.slots[key as usize]
    }

    fn slot_mut(&mut self, key: FormatKey) -> &mut FormatSlot {
        &mut self.slots[key as usize]
    }

    /// One engine round-trip: send the export command (the engine blocks
    /// until it has written the queued entities) and interpret the response.
    fn run_export_command(&self, py: Python) -> PyResult<(u16, EngineCounts)> {
        let mut data = [0u64; 5];
        let mut offset = [0u64; 5];
        let mut remaining = [0u64; 5];
        for (i, slot) in self.slots.iter().enumerate() {
            if let Some(buf) = &slot.buf {
                // Slab-relative byte offsets, as the engine expects.
                data[i] = buf.data_ptr as u64;
                offset[i] = buf.offset_ptr() as u64;
                remaining[i] = buf.remaining() as u64;
            }
        }

        // Everything the closure needs copied into locals: `detach` runs it
        // off-thread, so it must not capture the context (SHM holders are !Send).
        let names: [[u8; 64]; 5] = [
            self.slots[0].name,
            self.slots[1].name,
            self.slots[2].name,
            self.slots[3].name,
            self.slots[4].name,
        ];
        let (
            scene_transform,
            scene_similarity,
            asset_embedding,
            asset_transform,
            fragment_xyz,
            normal_variance,
            surface_variation,
            combined,
            points_original,
            faces,
            target_point_count,
        ) = (
            self.scene_transform,
            self.scene_similarity,
            self.asset_embedding,
            self.asset_transform,
            self.fragment_xyz,
            self.normal_variance,
            self.surface_variation,
            self.combined,
            self.points_original,
            self.faces,
            self.target_point_count,
        );

        let resp = py
            .detach(|| {
                engine_api::tbo_export_command(
                    &names[0], &names[1], &names[2], &names[3], &names[4],
                    data[0], offset[0], remaining[0],
                    data[1], offset[1], remaining[1],
                    data[2], offset[2], remaining[2],
                    data[3], offset[3], remaining[3],
                    data[4], offset[4], remaining[4],
                    scene_transform,
                    scene_similarity,
                    asset_embedding,
                    asset_transform,
                    fragment_xyz,
                    normal_variance,
                    surface_variation,
                    combined,
                    points_original,
                    faces,
                    target_point_count,
                )
            })
            .map_err(py_runtime_error)?;

        let status = resp.header.status;
        let (sc, ac, fc, pc, fac, sb, ab, fb, pb, facb) = resp
            .read_tbo_export_response()
            .map_err(|e| py_runtime_error(format!("Failed to read export response: {e}")))?;
        Ok((status, EngineCounts {
            counts: [sc, ac, fc, pc, fac],
            reported_bytes: [sb, ab, fb, pb, facb],
        }))
    }

    /// Flush to disk in a loop until the engine reports all queued points done.
    /// Returns any intermediate .tbo files written on overflow.
    fn do_flush(&mut self, py: Python) -> PyResult<Vec<(String, Vec<String>)>> {
        loop {
            let (status, r) = self.run_export_command(py)?;
            if status != 0 {
                let key = FormatKey::from_overflow_status(status)
                    .ok_or_else(|| py_runtime_error(format!("Unknown overflow status: {status}")))?;
                let flushed = self.flush_format_to_disk(key)?;
                if flushed.is_empty() {
                    return Err(py_runtime_error(format!(
                        "Engine reported {} buffer overflow but the buffer holds no flushed data; \
                         the export buffer is too small for a single batch",
                        key.name()
                    )));
                }
                continue;
            }

            let mut data_bytes = [0u64; 3];
            for (i, key) in FormatKey::ALL.iter().enumerate() {
                data_bytes[i] = r.reported_bytes[i]
                    .checked_sub(r.counts[i] * 8)
                    .ok_or_else(|| {
                        py_runtime_error(format!(
                            "{} buffer: reported bytes less than offset space",
                            key.name()
                        ))
                    })?;
                if let Some(buf) = self.slot_mut(*key).buf.as_mut() {
                    buf.advance(data_bytes[i], r.counts[i]).map_err(py_runtime_error)?;
                }
            }

            py.detach(engine_api::drop_all_groups_command)
                .map_err(py_runtime_error)?;
            self.accumulated_bytes = 0;
            return Ok(vec![]);
        }
    }

    /// Flush one format's buffer to a .tbo file and reset it. Returns the
    /// files written (empty when the buffer holds no data).
    fn flush_format_to_disk(&mut self, key: FormatKey) -> PyResult<Vec<(String, Vec<String>)>> {
        let buf = self
            .slot(key)
            .buf
            .as_ref()
            .ok_or_else(|| py_runtime_error(format!("No {} buffer", key.name())))?;
        let base = buf.shm.base_address().as_ptr() as usize;
        if buf.is_empty() {
            return Ok(vec![]);
        }

        format::set_entity_count(base as *mut u8, buf.entity_count() as u32);
        let filename = tbo_writer::write_tbo_file(
            unsafe { std::slice::from_raw_parts(base as *const u8, buf.buffer_size) },
            buf.data_ptr,
            buf.offset_region_start(),
            &self.output_dir,
            key.name(),
        )
        .map_err(py_runtime_error)?;

        self.slot_mut(key)
            .buf
            .as_mut()
            .expect("buffer checked above")
            .reset();

        Ok(vec![(key.name().to_string(), vec![filename])])
    }

    /// Zero-copy views of one format's buffer plus an independent keepalive
    /// mapping. Disabled formats yield empty views.
    fn format_views(&self, key: FormatKey) -> PyResult<FormatPair> {
        let Some(buf) = self.slot(key).buf.as_ref() else {
            return Ok((Vec::new(), Vec::new()));
        };
        let base = buf.shm.base_address().as_ptr() as usize;
        let view = DataView::new(
            (base + buf.data_start) as *const f32,
            buf.data_len_f32(),
            (base + buf.offset_region_start()) as *const u64,
            buf.offset_len(),
            buf.data_start as u64,
            buf.channels.clone(),
        );
        let mapping = buffer::open_shm_mapping(&self.slot(key).name).map_err(py_runtime_error)?;
        // Not Sync: the backing is only ever read on the (single) Python thread.
        #[allow(clippy::arc_with_non_send_sync)]
        let backing: DataBacking = Arc::new(ShmKeep(mapping));
        Ok((vec![view], vec![backing]))
    }
}

/// Entity counts and total reported bytes (data + offsets) per format,
/// in [scene, asset, fragment, points, faces] order.
struct EngineCounts {
    counts: [u64; 5],
    reported_bytes: [u64; 5],
}

#[pymethods]
impl TboExportContext {
    /// Create a streaming export context. Enabled formats get a live buffer of
    /// `target_export_size_mb` each; `max_memory_mb` is the auto-flush threshold.
    #[new]
    #[allow(clippy::too_many_arguments)]
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
        points_original,
        faces,
        max_memory_mb,
        target_export_size_mb,
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
        points_original: bool,
        faces: bool,
        max_memory_mb: f64,
        target_export_size_mb: f64,
        target_point_count: u32,
    ) -> PyResult<Self> {
        let buffer_size = (target_export_size_mb * 1024.0 * 1024.0) as usize;
        let flush_threshold = (max_memory_mb * 1024.0 * 1024.0) as u64;

        eprintln!(
            "[TBO] Config: scene_transform={}, scene_similarity={}, asset_embedding={}, asset_transform={}, fragment_xyz={}, normal_variance={}, surface_variation={}, combined={}, points_original={}, faces={}, max_memory={} MB, buffer_size={} MB, target_points={}",
            scene_transform, scene_similarity, asset_embedding, asset_transform,
            fragment_xyz, normal_variance, surface_variation, combined,
            points_original, faces,
            max_memory_mb, target_export_size_mb, target_point_count
        );

        let mut ctx = Self {
            output_dir,
            scene_transform,
            scene_similarity,
            asset_embedding,
            asset_transform,
            fragment_xyz,
            normal_variance,
            surface_variation,
            combined,
            points_original,
            faces,
            target_point_count,
            accumulated_bytes: 0,
            flush_threshold,
            slots: [
                FormatSlot::inactive(),
                FormatSlot::inactive(),
                FormatSlot::inactive(),
                FormatSlot::inactive(),
                FormatSlot::inactive(),
            ],
            pending_asset_ctx: None,
            pending_allocated_bytes: 0,
        };
        for key in FormatKey::ALL {
            ctx.enable(key, buffer_size)?;
        }
        Ok(ctx)
    }

    /// Stage an allocate-memory batch for the engine; `accumulate()` sends it.
    /// Returns 11 Python buffers to fill with the geometry.
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
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
        asset_uuids: Vec<pivot_com_types::fields::Uuid>,
    ) -> PyResult<(
        Py<PyAny>, Py<PyAny>, Py<PyAny>, Py<PyAny>, Py<PyAny>,
        Py<PyAny>, Py<PyAny>, Py<PyAny>, Py<PyAny>, Py<PyAny>, Py<PyAny>,
    )> {
        if self.pending_asset_ctx.is_some() {
            return Err(py_value_error(
                "pending asset batch not consumed; call accumulate() before the next prepare_mesh_send()",
            ));
        }

        let (asset_ctx, allocated_bytes) = py
            .detach(|| {
                engine_api::allocate_memory(
                    vert_counts, edge_counts, loop_counts, total_loop_lengths,
                    object_counts, group_names, surface_contexts, asset_uuids,
                )
            })
            .map_err(py_runtime_error)?;

        self.pending_asset_ctx = Some(asset_ctx);
        self.pending_allocated_bytes = allocated_bytes;

        self.pending_asset_ctx
            .as_ref()
            .expect("pending context just set")
            .buffers(py, 0)
    }

    /// Send the staged batch and drive a TBO export of everything queued.
    /// Flushes to disk when `flush` is set or the memory threshold is reached.
    #[pyo3(signature = (flush = false))]
    fn accumulate(&mut self, py: Python, flush: bool) -> PyResult<Vec<(String, Vec<String>)>> {
        if let Some(mut asset_ctx) = self.pending_asset_ctx.take() {
            asset_ctx.send_command(py)?;
        }

        self.accumulated_bytes += self.pending_allocated_bytes;
        self.pending_allocated_bytes = 0;

        if flush || self.accumulated_bytes >= self.flush_threshold {
            self.do_flush(py)
        } else {
            Ok(vec![])
        }
    }

    /// Final flush: drive remaining exports and write all non-empty buffers to
    /// disk. Returns the files written per format.
    fn close(&mut self, py: Python) -> PyResult<Vec<(String, Vec<String>)>> {
        // No data written -> engine has nothing to export; skip the
        // blocking round-trip entirely.
        let any_data = self
            .slots
            .iter()
            .any(|s| s.buf.as_ref().is_some_and(|b| !b.is_empty()));
        let mut all_flushed = if any_data { self.do_flush(py)? } else { vec![] };

        for key in FormatKey::ALL {
            if self.slot(key).buf.is_some() {
                all_flushed.extend(self.flush_format_to_disk(key)?);
            }
        }
        Ok(all_flushed)
    }

    /// Build a Scene->Asset->Fragment hierarchy over the current buffer contents.
    /// Views are zero-copy into shared memory; each format's backing stays alive
    /// via its own mapping, independent of this context.
    fn get_hierarchy(&self, _py: Python) -> PyResult<TBOHierarchy> {
        build_hierarchy(
            self.format_views(FormatKey::Scene)?,
            self.format_views(FormatKey::Asset)?,
            self.format_views(FormatKey::Fragment)?,
        )
    }
}
