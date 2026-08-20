//! Shared collection infrastructure for TBO collections.
//!
//! Provides `CollectionState` for multi-file view management, entity
//! resolution, and hierarchy consistency validation. Each `DataView` is paired
//! with a matching `DataBacking` keepalive so that the underlying memory (heap
//! buffer or shared-memory mapping) cannot be freed or invalidated while a
//! hierarchy built from it is alive.

use std::sync::Arc;

use pyo3::prelude::*;

use super::tbo_data_view::{ChannelSet, DataView};
use super::{py_index_error, py_runtime_error, py_value_error};

/// Marker trait for a type that can serve as a keepalive for the memory a
/// `DataView` points into. Implemented by the import-side heap holder and the
/// export-side shared-memory mapping; the concrete type is never inspected.
pub trait Keepalive: Send {}

/// Shared keepalive handle for view memory. The concrete owner type is erased:
/// consumers only ever need the memory to outlive the views, never to read it.
pub type DataBacking = Arc<dyn Keepalive>;

/// One format's contribution to a hierarchy: its views plus matchingly-indexed
/// keepalives.
pub type FormatPair = (Vec<DataView>, Vec<DataBacking>);

/// Shared state for a single level of the TBO hierarchy (scenes, assets, or
/// fragments). One entry per backing file/buffer, concatenated in order.
pub struct CollectionState {
    pub views: Vec<DataView>,
    pub backings: Vec<DataBacking>,
    boundaries: Vec<usize>,
    row_boundaries: Vec<usize>,
    pub channels: ChannelSet,
}

impl Default for CollectionState {
    fn default() -> Self {
        Self::build(Vec::new(), Vec::new()).expect("empty build is valid")
    }
}

impl CollectionState {
    pub fn build(views: Vec<DataView>, backings: Vec<DataBacking>) -> PyResult<Self> {
        if views.len() != backings.len() {
            return Err(py_runtime_error("views/backings length mismatch"));
        }

        // All views in a state must share the same channel layout.
        let channels = views
            .iter()
            .find(|v| !v.channels.is_empty())
            .map(|v| v.channels.clone())
            .unwrap_or_default();
        for v in &views {
            if v.channels.names() != channels.names() {
                return Err(py_value_error(format!(
                    "channel layout mismatch within a format: [{}] vs [{}]",
                    v.channels.names_display(),
                    channels.names_display()
                )));
            }
            v.validate()?;
        }

        let mut boundaries = Vec::new();
        let mut row_boundaries = Vec::new();
        let mut cum = 0usize;
        let mut row_cum = 0usize;
        for v in &views {
            boundaries.push(cum);
            cum += v.entity_count();
            if v.channel_count > 0 {
                let row_bytes = v.bytes_per_element * v.channel_count;
                if v.data_len % row_bytes != 0 {
                    return Err(py_value_error(
                        "data length is not a multiple of row size",
                    ));
                }
                row_cum += v.data_len / row_bytes;
            }
            row_boundaries.push(row_cum);
        }
        boundaries.push(cum);

        Ok(Self {
            views,
            backings,
            boundaries,
            row_boundaries,
            channels,
        })
    }

    pub fn total_entities(&self) -> usize {
        self.boundaries.last().copied().unwrap_or(0)
    }

    /// Total number of data rows across all entities in this state.
    pub fn total_rows(&self) -> usize {
        self.row_boundaries.last().copied().unwrap_or(0)
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
        let (view_idx, local_idx) = self
            .resolve(global_entity_idx)
            .ok_or_else(|| py_index_error("Entity index", global_entity_idx, self.total_entities()))?;
        let rows_before_view = if view_idx > 0 {
            self.row_boundaries[view_idx - 1]
        } else {
            0
        };
        let view = &self.views[view_idx];
        let (start, _) = view.entity_byte_range(local_idx).ok_or_else(|| {
            py_index_error("Entity index", global_entity_idx, self.total_entities())
        })?;
        let row_bytes = view.bytes_per_element * view.channel_count;
        Ok(rows_before_view + start.checked_div(row_bytes).unwrap_or(0))
    }

    /// Number of rows (child entries) in the entity at the given global index.
    pub fn entity_row_count(&self, global_entity_idx: usize) -> PyResult<usize> {
        let (view_idx, local_idx) = self.resolve(global_entity_idx).ok_or_else(|| {
            py_index_error("Entity index", global_entity_idx, self.total_entities())
        })?;
        let view = &self.views[view_idx];
        let (start, end) = view.entity_byte_range(local_idx).ok_or_else(|| {
            py_index_error("Entity index", global_entity_idx, self.total_entities())
        })?;
        let row_bytes = view.bytes_per_element * view.channel_count;
        Ok((end - start).checked_div(row_bytes).unwrap_or(0))
    }

    /// Resolves entity at given index to raw data pointer, length, row stride, channels, and bytes per element.
    pub fn resolve_entity(&self, idx: usize) -> PyResult<(*const u8, usize, usize, ChannelSet, usize)> {
        let total = self.total_entities();
        if total == 0 {
            return Err(PyErr::new::<pyo3::exceptions::PyIndexError, _>(
                "No entities in collection",
            ));
        }
        if idx >= total {
            return Err(PyErr::new::<pyo3::exceptions::PyIndexError, _>(
                format!("Entity index {} out of range (0..{})", idx, total),
            ));
        }
        let (view_idx, local_idx) = self.resolve(idx).expect("bounds checked above");
        let view = &self.views[view_idx];
        let (start, end) = view
            .entity_byte_range(local_idx)
            .ok_or_else(|| py_index_error("Entity index", idx, total))?;
        let data_ptr = unsafe { view.data_ptr.add(start) };
        let data_len = end - start;
        let row_stride = view.channel_count;
        Ok((data_ptr, data_len, row_stride, view.channels.clone(), view.bytes_per_element))
    }
}

