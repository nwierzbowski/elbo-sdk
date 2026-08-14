//! Unified file handle for both export (SHM-backed) and import (heap-backed) workflows.


/// Unified file handle for both export (SHM-backed) and import (heap-backed) workflows.
///
/// The same struct is used by TboExportContext and TboImportContext, providing a consistent
/// interface regardless of whether the data comes from shared memory or disk.
#[derive(Debug)]
pub struct LoadedFile {
    pub path: String,
    pub data_len: usize,
    pub offset_len: usize,
    pub channel_names: Vec<String>,
    pub format_index: u32,
    pub version: u32,
    pub flags: u32,
    pub entity_count: u32,
    pub channel_count: u32,
    pub data_holder_index: Option<usize>,
}
