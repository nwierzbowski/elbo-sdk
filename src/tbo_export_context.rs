//! TBO Export Context - SDK-side orchestrator for streaming export.
//!
//! Buffer layout: [header][data: grows →] ... gap ... [offsets: ← grows]
//! Engine writes raw f32 data at data_ptr and u64 offsets at offset_ptr.
//! Context writes header (magic, channel names, entity count) at flush time.

use pyo3::prelude::*;

use crate::asset_sync_context::AssetSyncContext;
use crate::engine_api;
use crate::tbo_writer;
use iceoryx2::prelude::{FileName, SemanticString};
use iceoryx2_bb_posix::file::CreationMode;
use iceoryx2_bb_posix::shared_memory::{SharedMemory, SharedMemoryBuilder};

macro_rules! slab_name {
    ($name:literal) => {{
        let mut buf = [0u8; 64];
        let bytes = $name.as_bytes();
        let len = bytes.len().min(63);
        buf[..len].copy_from_slice(&bytes[..len]);
        buf
    }};
}

/// Export format configuration (kept for backward compatibility).
#[pyclass]
#[derive(Clone)]
pub struct ExportFormat {
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

struct FormatBuffer {
    shm: SharedMemory,
    data_ptr: usize,
    offset_ptr: usize,
    remaining: usize,
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
    target_point_count: u32,
    accumulated_bytes: u64,
    flush_threshold: u64,
    scene_buf: Option<FormatBuffer>,
    asset_buf: Option<FormatBuffer>,
    fragment_buf: Option<FormatBuffer>,
    slab_scene_name: [u8; 64],
    slab_asset_name: [u8; 64],
    slab_fragment_name: [u8; 64],
    pending_asset_ctx: Option<AssetSyncContext>,
    pending_allocated_bytes: u64,
}

fn create_shm(name: &[u8; 64], size: usize) -> Result<SharedMemory, String> {
    let len = name.iter().position(|&b| b == 0).unwrap_or(64);
    let name_str = &name[..len];
    let file_name = FileName::new(name_str)
        .map_err(|e| format!("Invalid SHM name: {:?}", e))?;
    SharedMemoryBuilder::new(&file_name)
        .is_memory_locked(false)
        .creation_mode(CreationMode::PurgeAndCreate)
        .size(size)
        .create()
        .map_err(|e| format!("Failed to create SHM: {:?}", e))
}

#[pymethods]
impl TboExportContext {
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
        max_memory_mb: f64,
        target_export_size_mb: f64,
        target_point_count: u32,
    ) -> PyResult<Self> {
        let buffer_size = (target_export_size_mb * 1024.0 * 1024.0) as usize;
        let flush_threshold = (max_memory_mb * 1024.0 * 1024.0) as u64;

        eprintln!(
            "[TBO] Config: scene_transform={}, scene_similarity={}, asset_embedding={}, asset_transform={}, fragment_xyz={}, normal_variance={}, surface_variation={}, combined={}, max_memory={} MB, buffer_size={} MB, target_points={}",
            scene_transform, scene_similarity, asset_embedding, asset_transform,
            fragment_xyz, normal_variance, surface_variation, combined,
            max_memory_mb, target_export_size_mb, target_point_count
        );

        let slab_scene_name = slab_name!("tbo_scene");
        let slab_asset_name = slab_name!("tbo_asset");
        let slab_fragment_name = slab_name!("tbo_fragment");

        let make_buf = |enabled: bool, name: &[u8; 64]| -> Result<Option<FormatBuffer>, String> {
            if !enabled {
                return Ok(None);
            }
            let shm = create_shm(name, buffer_size)?;
            Ok(Some(FormatBuffer {
                shm,
                data_ptr: 0,
                offset_ptr: buffer_size,
                remaining: buffer_size,
            }))
        };

        let scene_buf = make_buf(scene_transform || scene_similarity, &slab_scene_name)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?;
        let asset_buf = make_buf(asset_embedding || asset_transform, &slab_asset_name)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?;
        let fragment_buf = make_buf(fragment_xyz || normal_variance || surface_variation || combined, &slab_fragment_name)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?;

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
            target_point_count,
            accumulated_bytes: 0,
            flush_threshold,
            scene_buf,
            asset_buf,
            fragment_buf,
            slab_scene_name,
            slab_asset_name,
            slab_fragment_name,
            pending_asset_ctx: None,
            pending_allocated_bytes: 0,
        })
    }

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
        let (asset_ctx, allocated_bytes) = crate::engine_api::allocate_memory(
            vert_counts, edge_counts, loop_counts, total_loop_lengths,
            object_counts, group_names, surface_contexts, asset_uuids.clone(),
        )
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?;

        self.pending_asset_ctx = Some(asset_ctx);
        self.pending_allocated_bytes = allocated_bytes;

        if let Some(ref mut ctx) = self.pending_asset_ctx {
            ctx.buffers(py, 0)
        } else {
            unreachable!()
        }
    }

    #[pyo3(signature = (flush = false))]
    fn accumulate(&mut self, flush: bool) -> PyResult<Vec<(String, Vec<String>)>> {
        if let Some(mut asset_ctx) = self.pending_asset_ctx.take() {
            asset_ctx.send();
        }

        self.accumulated_bytes += self.pending_allocated_bytes;
        self.pending_allocated_bytes = 0;

        if flush || self.accumulated_bytes >= self.flush_threshold {
            self.do_flush().map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))
        } else {
            Ok(vec![])
        }
    }

    fn close(&mut self) -> PyResult<Vec<(String, Vec<String>)>> {
        let mut all_flushed = self.do_flush().map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?;
        // Flush only formats that have an active buffer
        if self.scene_buf.is_some() {
            all_flushed.extend(self.flush_format_to_disk("scene")
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?);
        }
        if self.asset_buf.is_some() {
            all_flushed.extend(self.flush_format_to_disk("asset")
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?);
        }
        if self.fragment_buf.is_some() {
            all_flushed.extend(self.flush_format_to_disk("fragment")
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?);
        }
        Ok(all_flushed)
    }
}

