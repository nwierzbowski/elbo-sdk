use pivot_com_types::Buffer;
use pivot_com_types::EngineCommand;
use pivot_com_types::EngineResponse;
use pivot_com_types::OP_DROP_GROUPS;
use pivot_com_types::OP_GET_SURFACE_TYPES;
use pivot_com_types::OP_ORGANIZE_OBJECTS;
use pivot_com_types::OP_SET_SURFACE_TYPES;
use pivot_com_types::OP_STANDARDIZE_GROUPS;
use pivot_com_types::OP_STANDARDIZE_SYNCED_GROUPS;
use pivot_com_types::alloc::AllocRequestMeta;
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

    let asset_ptrs = mp
        .inline_data
        .to_asset_meta_ptr(mp.header.num_items as usize);
    let ptrs = CLIENT.hydrate_ptrs(&asset_ptrs, &mp.header.root_slab_handle)?;

    Ok(Some(AssetSyncContext::new(ptrs, asset_ptrs)))
}

///Requests memory for the provided asset mettadata and writes the group names and asset mettas into the correct places
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

    // let mut asset_slices = Vec::with_capacity(count);
    let mut sizes = Vec::with_capacity(count);
    let mut asset_metas = Vec::with_capacity(count);

    //Calculate the asset metta (offsets) and accumulate them to request memory from engine
    for i in 0..count as usize {
        // let handle_name = new_uid16();

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
        // let asset_ptr = AssetPtr::new(0, handle_name);
    }

    let resp = request_memory_allocation(&asset_uuids, &sizes)?;

    //Get the asset ptrs from the memory alloc reponse
    let (_uuids, asset_ptrs) = resp.inline_data.to_alloc_response();

    //Convert the provided asset ptrs to real local ptrs
    let ptrs = CLIENT.hydrate_ptrs(&asset_ptrs, &resp.header.root_slab_handle)?;

    //Write group names and meta datas into the provided memory
    for ((ptr, asset_meta), group_name) in zip(&ptrs, asset_metas).zip(group_names) {
        // Copy the AssetMeta into the shared memory
        unsafe {
            let raw_ptr = ptr.as_ptr();
            let base_bytes = raw_ptr as *mut u8;
            let name_dest = base_bytes.add(asset_meta.offset_group_name as usize);
            //Write name into memory
            std::ptr::write_unaligned(raw_ptr, asset_meta);

            //Copy meta into memory
            std::ptr::copy_nonoverlapping(group_name.as_ptr(), name_dest, group_name.len());
        };
    }
    Ok(AssetSyncContext::new(ptrs, asset_ptrs))
}

pub fn standardize_groups_command(meta_vec: Vec<AssetPtr>) -> Result<EngineResponse, String> {
    let mut command = EngineCommand {
        should_cache: 1,
        op_id: OP_STANDARDIZE_GROUPS,
        num_headers: meta_vec.len() as u32,
        inline_data: Buffer::new(),
    };

    command.inline_data.copy_payload(&meta_vec, 0);

    CLIENT.send_command(command)
}

pub fn standardize_synced_groups_command(
    uuids: Vec<Uuid>,
    surface_types: Vec<u32>,
) -> Result<EngineResponse, String> {
    // let command = json!({
    //     "id": COMMAND_CLASSIFY_GROUPS,
    //     "op": "standardize_synced_groups",
    //     "group_names": group_names,
    //     "surface_contexts": surface_contexts,
    // });
    let count = uuids.len() as u32;
    let mut surface_vec: Vec<GroupSurface> = Vec::with_capacity(count as usize);

    for i in 0..count as usize {
        let surf = GroupSurface::new(uuids[i], surface_types[i] as u64);
        surface_vec.push(surf);
    }

    let mut command = EngineCommand {
        should_cache: 1,
        op_id: OP_STANDARDIZE_SYNCED_GROUPS,
        num_headers: count,
        inline_data: Buffer::new(),
    };

    command.inline_data.copy_payload(&surface_vec, 0);

    CLIENT.send_command(command)
}

