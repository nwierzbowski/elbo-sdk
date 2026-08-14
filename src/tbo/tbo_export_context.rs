//! TBO Export Context - SDK-side orchestrator for streaming export.
//!
//! Buffer layout: [24-byte header][channel names][data: grows →] ... gap ... [offsets: ← grows]
//! Header: [4: magic][4: format_index][4: version][4: flags][4: entity_count][4: channel_count]
//! Engine writes raw f32 data at data_ptr and u64 offsets at offset_ptr.
//! The LoadedFile structs ARE the buffer handles — created on open, updated on each write,
//! and always accessible for native data access.

use pyo3::prelude::*;

use crate::asset_sync_context::AssetSyncContext;
use crate::engine_api;
use super::tbo_file::LoadedFile;
use super::tbo_data_view::DataView;
use super::tbo_collection::CollectionState;
use super::tbo_writer;
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

struct FormatBuffer {
    shm: SharedMemory,
    data_ptr: usize,
    offset_ptr: usize,
    data_start: usize,
    buffer_size: usize,
    remaining: usize,
}

// ── TboExportContext ─────────────────────────────────────────────────────────

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
    scene_file: Option<LoadedFile>,
    asset_file: Option<LoadedFile>,
    fragment_file: Option<LoadedFile>,
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

fn create_loaded_file(
    format_name: &str,
    format_index: u32,
    channel_names: Vec<String>,
    _buf: &FormatBuffer,
) -> LoadedFile {
    let channel_count = channel_names.len() as u32;
    LoadedFile {
        path: format_name.to_string(),
        data_len: 0,
        offset_len: 0,
        channel_names: channel_names.clone(),
        format_index,
        version: 0,
        flags: 0,
        entity_count: 0,
        channel_count,
        data_holder_index: None,
    }
}

fn make_buf(
    buffer_size: usize,
    name: &[u8; 64],
    channel_names: &[String],
    format_index: u32,
    format_name: &str,
) -> Result<(FormatBuffer, LoadedFile), String> {
    let shm = create_shm(name, buffer_size)?;
    let header_size = 24;
    let names_size: usize = channel_names.iter().map(|n| n.len() + 1).sum();
    let data_start = header_size + names_size;

    let mut fmt_buf = FormatBuffer {
        shm,
        data_ptr: data_start,
        offset_ptr: buffer_size,
        data_start,
        buffer_size,
        remaining: buffer_size - data_start,
    };

    let mut loaded_file = create_loaded_file(
        match format_index {
            0 => "scene",
            1 => "asset",
            2 => "fragment",
            _ => "unknown",
        },
        format_index,
        channel_names.to_vec(),
        &fmt_buf,
    );

    reset_format(&mut fmt_buf, &mut loaded_file, format_name);

    Ok((fmt_buf, loaded_file))
}

