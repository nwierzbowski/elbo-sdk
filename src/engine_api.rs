use pivot_com_types::EngineCommand;
use pivot_com_types::EngineResponse;
use pivot_com_types::asset_meta::AssetMeta;
use pivot_com_types::asset_ptr::AssetPtr;
use pivot_com_types::asset_surface::GroupSurface;
use pivot_com_types::fields::Uuid;

use crate::asset_sync_context::AssetSyncContext;
use crate::engine_client::EngineClient;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::iter::zip;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use uuid::Uuid as ExternalUuid;

pub static CLIENT: LazyLock<EngineClient> = LazyLock::new(|| EngineClient::new());
pub static ENGINE_DIR: LazyLock<Mutex<Option<PathBuf>>> = LazyLock::new(|| Mutex::new(None));

pub fn start_engine() -> Result<(), String> {
    let engine_path = resolve_engine_binary_path()
        .ok_or_else(|| "Failed to locate pivot_engine binary".to_string())?;
    CLIENT.start(engine_path.to_string_lossy().to_string())?;
    Ok(())
}

pub fn stop_engine() -> Result<(), String> {
    CLIENT.stop()?;
    Ok(())
}

pub fn generate_uuid_bytes() -> [u8; 16] {
    let ptr = ExternalUuid::new_v4().as_bytes().as_ptr() as *const [u8; 16];
    unsafe { *ptr }
}

pub fn poll_mesh_sync() -> Result<Option<AssetSyncContext>, String> {
    let mp = match CLIENT.poll_mesh_sync() {
        Ok(Some(mp)) => mp,
        Ok(None) => return Ok(None),
        Err(e) => return Err(e),
    };

    let asset_ptrs = mp.read_send_mesh()
        .map_err(|e| format!("Buffer read error: {}", e))?;
    let ptrs = CLIENT.hydrate_ptrs(asset_ptrs, &mp.header.root_slab_handle)?;

    Ok(Some(AssetSyncContext::new(ptrs, asset_ptrs)))
}

/// Requests memory for the provided asset metadata and writes the group names and asset metas into the correct places
pub fn allocate_memory(
    vert_counts: Vec<u32>,
    edge_counts: Vec<u32>,
    loop_counts: Vec<u32>,
    total_loop_lengths: Vec<u32>,
    object_counts: Vec<u32>,
    group_names: Vec<String>,
    surface_contexts: Vec<u16>,
    asset_uuids: Vec<Uuid>,
) -> Result<AssetSyncContext, String> {
    let count = asset_uuids.len();

    let mut sizes = Vec::with_capacity(count);
    let mut asset_metas = Vec::with_capacity(count);

    // Calculate the asset meta (offsets) and accumulate them to request memory from engine
    for i in 0..count as usize {
        let (group_metadata, total_size) = AssetMeta::new(
            vert_counts[i],
            edge_counts[i],
            loop_counts[i],
            total_loop_lengths[i],
            object_counts[i],
            surface_contexts[i],
            &group_names[i],
            asset_uuids[i],
        )?;

        asset_metas.push(group_metadata);
        sizes.push(total_size);
    }

    let command = EngineCommand::alloc_request(&asset_uuids, &sizes);
    let resp = CLIENT.send_command(command)?;

    let (_uuids, asset_ptrs) = resp
        .read_alloc_response()
        .map_err(|e| format!("Buffer read error: {}", e))?;

    let ptrs = CLIENT.hydrate_ptrs(asset_ptrs, &resp.header.root_slab_handle)?;

    // Write group names and meta datas into the provided memory
    for ((ptr, asset_meta), group_name) in zip(&ptrs, asset_metas).zip(group_names) {
        unsafe {
            let raw_ptr = ptr.as_ptr();
            let base_bytes = raw_ptr as *mut u8;
            let name_dest = base_bytes.add(asset_meta.offset_group_name as usize);
            std::ptr::write_unaligned(raw_ptr, asset_meta);
            std::ptr::copy_nonoverlapping(group_name.as_ptr(), name_dest, group_name.len());
        };
    }
    Ok(AssetSyncContext::new(ptrs, asset_ptrs))
}

pub fn send_mesh_command(meta_vec: Vec<AssetPtr>) -> Result<EngineResponse, String> {
    let command = EngineCommand::send_mesh(&meta_vec);
    CLIENT.send_command(command)
}

pub fn standardize_groups_command(uuids: Vec<Uuid>) -> Result<EngineResponse, String> {
    let command = EngineCommand::standardize_groups(&uuids);
    CLIENT.send_command(command)
}

