//! TBO (Typed Binary Object) module.
//!
//! Streaming export writes scene/asset/fragment buffers through shared memory
//! (`TboExportContext`); import reads `.tbo` files into heap buffers
//! (`TboImportContext`). Both expose the common Scene -> Asset -> Fragment
//! hierarchy (`TBOHierarchy`) for navigation, with per-entity row access via
//! `HierarchicalEntity`.

mod buffer;
mod format;
mod tbo_collection;
mod tbo_data_view;
mod tbo_entity;
mod tbo_export_context;
mod tbo_hierarchy;
mod tbo_import_context;
mod tbo_reader;
mod tbo_writer;

pub use tbo_entity::HierarchicalEntity;
pub use tbo_export_context::TboExportContext;
pub use tbo_hierarchy::TBOHierarchy;
pub use tbo_import_context::TboImportContext;

use pyo3::PyErr;

pub(crate) fn py_runtime_error(msg: impl Into<String>) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(msg.into())
}

pub(crate) fn py_value_error(msg: impl Into<String>) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyValueError, _>(msg.into())
}

pub(crate) fn py_index_error(what: &str, idx: usize, total: usize) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyIndexError, _>(format!("{what} {idx} out of range (0..{total})"))
}

pub(crate) fn py_attr_error(name: &str) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyAttributeError, _>(name.to_string())
}