fn reset_format(buf: &mut FormatBuffer, file: &mut LoadedFile, format_name: &str) {
    buf.data_ptr = buf.data_start;
    buf.offset_ptr = buf.buffer_size;
    buf.remaining = buf.buffer_size - buf.data_start;

    let format_index = match format_name {
        "scene" => 0,
        "asset" => 1,
        "fragment" => 2,
        _ => return,
    };

    let base = buf.shm.base_address().as_ptr() as usize;
    unsafe {
        let ptr = base as *mut u8;
        std::ptr::write_unaligned(ptr.add(0) as *mut u32, 0x004F4254);
        std::ptr::write_unaligned(ptr.add(4) as *mut u32, format_index);
        std::ptr::write_unaligned(ptr.add(8) as *mut u32, 3);
        std::ptr::write_unaligned(ptr.add(12) as *mut u32, 0);
        std::ptr::write_unaligned(ptr.add(16) as *mut u32, 0);
        std::ptr::write_unaligned(ptr.add(20) as *mut u32, file.channel_names.len() as u32);
        let mut offset = 24;
        for name in &file.channel_names {
            let bytes = name.as_bytes();
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.add(offset), bytes.len());
            offset += bytes.len();
            *ptr.add(offset) = 0;
            offset += 1;
        }
        // Write first offset (data_start) and decrement offset_ptr
        *((base + buf.buffer_size - 8) as *mut u64) = buf.data_start as u64;
        buf.offset_ptr -= 8;
    }
    file.data_len = 0;
    file.offset_len = 0;
    file.entity_count = 0;
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

        // Build self incrementally so we can call resolve_channel_names
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
            target_point_count,
            accumulated_bytes: 0,
            flush_threshold,
            scene_buf: None,
            asset_buf: None,
            fragment_buf: None,
            slab_scene_name,
            slab_asset_name,
            slab_fragment_name,
            pending_asset_ctx: None,
            pending_allocated_bytes: 0,
            scene_file: None,
            asset_file: None,
            fragment_file: None,
        };

        // Create scene buffer with correct channel names
        if ctx.scene_transform || ctx.scene_similarity {
            let channel_names = ctx.resolve_channel_names("scene");
            let (buf, file) = make_buf(buffer_size, &ctx.slab_scene_name, &channel_names, 0, "scene")
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?;
            ctx.scene_buf = Some(buf);
            ctx.scene_file = Some(file);
        }

        // Create asset buffer with correct channel names
        if ctx.asset_embedding || ctx.asset_transform {
            let channel_names = ctx.resolve_channel_names("asset");
            let (buf, file) = make_buf(buffer_size, &ctx.slab_asset_name, &channel_names, 1, "asset")
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?;
            ctx.asset_buf = Some(buf);
            ctx.asset_file = Some(file);
        }

        // Create fragment buffer with correct channel names
        if ctx.fragment_xyz || ctx.normal_variance || ctx.surface_variation || ctx.combined {
            let channel_names = ctx.resolve_channel_names("fragment");
            let (buf, file) = make_buf(buffer_size, &ctx.slab_fragment_name, &channel_names, 2, "fragment")
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?;
            ctx.fragment_buf = Some(buf);
            ctx.fragment_file = Some(file);
        }

        Ok(ctx)
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

    /// Build a TBOHierarchy linking scene, asset, and fragment collections.
    fn get_hierarchy(&self, _py: Python) -> PyResult<super::tbo_hierarchy::TBOHierarchy> {
        let scenes = self.scene_file.as_ref()
            .map(|file| {
                let buf = self.scene_buf.as_ref().unwrap();
                let base = buf.shm.base_address().as_ptr() as usize;
                let data_ptr = (base + buf.data_start) as *const f32;
                let offsets_ptr = (base + buf.data_start + file.data_len * 4) as *const u64;
                DataView::new(
                    data_ptr,
                    file.data_len,
                    offsets_ptr,
                    file.offset_len,
                    buf.data_start as u64,
                    file.channel_names.clone(),
                )
            });
        let assets = self.asset_file.as_ref()
            .map(|file| {
                let buf = self.asset_buf.as_ref().unwrap();
                let base = buf.shm.base_address().as_ptr() as usize;
                let data_ptr = (base + buf.data_start) as *const f32;
                let offsets_ptr = (base + buf.data_start + file.data_len * 4) as *const u64;
                DataView::new(
                    data_ptr,
                    file.data_len,
                    offsets_ptr,
                    file.offset_len,
                    buf.data_start as u64,
                    file.channel_names.clone(),
                )
            });
        let fragments = self.fragment_file.as_ref()
            .map(|file| {
                let buf = self.fragment_buf.as_ref().unwrap();
                let base = buf.shm.base_address().as_ptr() as usize;
                let data_ptr = (base + buf.data_start) as *const f32;
                let offsets_ptr = (base + buf.data_start + file.data_len * 4) as *const u64;
                DataView::new(
                    data_ptr,
                    file.data_len,
                    offsets_ptr,
                    file.offset_len,
                    buf.data_start as u64,
                    file.channel_names.clone(),
                )
            });

        Ok(super::tbo_hierarchy::TBOHierarchy::new(
            scenes.map(|v| CollectionState::build(vec![v])).unwrap_or_default(),
            assets.map(|v| CollectionState::build(vec![v])).unwrap_or_default(),
            fragments.map(|v| CollectionState::build(vec![v])).unwrap_or_default(),
        ))
    }
}

impl TboExportContext {
    fn get_format_buf(&self, format_name: &str) -> Result<&FormatBuffer, String> {
        match format_name {
            "scene" => self.scene_buf.as_ref().ok_or("No scene buffer".to_string()),
            "asset" => self.asset_buf.as_ref().ok_or("No asset buffer".to_string()),
            "fragment" => self.fragment_buf.as_ref().ok_or("No fragment buffer".to_string()),
            _ => Err("Unknown format".to_string()),
        }
    }

