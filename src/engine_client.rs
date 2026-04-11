use crossbeam::channel;
use iceoryx2::prelude::*;
use iceoryx2_bb_posix::file::AccessMode;
use iceoryx2_bb_posix::shared_memory::{SharedMemory, SharedMemoryBuilder};
use pivot_com_types::alloc::SlabRegistry;
use pivot_com_types::asset_meta::AssetMeta;
use pivot_com_types::asset_ptr::AssetPtr;
use pivot_com_types::{EngineCommand, EngineResponse, MeshPublish};
use std::process::Child;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::command_thread::{CommandWork, spawn_command_thread};
use crate::mesh_sync_thread::spawn_mesh_sync_thread;

#[derive(Debug)]
struct ActiveState {
    engine_process: Child,
    command_tx: channel::Sender<CommandWork>,
    mesh_update_rx: channel::Receiver<MeshPublish>,
    shutdown: Arc<AtomicBool>,
    threads: Vec<std::thread::JoinHandle<()>>,

    slabs: Vec<SharedMemory>,
}
unsafe impl Send for ActiveState {}

#[derive(Debug)]
pub struct EngineClient {
    state: Mutex<Option<ActiveState>>,
    node: Arc<Node<ipc::Service>>,
}

impl EngineClient {
    pub fn new() -> Self {
        let node = NodeBuilder::new()
            .create::<ipc::Service>()
            .expect("Failed to create iceoryx2 node");

        EngineClient {
            state: Mutex::new(None),
            node: Arc::new(node),
        }
    }

    pub fn send_command(&self, cmd: EngineCommand) -> Result<EngineResponse, String> {
        let (tx, rx) = channel::bounded(1);

        let guard = self.state.lock().unwrap();
        let state = guard.as_ref().ok_or("Engine not started")?;

        state
            .command_tx
            .send(CommandWork {
                cmd,
                response_tx: tx,
            })
            .map_err(|e| format!("Failed to send command: {}", e))?;

        drop(guard);

        rx.recv()
            .map_err(|e| format!("Failed to receive response: {}", e))?
    }

    pub fn poll_mesh_sync(&self) -> Result<Option<MeshPublish>, String> {
        let guard = self.state.lock().unwrap();

        let state = match guard.as_ref() {
            Some(state) => state,
            None => return Ok(None),
        };

        match state.mesh_update_rx.try_recv() {
            Ok(publish) => Ok(Some(publish)),
            Err(_) => Ok(None),
        }
    }

    pub fn start(&self, path: String) -> Result<(), String> {
        let mut guard = self.state.lock().unwrap();

        if guard.is_some() {
            return Ok(());
        }

        let engine_process = std::process::Command::new(path)
            .spawn()
            .map_err(|e| e.to_string())?;

        let (command_tx, command_rx) = channel::bounded::<CommandWork>(10);
        let (mesh_update_tx, mesh_update_rx) = channel::unbounded::<MeshPublish>();
        let shutdown = Arc::new(AtomicBool::new(false));
        let command_thread = spawn_command_thread(self.node.clone(), command_rx, shutdown.clone());
        let mesh_sync_thread =
            spawn_mesh_sync_thread(self.node.clone(), shutdown.clone(), mesh_update_tx);

        *guard = Some(ActiveState {
            engine_process,
            command_tx,
            threads: vec![command_thread, mesh_sync_thread],
            shutdown: shutdown,
            mesh_update_rx,
            slabs: Vec::new(),
        });
        Ok(())
    }

    pub fn stop(&self) -> Result<(), String> {
        {
            let guard = self.state.lock().unwrap();
            if guard.is_none() {
                return Ok(());
            }
        }

        let command = EngineCommand::stop_engine();
        let res = self.send_command(command);

        let mut guard = self.state.lock().unwrap();
        if let Some(mut state) = guard.take() {
            if let Err(e) = res {
                eprintln!(
                    "Failed to send stop command to engine, killing process: {}",
                    e
                );
                let _ = state.engine_process.kill();
            }

            state.shutdown.store(true, Ordering::SeqCst);

            for handle in state.threads {
                let _ = handle.join();
            }

            let _ = state.engine_process.wait();

            println!("All threads joined. SDK is clean.");
        }

        Ok(())
    }

    ///Takes asset ptrs and hydrates them into local pointers into shared memory for AssetMetas
    /// If we dont have any slabs registered yet we use the root_handle as the slab registry is the first address there
    pub fn hydrate_ptrs(
        &self,
        asset_ptrs: &[AssetPtr],
        root_handle: &[u8],
    ) -> Result<Vec<NonNull<AssetMeta>>, String> {
        let mut guard = self.state.lock().unwrap();
        let state = guard.as_mut().expect("Engine not started");

        //If this is engine startup we open the root shm which contains the slab registry for later syncing
        if state.slabs.is_empty() {
            state.slabs.push(open_shm(root_handle)?);
        }

        Self::ensure_slabs_synced(state); // Ensure that we have the correct number of slabs

        let mut ptrs = Vec::with_capacity(asset_ptrs.len());

        for asset_ptr in asset_ptrs {
            let (slab_index, offset) = asset_ptr.unpack();

            let shm = &state
                .slabs
                .get(slab_index as usize)
                .ok_or_else(|| format!("Slab index {} is out of bounds", slab_index))?;

            unsafe {
                let raw_ptr = shm.base_address().as_ptr().add(offset as usize) as *mut AssetMeta;
                ptrs.push(NonNull::new_unchecked(raw_ptr));
            }
        };

        Ok(ptrs)
        
    }

    ///Checks the returned number of slabs and opens the ones at the end of the list until we have the correct ones open as the engine will only ever create new ones at the end
    fn ensure_slabs_synced(state: &mut ActiveState) {
        let registry = unsafe { &*(state.slabs[0].base_address().as_ptr() as *const SlabRegistry) };
        let target_count = registry.num_slabs as usize;

        // If the Engine added a slab, Blender catches up here.
        while state.slabs.len() < target_count {
            let next_idx = state.slabs.len();
            let handle = &registry.slab_handles[next_idx];

            match open_shm(handle) {
                Ok(shm) => {
                    println!(
                        "[SDK] Auto-mapped new memory slab [{}]: {:?}",
                        next_idx,
                        bytes_to_clean_str(handle)
                    );
                    state.slabs.push(shm);
                }
                Err(e) => {
                    eprintln!("[SDK] Failed to map discovered slab: {}", e);
                    break;
                }
            }
        }
    }
}

pub fn bytes_to_clean_str(bytes: &[u8]) -> &[u8] {
    // Look for the first null terminator, or use the whole slice if none found
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..len]
}

///Opens existing shm by u8 handle
fn open_shm(handle: &[u8]) -> Result<SharedMemory, String> {
    let clean_handle = bytes_to_clean_str(handle);
    let file_name = match FileName::new(clean_handle) {
        Ok(f) => f,
        Err(e) => {
            return Err(format!(
                "invalid shared memory name '{:?}': {:?}",
                clean_handle, e
            ));
        }
    };

    let shm = {
        SharedMemoryBuilder::new(&file_name)
            .open_existing(AccessMode::ReadWrite)
            .expect("Failed to open shm")
    };
    Ok(shm)
}