pub fn set_surface_types_command(
    group_surface_map: HashMap<Uuid, i64>,
) -> Result<EngineResponse, String> {
    let count = group_surface_map.len() as u32;
    let mut surface_vec: Vec<GroupSurface> = Vec::with_capacity(count as usize);

    group_surface_map.iter().for_each(|(uuid, surface_type)| {
        let surf = GroupSurface::new(*uuid, *surface_type as u64);
        surface_vec.push(surf);
    });

    // let command = json!({
    //     "id": COMMAND_SET_SURFACE_TYPES,
    //     "op": "set_surface_types",
    //     "classifications": classifications
    // });

    let mut command = EngineCommand {
        should_cache: 1,
        op_id: OP_SET_SURFACE_TYPES,
        num_headers: count,
        inline_data: Buffer::new(),
    };

    command.inline_data.copy_payload(&surface_vec, 0);

    CLIENT.send_command(command)
}

pub fn drop_groups_command(uuids: Vec<Uuid>) -> Result<EngineResponse, String> {
    // let command = json!({
    //     "id": COMMAND_DROP_GROUPS,
    //     "op": "drop_groups",
    //     "group_names": group_names
    // });
    let count = uuids.len() as u32;
    let mut uuid_vec: Vec<Uuid> = Vec::with_capacity(count as usize);

    uuids.iter().for_each(|uuid| {
        uuid_vec.push(*uuid);
    });

    let mut command = EngineCommand {
        should_cache: 1,
        op_id: OP_DROP_GROUPS,
        num_headers: count,
        inline_data: Buffer::new(),
    };

    command.inline_data.copy_payload(&uuid_vec, 0);

    CLIENT.send_command(command)
}

pub fn organize_objects_command() -> Result<EngineResponse, String> {
    // let command = json!({
    //     "id": COMMAND_ORGANIZE_OBJECTS,
    //     "op": "organize_objects"
    // });

    let command = EngineCommand {
        should_cache: 1,
        op_id: OP_ORGANIZE_OBJECTS,
        num_headers: 0,
        inline_data: Buffer::new(),
    };

    CLIENT.send_command(command)
}

pub fn get_surface_types_command() -> Result<EngineResponse, String> {
    // let command = json!({
    //     "id": COMMAND_GET_GROUP_SURFACE_TYPES,
    //     "op": "get_surface_types"
    // });

    let command = EngineCommand {
        should_cache: 1,
        op_id: OP_GET_SURFACE_TYPES,
        num_headers: 0,
        inline_data: Buffer::new(),
    };

    CLIENT.send_command(command)
}

pub fn request_memory_allocation(
    uuids: &[Uuid],
    sizes: &[usize],
) -> Result<EngineResponse, String> {
    let mut command = EngineCommand {
        should_cache: 1,
        op_id: pivot_com_types::OP_ALLOC_MEM,
        num_headers: 1,
        inline_data: Buffer::new(),
    };

    println!(
        "[Engine API] Requesting memory allocation for {} assets",
        uuids.len()
    );

    let req = AllocRequestMeta::new(uuids.len() as u64);

    command.inline_data.copy_payload(&[req], 0);
    command.inline_data.copy_payload(uuids, req.offset_uuids);
    command.inline_data.copy_payload(sizes, req.offset_sizes);
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

// Platform / binary resolution helpers (mirrors C++ `get_platform_id`/`resolve_engine_binary_path`)
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
    // Respect explicit override if set and valid
    if let Some(val) = env::var_os("PIVOT_ENGINE_PATH") {
        if !val.is_empty() {
            let pb = PathBuf::from(&val);
            if pb.is_file() {
                // return Some(pb);
                ensure_executable(&pb);
                return Some(pb);
            }
        }
    }

    if let Some(engine_dir) = stored_engine_dir() {
        let exe_name = pivot_engine_executable_name();
        let platform_path = engine_dir.join(get_platform_id()).join(exe_name);
        if platform_path.is_file() {
            // return Some(platform_path);
            ensure_executable(&platform_path);
            return Some(platform_path);
        }

        let fallback = engine_dir.join(exe_name);
        if fallback.is_file() {
            // return Some(fallback);
            ensure_executable(&fallback);
            return Some(fallback);
        }
    }

    None
}
