//! Internal data view for TBO files.
//!
//! A `DataView` is raw-pointer metadata over one buffer (live SHM or heap) holding
//! TBO data. Offsets are stored in reverse entity order (the start offset is
//! written first, entity end offsets afterwards), so in memory/file order the
//! offset array is *descending* by value: index 0 (lowest address) is the end of
//! the last entity, index `offset_len - 1` (highest address) is the start of
//! entity 0 (equal to `data_start`).
//!
//! `channel_map: HashMap<String, usize>` maps channel name -> column index.
//! All data access is zero-copy via raw pointers.

use std::collections::HashMap;
use std::sync::Arc;

use pyo3::prelude::*;

use super::py_value_error;

/// Cheaply-clonable handle to a channel layout (names + lookup map), shared
/// between the collection state, per-file views, and entities.
#[derive(Clone, Default)]
pub struct ChannelSet {
    names: Arc<Vec<String>>,
    map: Arc<HashMap<String, usize>>,
}

impl ChannelSet {
    pub fn from_names(names: Vec<String>) -> Self {
        let map = names
            .iter()
            .enumerate()
            .map(|(i, name)| (name.clone(), i))
            .collect();
        Self {
            names: Arc::new(names),
            map: Arc::new(map),
        }
    }

    pub fn name_count(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub fn names(&self) -> &Vec<String> {
        &self.names
    }

    /// Returns the column index for a channel name, or None.
    pub fn index(&self, name: &str) -> Option<usize> {
        self.map.get(name).copied()
    }

    /// Comma-separated list of names (for error messages).
    pub fn names_display(&self) -> String {
        self.names.join(", ")
    }
}

/// Wraps one buffer's data + offsets via zero-copy pointers.
#[derive(Clone)]
pub struct DataView {
    pub data_ptr: *const u8,
    /// Byte count in the data region.
    pub data_len: usize,
    /// Raw u64 offset pointer (start of offset region, lowest address).
    pub offsets_ptr: *const u64,
    pub offset_len: usize,
    /// Buffer-relative byte offset where the data region starts.
    pub data_start: u64,
    pub channels: ChannelSet,
    pub channel_count: usize,
    /// Bytes per element: 4 for f32 formats, 8 for u64 formats.
    pub bytes_per_element: usize,
}

impl DataView {
    pub fn new(
        data_ptr: *const u8,
        data_len: usize,
        offsets_ptr: *const u64,
        offset_len: usize,
        data_start: u64,
        channels: ChannelSet,
        bytes_per_element: usize,
    ) -> Self {
        let channel_count = channels.name_count();
        Self {
            data_ptr,
            data_len,
            offsets_ptr,
            offset_len,
            data_start,
            channels,
            channel_count,
            bytes_per_element,
        }
    }

    pub fn entity_count(&self) -> usize {
        self.offset_len.saturating_sub(1)
    }

    /// Returns the (start_byte, end_byte) range of the entity at the given index.
    ///
    /// Offsets are stored in reverse entity order: in memory/file order,
    /// index `offset_len - 1 - i` holds the start of entity `i`
    /// (entity 0's start equals `data_start`), and index `offset_len - 2 - i`
    /// holds the end of entity `i`.
    pub fn entity_byte_range(&self, entity_idx: usize) -> Option<(usize, usize)> {
        if entity_idx >= self.entity_count() {
            return None;
        }
        let n = self.offset_len;
        let raw_to_byte = |raw_off: u64| {
            (raw_off.saturating_sub(self.data_start)) as usize
        };
        let raw_start = unsafe { *self.offsets_ptr.add(n - 1 - entity_idx) };
        let raw_end = unsafe { *self.offsets_ptr.add(n - 2 - entity_idx) };
        Some((raw_to_byte(raw_start), raw_to_byte(raw_end)))
    }