impl TboExportContext {
    fn do_flush(&mut self) -> Result<Vec<(String, Vec<String>)>, String> {
        loop {
            let s_dp = self.scene_buf.as_ref().map(|b| b.data_ptr as u64).unwrap_or(0);
            let s_op = self.scene_buf.as_ref().map(|b| b.offset_ptr as u64).unwrap_or(0);
            let s_rem = self.scene_buf.as_ref().map(|b| b.remaining as u64).unwrap_or(0);
            let a_dp = self.asset_buf.as_ref().map(|b| b.data_ptr as u64).unwrap_or(0);
            let a_op = self.asset_buf.as_ref().map(|b| b.offset_ptr as u64).unwrap_or(0);
            let a_rem = self.asset_buf.as_ref().map(|b| b.remaining as u64).unwrap_or(0);
            let f_dp = self.fragment_buf.as_ref().map(|b| b.data_ptr as u64).unwrap_or(0);
            let f_op = self.fragment_buf.as_ref().map(|b| b.offset_ptr as u64).unwrap_or(0);
            let f_rem = self.fragment_buf.as_ref().map(|b| b.remaining as u64).unwrap_or(0);

            match engine_api::tbo_export_command(
                &self.slab_scene_name, &self.slab_asset_name, &self.slab_fragment_name,
                s_dp, s_op, s_rem,
                a_dp, a_op, a_rem,
                f_dp, f_op, f_rem,
                self.scene_transform, self.scene_similarity,
                self.asset_embedding, self.asset_transform,
                self.fragment_xyz, self.normal_variance,
                self.surface_variation, self.combined,
                self.target_point_count,
            ) {
                Ok(resp) => {
                    let status = resp.header.status;
                    let (s_count, a_count, f_count, s_bytes, a_bytes, f_bytes) =
                        resp.read_tbo_export_response()
                            .map_err(|e| format!("Failed to read export response: {}", e))?;

                    if status != 0 {
                        let buffer_to_flush = match status {
                            1 => "scene",
                            2 => "asset",
                            3 => "fragment",
                            _ => return Err(format!("Unknown overflow status: {}", status)),
                        };
                        let flushed = self.flush_format_to_disk(buffer_to_flush)?;
                        if !flushed.is_empty() {
                            return Ok(flushed);
                        }
                        continue;
                    }

                    let s_data_bytes = s_bytes - (s_count * 8);
                    let a_data_bytes = a_bytes - (a_count * 8);
                    let f_data_bytes = f_bytes - (f_count * 8);

                    if let Some(ref mut buf) = self.scene_buf {
                        buf.data_ptr += s_data_bytes as usize;
                        buf.offset_ptr -= (s_count as usize) * 8;
                        buf.remaining -= s_bytes as usize;
                    }
                    if let Some(ref mut buf) = self.asset_buf {
                        buf.data_ptr += a_data_bytes as usize;
                        buf.offset_ptr -= (a_count as usize) * 8;
                        buf.remaining -= a_bytes as usize;
                    }
                    if let Some(ref mut buf) = self.fragment_buf {
                        buf.data_ptr += f_data_bytes as usize;
                        buf.offset_ptr -= (f_count as usize) * 8;
                        buf.remaining -= f_bytes as usize;
                    }

                    engine_api::drop_all_groups_command()?;
                    self.accumulated_bytes = 0;
                    return Ok(vec![]);
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn flush_format_to_disk(&mut self, format_name: &str) -> Result<Vec<(String, Vec<String>)>, String> {
        // Read buffer state into local variables to avoid borrow issues
        let (base, data_ptr, offset_ptr, buf_size) = match format_name {
            "scene" => {
                let buf = self.scene_buf.as_ref().ok_or("No scene buffer")?;
                (buf.shm.base_address().as_ptr() as usize, buf.data_ptr, buf.offset_ptr, buf.shm.size())
            }
            "asset" => {
                let buf = self.asset_buf.as_ref().ok_or("No asset buffer")?;
                (buf.shm.base_address().as_ptr() as usize, buf.data_ptr, buf.offset_ptr, buf.shm.size())
            }
            "fragment" => {
                let buf = self.fragment_buf.as_ref().ok_or("No fragment buffer")?;
                (buf.shm.base_address().as_ptr() as usize, buf.data_ptr, buf.offset_ptr, buf.shm.size())
            }
            _ => return Ok(vec![]),
        };

        if data_ptr == 0 {
            return Ok(vec![]);
        }

        let channel_names = self.resolve_channel_names(format_name);

        // Read offsets (growing left from offset_ptr)
        let offset_count = (buf_size - offset_ptr) / 8;
        let mut offsets = Vec::with_capacity(offset_count);
        for i in (0..offset_count).rev() {
            let off = unsafe {
                let val = *((base + offset_ptr + i * 8) as *const u64);
                u64::from_le(val)
            };
            offsets.push(off);
        }

        // Read data (from base to data_ptr)
        let data_len = data_ptr / 4;
        let data_slice = unsafe {
            std::slice::from_raw_parts(base as *const f32, data_len)
        };

        // Compute file offsets (engine stores byte positions from data_start, which is 0 after reset)
        let file_header_size = 20u64 + channel_names.iter().map(|n| n.len() as u64 + 1).sum::<u64>();
        let file_offsets: Vec<u64> = offsets.iter()
            .map(|&o| file_header_size + o)
            .collect();

        let filename = tbo_writer::write_tbo_file(
            data_slice, &file_offsets, &channel_names,
            &self.output_dir, format_name,
        )?;

        // Reset buffer
        match format_name {
            "scene" => {
                let buf = self.scene_buf.as_mut().unwrap();
                buf.data_ptr = 0;
                buf.offset_ptr = buf_size;
                buf.remaining = buf_size;
            }
            "asset" => {
                let buf = self.asset_buf.as_mut().unwrap();
                buf.data_ptr = 0;
                buf.offset_ptr = buf_size;
                buf.remaining = buf_size;
            }
            "fragment" => {
                let buf = self.fragment_buf.as_mut().unwrap();
                buf.data_ptr = 0;
                buf.offset_ptr = buf_size;
                buf.remaining = buf_size;
            }
            _ => {}
        }

        Ok(vec![(format_name.to_string(), vec![filename])])
    }

    fn resolve_channel_names(&self, format_name: &str) -> Vec<String> {
        let mut names = Vec::new();
        match format_name {
            "scene" => {
                if self.scene_transform {
                    for i in 0..16 {
                        names.push(format!("trans_{:02}", i));
                    }
                }
                if self.scene_similarity {
                    names.push("similarity".to_string());
                }
            }
            "asset" => {
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
            "fragment" => {
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
            _ => {}
        }
        names
    }
}