/// Verify the cross-format alignment invariants that the hierarchy relies on:
/// each scene entity holds one row per asset child, each asset entity holds one
/// row per fragment child. Checked only for format pairs that are both present.
pub fn validate_cross_format(
    scenes: &CollectionState,
    assets: &CollectionState,
    fragments: &CollectionState,
) -> PyResult<()> {
    let scene_present = !scenes.views.is_empty();
    let asset_present = !assets.views.is_empty();
    let fragment_present = !fragments.views.is_empty();

    if scene_present && asset_present && scenes.total_rows() != assets.total_entities() {
        return Err(py_value_error(format!(
            "hierarchy misaligned: {} scene row(s) != {} asset entit(ies). \
             Load a consistent set of scene/asset/fragment files from the same export.",
            scenes.total_rows(),
            assets.total_entities()
        )));
    }
    if asset_present && fragment_present && assets.total_rows() != fragments.total_entities() {
        return Err(py_value_error(format!(
            "hierarchy misaligned: {} asset row(s) != {} fragment entit(ies). \
             Load a consistent set of asset/fragment files from the same export.",
            assets.total_rows(),
            fragments.total_entities()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tbo::tbo_data_view::{ChannelSet, DataView};

    struct MarkerKeep;
    impl Keepalive for MarkerKeep {}

    fn backing() -> DataBacking {
        Arc::new(MarkerKeep)
    }

    /// DataView over entities with the given f32 spans + its storage.
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
    fn resolve_maps_global_indices_across_views() {
        let (v0, d0, f0) = make_view(&[2, 2], 1);
        let (v1, d1, f1) = make_view(&[1, 1, 1], 1);
        let state = CollectionState::build(vec![v0, v1], vec![backing(), backing()]).unwrap();
        assert_eq!(state.total_entities(), 5);
        assert_eq!(state.resolve(0), Some((0, 0)));
        assert_eq!(state.resolve(1), Some((0, 1)));
        assert_eq!(state.resolve(2), Some((1, 0)));
        assert_eq!(state.resolve(4), Some((1, 2)));
        assert_eq!(state.resolve(5), None);
        let _ = (&d0, &f0, &d1, &f1);
    }

    #[test]
    fn resolve_entity_returns_data_window() {
        let (v0, d0, f0) = make_view(&[4, 2], 2);
        let state = CollectionState::build(vec![v0], vec![backing()]).unwrap();
        let (ptr, len, stride, channels, bytes_per_element) = state.resolve_entity(1).unwrap();
        assert_eq!(ptr, unsafe { d0.as_ptr().add(4) as *const u8 });
        assert_eq!(len, 2);
        assert_eq!(stride, 2);
        assert_eq!(channels.name_count(), 2);
        assert_eq!(bytes_per_element, 4);
        assert!(state.resolve_entity(2).is_err());
        let _ = (&f0, &d0);
    }

    #[test]
    fn row_boundaries_track_entity_rows() {
        let (v0, d0, f0) = make_view(&[4, 2], 2);
        let state = CollectionState::build(vec![v0], vec![backing()]).unwrap();
        // entity 0: 4 f32 / 2 channels = 2 rows; entity 1: 1 row.
        assert_eq!(state.total_rows(), 3);
        assert_eq!(state.entity_row_count(0).unwrap(), 2);
        assert_eq!(state.entity_row_count(1).unwrap(), 1);
        assert_eq!(state.cumulative_rows_before(1).unwrap(), 2);
        let _ = (&d0, &f0);
    }

    #[test]
    fn build_rejects_mismatched_channel_layouts() {
        let (v0, d0, f0) = make_view(&[2], 1);
        let (v1, d1, f1) = make_view(&[2], 2);
        assert!(
            CollectionState::build(vec![v0, v1], vec![backing(), backing()])
                .is_err()
        );
        let _ = (&d0, &f0, &d1, &f1);
    }

    #[test]
    fn build_rejects_views_backings_length_mismatch() {
        let (v0, d0, f0) = make_view(&[2], 1);
        assert!(CollectionState::build(vec![v0], Vec::new()).is_err());
        let _ = (&d0, &f0);
    }

    #[test]
    fn cross_format_alignment_checked() {
        let (s0, sd, sf) = make_view(&[1, 1], 1); // 2 scene entities, 2 rows
        let (a0, ad, af) = make_view(&[1, 1], 1); // 2 asset entities, 2 rows
        let (g0, gd, gf) = make_view(&[1, 1], 1); // 2 fragment entities
        let scenes = CollectionState::build(vec![s0], vec![backing()]).unwrap();
        let assets = CollectionState::build(vec![a0], vec![backing()]).unwrap();
        let fragments = CollectionState::build(vec![g0], vec![backing()]).unwrap();
        assert!(validate_cross_format(&scenes, &assets, &fragments).is_ok());

        // Misaligned: 2 scene rows != 3 asset entities.
        let (a1, ad1, af1) = make_view(&[1, 1, 1], 1);
        let assets_bad = CollectionState::build(vec![a1], vec![backing()]).unwrap();
        assert!(validate_cross_format(&scenes, &assets_bad, &fragments).is_err());
        let _ = (&sd, &sf, &ad, &af, &gd, &gf, &ad1, &af1);
    }
}
