//! Shared collection infrastructure for TBO collections.
//!
//! Provides `CollectionState` for multi-file view management and entity resolution.

use pyo3::prelude::*;

use super::tbo_data_view::DataView;

/// Shared state for all TBO collections: views, entity indexing, row tracking.
pub struct CollectionState {
    pub views: Vec<DataView>,
    boundaries: Vec<usize>,
    row_boundaries: Vec<usize>,
    pub channel_names: Vec<String>,
}

impl Default for CollectionState {
    fn default() -> Self {
        Self::build(Vec::new())
    }
}

impl CollectionState {
    pub fn build(views: Vec<DataView>) -> Self {
        let channel_names = views
            .iter()
            .find(|v| !v.channel_names.is_empty())
            .map(|v| v.channel_names.clone())
            .unwrap_or_default();

        let mut boundaries = Vec::new();
        let mut row_boundaries = Vec::new();
        let mut cum = 0;
        let mut row_cum = 0;
        for v in &views {
            boundaries.push(cum);
            cum += v.entity_count;
            if v.channel_count > 0 {
                row_cum += v.data_len / v.channel_count;
            }
            row_boundaries.push(row_cum);
        }
        boundaries.push(cum);

        Self {
            views,
            boundaries,
            row_boundaries,
            channel_names,
        }
    }

    pub fn total_entities(&self) -> usize {
        self.boundaries.last().copied().unwrap_or(0)
    }

    /// Resolve global entity index to (view_idx, local_entity_idx).
    pub fn resolve(&self, global_idx: usize) -> Option<(usize, usize)> {
        if self.views.is_empty() {
            return None;
        }
        if global_idx >= self.total_entities() {
            return None;
        }
        let view_idx = self.boundaries.partition_point(|&b| b <= global_idx) - 1;
        let local = global_idx - self.boundaries[view_idx];
        Some((view_idx, local))
    }

    /// Cumulative row count across all entities before the given global entity index.
    pub fn cumulative_rows_before(&self, global_entity_idx: usize) -> PyResult<usize> {
        let (view_idx, local_idx) = self.resolve(global_entity_idx)
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyIndexError, _>(
                format!("Entity index {} out of range", global_entity_idx)
            ))?;
        let rows_before_view = if view_idx > 0 {
            self.row_boundaries[view_idx - 1]
        } else {
            0
        };
        let view = &self.views[view_idx];
        if view.channel_count > 0 {
            Ok(rows_before_view + view.entity_f32_range(local_idx).0 / view.channel_count)
        } else {
            Ok(rows_before_view)
        }
    }

    /// Number of rows (child entries) in the entity at the given global index.
    pub fn entity_row_count(&self, global_entity_idx: usize) -> PyResult<usize> {
        let (view_idx, local_idx) = self.resolve(global_entity_idx)
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyIndexError, _>(
                format!("Entity index {} out of range", global_entity_idx)
            ))?;
        let view = &self.views[view_idx];
        let (start, end) = view.entity_f32_range(local_idx);
        if view.channel_count > 0 {
            Ok((end - start) / view.channel_count)
        } else {
            Ok(0)
        }
    }

    /// Resolves entity at given index to raw data pointer, length, row stride, and channel names.
    pub fn resolve_entity(&self, idx: usize) -> PyResult<(*const f32, usize, usize, &Vec<String>)> {
        let total = self.total_entities();
        if total == 0 {
            return Err(PyErr::new::<pyo3::exceptions::PyIndexError, _>("No entities in collection"));
        }
        let actual_idx = idx.min(total - 1);
        let (view_idx, local_idx) = self.resolve(actual_idx)
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyIndexError, _>(
                format!("Entity index {} out of range", actual_idx)
            ))?;
        let view = &self.views[view_idx];
        let (start, end) = view.entity_f32_range(local_idx);
        let data_ptr = unsafe { view.data_ptr.add(start) };
        let data_len = end - start;
        let row_stride = view.channel_count;
        Ok((data_ptr, data_len, row_stride, &view.channel_names))
    }
}
