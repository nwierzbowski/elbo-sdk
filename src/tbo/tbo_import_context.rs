//! TBO Import Context - SDK-side handler for loading TBO files from disk.
//!
//! Reads TBO files from disk into heap-allocated buffers and exposes them
//! through the same LoadedFile interface as TboExportContext.

use pyo3::prelude::*;

use super::tbo_file::LoadedFile;
use super::tbo_reader;
use super::tbo_data_view::DataView;
use super::tbo_collection::CollectionState;

/// Owns heap-allocated data for a single loaded TBO file.
/// Stored in Vec<Option<>> so indices remain stable when files are unloaded.
struct DataHolder {
    data: Vec<f32>,
    offsets: Vec<u64>,
}

#[pyclass(unsendable)]
pub struct TboImportContext {
    heap_data: Vec<Option<DataHolder>>,
    loaded_files: Vec<LoadedFile>,
}

#[pymethods]
impl TboImportContext {
    #[new]
    fn new() -> Self {
        Self {
            heap_data: Vec::new(),
            loaded_files: Vec::new(),
        }
    }

    /// Load a TBO file from disk into heap-allocated buffer.
    fn load_file(&mut self, path: &str) -> PyResult<usize> {
        let tbo = tbo_reader::read_tbo_file(path)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e))?;

        let data_holder_idx = self.heap_data.len();
        self.heap_data.push(Some(DataHolder {
            data: tbo.data.clone(),
            offsets: tbo.offsets.clone(),
        }));

        let file = LoadedFile {
            path: path.to_string(),
            data_len: tbo.data.len(),
            offset_len: tbo.offsets.len(),
            channel_names: tbo.channel_names,
            format_index: tbo.format_index,
            version: tbo.version,
            flags: tbo.flags,
            entity_count: tbo.entity_count,
            channel_count: tbo.channel_count,
            data_holder_index: Some(data_holder_idx),
        };

        let idx = self.loaded_files.len();
        self.loaded_files.push(file);
        Ok(idx)
    }

    /// Unload a file by index.
    fn unload_file(&mut self, file_idx: usize) -> PyResult<()> {
        if file_idx >= self.loaded_files.len() {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Invalid file index"));
        }
        let file = &self.loaded_files[file_idx];
        if let Some(holder_idx) = file.data_holder_index {
            self.heap_data[holder_idx] = None;
        }
        self.loaded_files.remove(file_idx);
        Ok(())
    }

    fn get_file_info(&self, path: &str) -> PyResult<(u32, u32, u32, u32, Vec<String>)> {
        let file = self.loaded_files.iter().find(|f| f.path == path)
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("File not loaded: {}", path)))?;
        Ok((
            file.version,
            file.flags,
            file.entity_count,
            file.channel_count,
            file.channel_names.clone(),
        ))
    }

    fn unload_file_by_path(&mut self, path: &str) -> PyResult<()> {
        let file_index = self.loaded_files.iter().position(|f| f.path == path)
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("File not loaded: {}", path)))?;
        self.unload_file(file_index)
    }

    /// Build a TBOHierarchy linking scene, asset, and fragment collections.
    fn get_hierarchy(&self, _py: Python) -> PyResult<super::tbo_hierarchy::TBOHierarchy> {
        let scenes = CollectionState::build(self.build_views_for_format(0)?);
        let assets = CollectionState::build(self.build_views_for_format(1)?);
        let fragments = CollectionState::build(self.build_views_for_format(2)?);

        Ok(super::tbo_hierarchy::TBOHierarchy::new(scenes, assets, fragments))
    }
}

impl TboImportContext {
    fn build_views_for_format(&self, format_index: u32) -> PyResult<Vec<DataView>> {
        // Collect matching files and sort by filename for deterministic iteration order.
        let mut matching: Vec<&LoadedFile> = self.loaded_files
            .iter()
            .filter(|f| f.format_index == format_index)
            .collect();
        matching.sort_by_key(|f| {
            std::path::Path::new(&f.path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });

        let mut views = Vec::new();
        for file in matching {
            let holder_idx = file.data_holder_index
                .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("No data holder"))?;
            let holder = self.heap_data.get(holder_idx)
                .and_then(|h| h.as_ref())
                .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Data holder freed"))?;

            // Compute data_start from file layout
            let data_start = 24u64 + file.channel_names.iter()
                .map(|n| n.len() as u64 + 1)
                .sum::<u64>();

            let view = DataView::new(
                holder.data.as_ptr(),
                holder.data.len(),
                holder.offsets.as_ptr(),
                holder.offsets.len(),
                data_start,
                file.channel_names.clone(),
            );
            views.push(view);
        }
        Ok(views)
    }
}
