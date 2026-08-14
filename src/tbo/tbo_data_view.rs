//! Internal data view for TBO files.
//!
//! Parses offsets (N offsets for N entities, end boundaries, reverse entity order)
//! and provides entity slicing and channel mapping.
//! All data access is zero-copy via raw pointers.

use std::collections::HashMap;

/// Wraps one file's data + offsets via zero-copy pointers.
/// Handles entity slicing and channel lookup.
#[derive(Clone)]
pub struct DataView {
    pub data_ptr: *const f32,
    pub data_len: usize,
    /// Raw u64 offset pointer (absolute buffer positions).
    /// `offsets[i]` = end of entity `(n-1-i)` in buffer positions.
    /// Entity 0 starts at f32 index 0 (implicit).
    pub offsets_ptr: *const u64,
    pub offset_len: usize,
    /// Byte offset where data region starts (used to convert raw u64 → f32 index).
    pub data_start: u64,
    pub channel_names: Vec<String>,
    pub channel_map: HashMap<String, usize>,
    pub entity_count: usize,
    pub channel_count: usize,
}

impl DataView {
    pub fn new(
        data_ptr: *const f32,
        data_len: usize,
        offsets_ptr: *const u64,
        offset_len: usize,
        data_start: u64,
        channel_names: Vec<String>,
    ) -> Self {
        let channel_count = channel_names.len();
        let channel_map: HashMap<String, usize> = channel_names
            .iter()
            .enumerate()
            .map(|(i, name)| (name.clone(), i))
            .collect();

        Self {
            data_ptr,
            data_len,
            offsets_ptr,
            offset_len,
            data_start,
            channel_names,
            channel_map,
            entity_count: offset_len.saturating_sub(1),
            channel_count,
        }
    }

    /// Returns (start_f32, end_f32) for entity at given index.
    /// Offsets are in reverse entity order: offsets[-(i+1)] = end of entity i.
    /// Raw u64 values are converted to f32 indices on-the-fly.
    pub fn entity_f32_range(&self, entity_idx: usize) -> (usize, usize) {
        let n = self.offset_len;
        let raw_to_f32 = |raw_off: u64| {
            ((raw_off.saturating_sub(self.data_start)) as usize) / 4
        };

        let raw_start = unsafe { *self.offsets_ptr.add(n - entity_idx) };
        let start = raw_to_f32(raw_start);
        let raw_end = unsafe { *self.offsets_ptr.add(n - entity_idx - 1) };
        let end = raw_to_f32(raw_end);
        (start, end)
    }

    /// Returns contiguous slice of f32 values for the entity.
    pub fn entity_data(&self, entity_idx: usize) -> &[f32] {
        let (start, end) = self.entity_f32_range(entity_idx);
        unsafe { std::slice::from_raw_parts(self.data_ptr.add(start), end - start) }
    }

    /// Returns the column index for a channel name, or None.
    pub fn channel_index(&self, name: &str) -> Option<usize> {
        self.channel_map.get(name).copied()
    }

    /// Returns a single f32 value for a channel at the first row.
    pub fn channel_scalar(&self, entity_idx: usize, channel_name: &str) -> Option<f32> {
        let ch_idx = self.channel_index(channel_name)?;
        let (start, _) = self.entity_f32_range(entity_idx);
        unsafe { Some(*self.data_ptr.add(start + ch_idx)) }
    }
}
