//! TBO (Typed Binary Object) module.
//!
//! Provides zero-copy iterable interfaces for TBO data access via shared memory (export)
//! or heap-backed buffers (import).

mod tbo_collection;
mod tbo_data_view;
mod tbo_entity;
mod tbo_export_context;
mod tbo_file;
mod tbo_hierarchy;
mod tbo_import_context;
mod tbo_reader;
mod tbo_writer;

pub use tbo_entity::HierarchicalEntity;
pub use tbo_export_context::TboExportContext;
pub use tbo_hierarchy::TBOHierarchy;
pub use tbo_import_context::TboImportContext;