    /// Validates the offset array: strictly ascending in write order
    /// (i.e. descending in memory/file order), starting exactly at
    /// `data_start`, staying within the data region, and with every
    /// entity span divisible by one row.
    pub fn validate(&self) -> PyResult<()> {
        let data_end = self
            .data_start
            .checked_add(self.data_len as u64)
            .ok_or_else(|| py_value_error("data region overflows u64"))?;

        let n = self.offset_len;
        if n == 0 {
            return Err(py_value_error("offset region is empty"));
        }
        // w[i] in write order: w[0] = start (data_start), w[k] = end of entity k-1.
        // In memory/file order: w[i] is at index n - 1 - i.
        let write_at = |i: usize| unsafe { *self.offsets_ptr.add(n - 1 - i) };

        if write_at(0) != self.data_start {
            return Err(py_value_error(format!(
                "first offset {} != data_start {}",
                write_at(0),
                self.data_start
            )));
        }
        let mut prev = self.data_start;
        for i in 1..n {
            let v = write_at(i);
            if v < prev {
                return Err(py_value_error(format!(
                    "offset {} (entity end {}) is less than previous offset {}",
                    i,
                    i - 1,
                    prev
                )));
            }
            if v > data_end {
                return Err(py_value_error(format!(
                    "offset {} ({}) exceeds data end ({})",
                    i,
                    v,
                    data_end
                )));
            }
            if self.channel_count > 0 {
                let span_bytes = v - prev;
                let row_bytes = self.bytes_per_element * self.channel_count;
                if !span_bytes.is_multiple_of(self.bytes_per_element as u64)
                    || (span_bytes as usize) % row_bytes != 0
                {
                    return Err(py_value_error(format!(
                        "entity {} span ({} bytes) is not a multiple of row size {} bytes",
                        i - 1,
                        span_bytes,
                        row_bytes
                    )));
                }
            }
            prev = v;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a DataView over entities with the given f32 spans (each a
    /// multiple of `channel_count`). Returns the view plus the storage it
    /// points into (kept alive for the test's duration).
    fn make_view(spans: &[usize], channel_count: usize) -> (DataView, Vec<f32>, Vec<u64>) {
        let ch = channel_count.max(1);
        let data: Vec<f32> = spans.iter().flat_map(|&s| vec![1.0f32; s]).collect();
        let mut write_order = vec![0u64];
        for &s in spans {
            write_order.push(write_order.last().copied().unwrap() + (s * 4) as u64);
        }
        let file_order: Vec<u64> = write_order.iter().rev().copied().collect();
        let names: Vec<String> = (0..ch).map(|i| format!("c{i}")).collect();
        let view = DataView::new(
            data.as_ptr() as *const u8,
            data.len() * 4,
            file_order.as_ptr(),
            write_order.len(),
            0,
            ChannelSet::from_names(names),
            4,
        );
        (view, data, file_order)
    }

    #[test]
    fn channel_set_lookup() {
        let set = ChannelSet::from_names(vec!["x".into(), "y".into()]);
        assert_eq!(set.name_count(), 2);
        assert_eq!(set.index("x"), Some(0));
        assert_eq!(set.index("y"), Some(1));
        assert_eq!(set.index("z"), None);
        assert_eq!(set.names_display(), "x, y");
    }

    #[test]
    fn valid_offsets_pass_validation() {
        let (view, data, file) = make_view(&[4, 2, 6], 2);
        view.validate().unwrap();
        assert_eq!(view.entity_count(), 3);
        assert_eq!(view.entity_f32_range(0), Some((0, 4)));
        assert_eq!(view.entity_f32_range(1), Some((4, 6)));
        assert_eq!(view.entity_f32_range(2), Some((6, 12)));
        assert_eq!(view.entity_f32_range(3), None);
        let _ = (&data, &file);
    }

    // In file order, write-order w[0] (= data_start, 0) is the LAST element.
    #[test]
    fn first_offset_must_equal_data_start() {
        let (view, data, mut file) = make_view(&[4, 2, 6], 2);
        *file.last_mut().unwrap() += 8;
        assert!(view.validate().is_err());
        let _ = (&data, &file);
    }

    #[test]
    fn write_order_must_be_non_decreasing() {
        let (view, data, mut file) = make_view(&[4, 2, 6], 2);
        // write-order w[2] lives at file index n-3; make it smaller than w[1].
        let idx = file.len() - 3;
        file[idx] = 4;
        assert!(view.validate().is_err());
        let _ = (&data, &file);
    }

    #[test]
    fn offsets_must_stay_within_data_region() {
        let (view, data, mut file) = make_view(&[4, 2, 6], 2);
        // write-order w[1] lives at file index n-2.
        let idx = file.len() - 2;
        file[idx] = 4096;
        assert!(view.validate().is_err());
        let _ = (&data, &file);
    }

    #[test]
    fn entity_span_must_be_a_row_multiple() {
        let (view, data, file) = make_view(&[3], 2);
        assert!(view.validate().is_err());
        let _ = (&data, &file);
    }
}