pub fn standardize_synced_groups_command(
    uuids: Vec<Uuid>,
    surface_types: Vec<u32>,
) -> Result<EngineResponse, String> {
    let count = uuids.len();
    let mut surface_vec: Vec<GroupSurface> = Vec::with_capacity(count);

    for i in 0..count {
        let surf = GroupSurface::new(uuids[i], surface_types[i] as u64);
        surface_vec.push(surf);
    }

    let command = EngineCommand::standardize_synced_groups(&surface_vec, 1);
    CLIENT.send_command(command)
}

pub fn set_surface_types_command(
    group_surface_map: HashMap<Uuid, i64>,
) -> Result<EngineResponse, String> {
    let count = group_surface_map.len();
    let mut surface_vec: Vec<GroupSurface> = Vec::with_capacity(count);

    group_surface_map.iter().for_each(|(uuid, surface_type)| {
        let surf = GroupSurface::new(*uuid, *surface_type as u64);
        surface_vec.push(surf);
    });

    let command = EngineCommand::set_surface_types(&surface_vec, 1);
    CLIENT.send_command(command)
}

pub fn drop_groups_command(uuids: Vec<Uuid>) -> Result<EngineResponse, String> {
    let command = EngineCommand::drop_groups(&uuids, 1);
    CLIENT.send_command(command)
}

pub fn organize_objects_command() -> Result<EngineResponse, String> {
    let command = EngineCommand::organize_objects(1);
    CLIENT.send_command(command)
}

pub fn extract_geometric_features_command(
    uuids: Vec<Uuid>,
) -> Result<EngineResponse, String> {
    let command = EngineCommand::extract_geometric_features(&uuids, 1);
    CLIENT.send_command(command)
}

pub fn get_surface_types_command() -> Result<EngineResponse, String> {
    let command = EngineCommand::get_surface_types(1);
    CLIENT.send_command(command)
}

pub fn export_assets_command(
    path: &str,
    target_bytes: u32,
    uuids: Vec<Uuid>,
) -> Result<EngineResponse, String> {
    let command = EngineCommand::export_assets(path, target_bytes, &uuids);
    CLIENT.send_command(command)
}

pub fn export_all_command(path: &str, target_bytes: u32) -> Result<EngineResponse, String> {
    let command = EngineCommand::export_all(path, target_bytes);
    CLIENT.send_command(command)
}

pub fn import_assets_command(paths: Vec<String>) -> Result<EngineResponse, String> {
    let path_refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
    let command = EngineCommand::import_assets(&path_refs);
    CLIENT.send_command(command)
}

pub fn set_engine_dir(path: PathBuf) {
    let mut guard = ENGINE_DIR
        .lock()
        .expect("Failed to lock static engine directory");
    *guard = Some(path);
}

fn stored_engine_dir() -> Option<PathBuf> {
    ENGINE_DIR.lock().ok().and_then(|guard| guard.clone())
}

fn pivot_engine_executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "pivot_engine.exe"
    } else {
        "pivot_engine"
    }
}

fn ensure_executable(path: &PathBuf) {
    #[cfg(unix)]
    {
        if let Ok(meta) = fs::metadata(path) {
            let mode = meta.permissions().mode();
            if mode & 0o111 == 0 {
                let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode | 0o111));
            }
        }
    }
}

pub fn get_platform_id() -> String {
    let system = match env::consts::OS {
        "windows" => "windows",
        "macos" => "macos",
        "linux" => "linux",
        other => other,
    };

    let arch = match env::consts::ARCH {
        "x86_64" => "x86-64",
        "aarch64" => "arm64",
        other => other,
    };

    format!("{}-{}", system, arch)
}

pub fn resolve_engine_binary_path() -> Option<PathBuf> {
    if let Some(val) = env::var_os("PIVOT_ENGINE_PATH") {
        if !val.is_empty() {
            let pb = PathBuf::from(&val);
            if pb.is_file() {
                ensure_executable(&pb);
                return Some(pb);
            }
        }
    }

    if let Some(engine_dir) = stored_engine_dir() {
        let exe_name = pivot_engine_executable_name();
        let platform_path = engine_dir.join(get_platform_id()).join(exe_name);
        if platform_path.is_file() {
            ensure_executable(&platform_path);
            return Some(platform_path);
        }

        let fallback = engine_dir.join(exe_name);
        if fallback.is_file() {
            ensure_executable(&fallback);
            return Some(fallback);
        }
    }

    None
}