    fn reset_format(&mut self, format_name: &str) {
        match format_name {
            "scene" => {
                let buf = self.scene_buf.as_mut().unwrap();
                let file = self.scene_file.as_mut().unwrap();
                reset_format(buf, file, "scene");
            }
            "asset" => {
                let buf = self.asset_buf.as_mut().unwrap();
                let file = self.asset_file.as_mut().unwrap();
                reset_format(buf, file, "asset");
            }
            "fragment" => {
                let buf = self.fragment_buf.as_mut().unwrap();
                let file = self.fragment_file.as_mut().unwrap();
                reset_format(buf, file, "fragment");
            }
            _ => {}
        }
    }

    fn do_flush(&mut self) -> Result<Vec<(String, Vec<String>)>, String> {
        loop {
            let s_dp = self.scene_buf.as_ref().map(|b| b.data_ptr as u64).unwrap_or(0);
            let s_rem = self.scene_buf.as_ref().map(|b| b.remaining as u64).unwrap_or(0);
            let a_dp = self.asset_buf.as_ref().map(|b| b.data_ptr as u64).unwrap_or(0);
            let a_rem = self.asset_buf.as_ref().map(|b| b.remaining as u64).unwrap_or(0);
            let f_dp = self.fragment_buf.as_ref().map(|b| b.data_ptr as u64).unwrap_or(0);
            let f_rem = self.fragment_buf.as_ref().map(|b| b.remaining as u64).unwrap_or(0);

            let s_op = self.scene_buf.as_ref().map(|b| b.offset_ptr as u64).unwrap_or(0);
            let a_op = self.asset_buf.as_ref().map(|b| b.offset_ptr as u64).unwrap_or(0);
            let f_op = self.fragment_buf.as_ref().map(|b| b.offset_ptr as u64).unwrap_or(0);

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

                    // Update buffer state AND LoadedFile handles
                    if let Some(ref mut buf) = self.scene_buf {
                        buf.data_ptr += s_data_bytes as usize;
                        buf.offset_ptr -= (s_count as usize) * 8;
                        buf.remaining -= s_bytes as usize;
                        if let Some(ref mut file) = self.scene_file {
                            file.data_len = (buf.data_ptr - buf.data_start) / 4;
                            file.offset_len = (buf.buffer_size - buf.offset_ptr) / 8;
                            file.entity_count = s_count as u32;
                        }
                    }
                    if let Some(ref mut buf) = self.asset_buf {
                        buf.data_ptr += a_data_bytes as usize;
                        buf.offset_ptr -= (a_count as usize) * 8;
                        buf.remaining -= a_bytes as usize;
                        if let Some(ref mut file) = self.asset_file {
                            file.data_len = (buf.data_ptr - buf.data_start) / 4;
                            file.offset_len = (buf.buffer_size - buf.offset_ptr) / 8;
                            file.entity_count = a_count as u32;
                        }
                    }
                    if let Some(ref mut buf) = self.fragment_buf {
                        buf.data_ptr += f_data_bytes as usize;
                        buf.offset_ptr -= (f_count as usize) * 8;
                        buf.remaining -= f_bytes as usize;
                        if let Some(ref mut file) = self.fragment_file {
                            file.data_len = (buf.data_ptr - buf.data_start) / 4;
                            file.offset_len = (buf.buffer_size - buf.offset_ptr) / 8;
                            file.entity_count = f_count as u32;
                        }
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
        let buf = self.get_format_buf(format_name)?;
        let base = buf.shm.base_address().as_ptr() as usize;
        let data_ptr = buf.data_ptr;
        let offset_ptr = buf.offset_ptr;
        let buf_size = buf.buffer_size;
        let data_start = buf.data_start;

        if data_ptr == data_start {
            return Ok(vec![]);
        }

        // Update entity_count in header
        let offset_count = (buf_size - offset_ptr) / 8;
        unsafe {
            let header_ptr = base as *mut u32;
            std::ptr::write_unaligned(header_ptr.add(16), offset_count as u32);
        }

        // Write buffer to disk
        let filename = tbo_writer::write_tbo_file(
            unsafe { std::slice::from_raw_parts(base as *const u8, buf_size) },
            data_ptr,
            offset_ptr,
            &self.output_dir,
            format_name,
        )?;

        // Reset buffer and LoadedFile to initial state
        self.reset_format(format_name);

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
