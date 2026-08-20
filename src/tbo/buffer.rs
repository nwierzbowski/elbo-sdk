//! Live export buffer for one TBO format, backed by shared memory.
//!
//! The engine writes raw f32 data forward from `data_ptr` and one u64
//! end-offset per entity backward from the top of the buffer; [`FormatBuffer`]
//! tracks both cursors and the file header between flushes.

use iceoryx2::prelude::{FileName, SemanticString};
use iceoryx2_bb_posix::file::{AccessMode, CreationMode};
use iceoryx2_bb_posix::shared_memory::{SharedMemory, SharedMemoryBuilder};

use super::format;
use super::tbo_collection::Keepalive;
use super::tbo_data_view::ChannelSet;

/// Export-side keepalive: an independent mapping of the buffer's shared-memory
/// region. The mapping outlives the exporting context, so zero-copy views built
/// from it stay valid.
///
/// SAFETY: the mapping is a private `mmap` of a named segment; dropping it on a
/// different thread is a plain `munmap`, which is thread-safe, and the region's
/// owner is the OS, not this handle.
#[allow(dead_code)] // the mapping is held only to keep the region alive
pub struct ShmKeep(pub SharedMemory);
unsafe impl Send for ShmKeep {}
impl Keepalive for ShmKeep {}

/// Fixed-size, null-terminated name for an iceoryx shared-memory segment.
pub fn slab_name(name: &str) -> [u8; 64] {
    let mut buf = [0u8; 64];
    let bytes = name.as_bytes();
    let len = bytes.len().min(63);
    buf[..len].copy_from_slice(&bytes[..len]);
    buf
}

/// Unique slab name for this process: `<prefix>-<pid>-<8 hex random>`.
pub fn unique_slab_name(prefix: &str) -> [u8; 64] {
    slab_name(&format!("{}-{}-{:08x}", prefix, std::process::id(), rand::random::<u32>()))
}

fn file_name(name: &[u8; 64]) -> Result<FileName, String> {
    let len = name.iter().position(|&b| b == 0).unwrap_or(64);
    FileName::new(&name[..len]).map_err(|e| format!("Invalid SHM name: {:?}", e))
}

/// Create (or purge and recreate) the shared-memory segment for a buffer.
pub fn create_shm(name: &[u8; 64], size: usize) -> Result<SharedMemory, String> {
    let file_name = file_name(name)?;
    SharedMemoryBuilder::new(&file_name)
        .is_memory_locked(false)
        .creation_mode(CreationMode::PurgeAndCreate)
        .size(size)
        .create()
        .map_err(|e| format!("Failed to create SHM: {:?}", e))
}

/// Open an existing shared-memory segment read-only, as an independent mapping
/// that keeps the region alive for zero-copy views.
pub fn open_shm_mapping(name: &[u8; 64]) -> Result<SharedMemory, String> {
    let file_name = file_name(name)?;
    SharedMemoryBuilder::new(&file_name)
        .open_existing(AccessMode::Read)
        .map_err(|e| format!("Failed to open SHM: {:?}", e))
}

/// Shared-memory-backed buffer for one export format.
pub struct FormatBuffer {
    pub shm: SharedMemory,
    pub channels: ChannelSet,
    format_index: u32,
    pub data_ptr: usize,
    pub data_start: usize,
    pub buffer_size: usize,
    remaining: usize,
    entity_count: u64,
}

impl FormatBuffer {
    /// Lay out and initialize a new buffer: writes the header and channel names
    /// and seeds the first offset with `data_start`.
    pub fn new(
        shm: SharedMemory,
        buffer_size: usize,
        format_index: u32,
        channels: ChannelSet,
    ) -> Result<Self, String> {
        let data_start = format::data_start(channels.names());
        if buffer_size < data_start + 16 {
            return Err(
                "Export buffer too small for channel names and offset region".to_string(),
            );
        }
        let base = shm.base_address().as_ptr();
        format::write_header(base, buffer_size, format_index, channels.names(), data_start);

        Ok(Self {
            shm,
            channels,
            format_index,
            data_ptr: data_start,
            data_start,
            buffer_size,
            remaining: buffer_size - 16 - data_start,
            entity_count: 0,
        })
    }

    /// Reset to the empty state: rewrite the header and rewind the cursors.
    pub fn reset(&mut self) {
        let base = self.shm.base_address().as_ptr();
        format::write_header(
            base,
            self.buffer_size,
            self.format_index,
            self.channels.names(),
            self.data_start,
        );
        self.data_ptr = self.data_start;
        self.remaining = self.buffer_size - 16 - self.data_start;
        self.entity_count = 0;
    }

    /// Whether the buffer holds any data past the initial cursors.
    pub fn is_empty(&self) -> bool {
        self.data_ptr == self.data_start
    }

    /// Number of entities flushed into this buffer (excludes the seed offset).
    pub fn entity_count(&self) -> u64 {
        self.entity_count
    }

    /// Address where the engine will write the *next* entity end-offset.
    pub fn offset_ptr(&self) -> usize {
        self.buffer_size - ((self.entity_count as usize) + 2) * 8
    }

    /// Lowest address of the occupied offset region (seed + all written entity
    /// end-offsets), for zero-copy views and disk flush.
    pub fn offset_region_start(&self) -> usize {
        self.offset_ptr() + 8
    }

    /// Number of u64 offsets in the buffer (seed + one per entity).
    pub fn offset_len(&self) -> usize {
        format::offset_count(self.entity_count)
    }

    /// Remaining capacity in bytes (data + offset space).
    pub fn remaining(&self) -> usize {
        self.remaining
    }

    /// Consume `data_bytes` of data and `count` entity offsets after a flush.
    pub fn advance(&mut self, data_bytes: u64, count: u64) -> Result<(), String> {
        let total = data_bytes
            .checked_add(count * 8)
            .ok_or_else(|| "advance: overflow".to_string())?;
        if (total as usize) > self.remaining {
            return Err(format!(
                "advance: {} bytes exceeds remaining {}",
                total, self.remaining
            ));
        }
        self.data_ptr += data_bytes as usize;
        self.entity_count += count;
        self.remaining -= total as usize;
        Ok(())
    }
}
