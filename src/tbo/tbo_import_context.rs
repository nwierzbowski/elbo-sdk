//! TBO Import Context - SDK-side handler for loading TBO files from disk.
//!
//! Reads TBO files from disk into heap-allocated buffers and exposes them
//! through the same hierarchy interface as TboExportContext.

use std::path::Path;
use std::sync::Arc;

use pyo3::prelude::*;

use super::format;
use super::tbo_collection::{DataBacking, FormatPair, Keepalive};
use super::tbo_data_view::{ChannelSet, DataView};
use super::tbo_hierarchy::{build_hierarchy, TBOHierarchy};
use super::tbo_reader;

/// Owns the heap data and header metadata for one loaded TBO file.
/// Held behind `Arc` so hierarchies built from a file keep its memory alive
/// even after the file is unloaded from the context.
pub struct DataHolder {
    pub path: String,
    pub data: Vec<f32>,
    pub offsets: Vec<u64>,
    pub channel_names: Vec<String>,
    pub format_index: u32,
    pub version: u32,
    pub flags: u32,
    pub entity_count: u32,
    pub channel_count: u32,
    pub data_start: u64,
}

impl Keepalive for DataHolder {}

#[pyclass(unsendable)]
pub struct TboImportContext {
    /// Slot i corresponds to load_file's returned index i. Unloaded slots are None,
    /// so remaining indices stay stable.
    holders: Vec<Option<Arc<DataHolder>>>,
}

fn no_data_error(path_or_idx: String) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("No data available: {}", path_or_idx))
}

fn natural_key(path: &str) -> (String, u64) {
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = Path::new(&name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or(name);
    let digits_start = stem
        .rfind(|c: char| !c.is_ascii_digit())
        .map(|i| i + 1)
        .unwrap_or(0);
    let (prefix, digits) = stem.split_at(digits_start);
    let number = digits.parse::<u64>().unwrap_or(0);
    (prefix.to_string(), number)
}

#[pymethods]
impl TboImportContext {
    #[new]
    fn new() -> Self {
        Self {
            holders: Vec::new(),
        }
    }

    /// Load a TBO file from disk into a heap-allocated buffer.
    /// Returns the file index used by unload_file.
    fn load_file(&mut self, py: Python, path: &str) -> PyResult<usize> {
        let tbo = py
            .detach(|| tbo_reader::read_tbo_file(path))
            .map_err(PyErr::new::<pyo3::exceptions::PyRuntimeError, String>)?;

        let data_start = format::data_start(&tbo.channel_names) as u64;
        let holder = DataHolder {
            path: path.to_string(),
            data: tbo.data,
            offsets: tbo.offsets,
            channel_names: tbo.channel_names,
            format_index: tbo.format_index,
            version: tbo.version,
            flags: tbo.flags,
            entity_count: tbo.entity_count,
            channel_count: tbo.channel_count,
            data_start,
        };

        let idx = self.holders.len();
        self.holders.push(Some(Arc::new(holder)));
        Ok(idx)
    }

    /// Unload a file by index. The index slot is kept, so other indices stay valid.
    fn unload_file(&mut self, file_idx: usize) -> PyResult<()> {
        if file_idx >= self.holders.len() || self.holders[file_idx].is_none() {
            return Err(no_data_error(format!("file index {}", file_idx)));
        }
        self.holders[file_idx] = None;
        Ok(())
    }

    fn get_file_info(&self, path: &str) -> PyResult<(u32, u32, u32, u32, Vec<String>)> {
        let holder = self
            .holders
            .iter()
            .flatten()
            .find(|h| h.path == path)
            .ok_or_else(|| no_data_error(format!("File not loaded: {}", path)))?;
        Ok((
            holder.version,
            holder.flags,
            holder.entity_count,
            holder.channel_count,
            holder.channel_names.clone(),
        ))
    }

    fn unload_file_by_path(&mut self, path: &str) -> PyResult<()> {
        let file_index = self
            .holders
            .iter()
            .enumerate()
            .find(|(_, h)| h.as_ref().is_some_and(|h| h.path == path))
            .map(|(i, _)| i)
            .ok_or_else(|| no_data_error(format!("File not loaded: {}", path)))?;
        self.holders[file_index] = None;
        Ok(())
    }

    /// Build a TBOHierarchy linking scene, asset, and fragment collections.
    fn get_hierarchy(&self, _py: Python) -> PyResult<TBOHierarchy> {
        build_hierarchy(self.format_views(0)?, self.format_views(1)?, self.format_views(2)?)
    }
}

impl TboImportContext {
    fn format_views(&self, format_index: u32) -> PyResult<FormatPair> {
        let mut indexed: Vec<usize> = self
            .holders
            .iter()
            .enumerate()
            .filter(|(_, h)| h.as_ref().is_some_and(|h| h.format_index == format_index))
            .map(|(i, _)| i)
            .collect();
        indexed.sort_by_key(|&i| natural_key(&self.holders[i].as_ref().unwrap().path));

        let mut views = Vec::new();
        let mut backings: Vec<DataBacking> = Vec::new();
        for i in indexed {
            let holder = Arc::clone(
                self.holders[i]
                    .as_ref()
                    .ok_or_else(|| no_data_error(format!("file slot {}", i)))?,
            );
            views.push(DataView::new(
                holder.data.as_ptr(),
                holder.data.len(),
                holder.offsets.as_ptr(),
                holder.offsets.len(),
                holder.data_start,
                ChannelSet::from_names(holder.channel_names.clone()),
            ));
            let holder_arc = Arc::clone(&holder);
            let backing: DataBacking = holder_arc;
            backings.push(backing);
        }
        Ok((views, backings))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_key_orders_trailing_numbers_numerically() {
        let mut paths = vec!["scene_10.tbo", "scene_2.tbo", "scene_1.tbo"];
        paths.sort_by_key(|p| natural_key(p));
        assert_eq!(paths, &["scene_1.tbo", "scene_2.tbo", "scene_10.tbo"]);
    }

    #[test]
    fn natural_key_ignores_directories() {
        let mut paths = vec!["/tmp/exports/scene_2.tbo", "/tmp/exports/scene_10.tbo"];
        paths.sort_by_key(|p| natural_key(p));
        assert_eq!(paths, &["/tmp/exports/scene_2.tbo", "/tmp/exports/scene_10.tbo"]);
    }

    #[test]
    fn natural_key_without_digits_sorts_by_name() {
        let mut paths = vec!["zeta.tbo", "alpha.tbo"];
        paths.sort_by_key(|p| natural_key(p));
        assert_eq!(paths, &["alpha.tbo", "zeta.tbo"]);
    }
}
