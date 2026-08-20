//! TBO file reader for SDK-side import.
//!
//! Reads per-entity data from .tbo files (see [`super::format`] for the layout).
//! Offsets hold entity_count + 1 u64 values, in ascending address order.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};

use super::format::{self, HEADER_SIZE, TBO_VERSION};

#[derive(Debug)]
pub struct TboFile {
    pub format_index: u32,
    pub version: u32,
    pub flags: u32,
    pub entity_count: u32,
    pub channel_count: u32,
    pub channel_names: Vec<String>,
    pub data: Vec<u8>,
    pub offsets: Vec<u64>,
}

fn read_u32_le(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn read_null_terminated(reader: &mut BufReader<File>, path: &str) -> Result<String, String> {
    let mut name = Vec::new();
    let n = reader
        .read_until(b'\0', &mut name)
        .map_err(|e| format!("Failed to read channel name in {}: {}", path, e))?;
    if n == 0 || name.is_empty() || !name.ends_with(&[0u8]) {
        return Err(format!("Unexpected end of file in channel names in {}", path));
    }
    name.pop();
    String::from_utf8(name).map_err(|_| format!("Invalid UTF-8 channel name in {}", path))
}

pub fn read_tbo_file(path: &str) -> Result<TboFile, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open {}: {}", path, e))?;
    let mut reader = BufReader::new(file);

    let mut header = [0u8; HEADER_SIZE];
    reader
        .read_exact(&mut header)
        .map_err(|e| format!("Failed to read header from {}: {}", path, e))?;

    let magic = &header[0..4];
    if magic != &format::MAGIC[..] {
        return Err(format!("Invalid TBO magic in {}", path));
    }

    let format_index = read_u32_le(&header[4..8]);
    let version = read_u32_le(&header[8..12]);
    let flags = read_u32_le(&header[12..16]);
    let entity_count = read_u32_le(&header[16..20]);
    let channel_count = read_u32_le(&header[20..24]);

    if version != TBO_VERSION {
        return Err(format!(
            "Unsupported TBO version {} in {} (expected {})",
            version, path, TBO_VERSION
        ));
    }

    let mut channel_names = Vec::with_capacity(channel_count as usize);
    for _ in 0..channel_count {
        channel_names.push(read_null_terminated(&mut reader, path)?);
    }

    let data_start = format::data_start(&channel_names) as u64;
    let offsets_size = format::offset_count(entity_count as u64) as u64 * 8;

    let file_size = reader
        .seek(SeekFrom::End(0))
        .map_err(|e| format!("Failed to seek in {}: {}", path, e))?;
    if file_size < data_start + offsets_size {
        return Err(format!(
            "TBO file {} truncated: size {} smaller than required {}",
            path,
            file_size,
            data_start + offsets_size
        ));
    }
    let data_end = file_size - offsets_size;
    let data_bytes = (data_end - data_start) as usize;
    
    // Validate alignment based on format
    // Format 4 (faces) uses u64 (8-byte aligned), others use f32 (4-byte aligned)
    let required_alignment = if format_index == 4 { 8 } else { 4 };
    if !data_bytes.is_multiple_of(required_alignment) {
        return Err(format!(
            "TBO file {} data region ({} bytes) is not a multiple of {} for format {}",
            path, data_bytes, required_alignment, format_index
        ));
    }

    reader
        .seek(SeekFrom::Start(data_start))
        .map_err(|e| format!("Failed to seek data in {}: {}", path, e))?;

    let mut data = vec![0u8; data_bytes];
    reader
        .read_exact(&mut data)
        .map_err(|e| format!("Failed to read data in {}: {}", path, e))?;

    let offset_count = (offsets_size / 8) as usize;
    let mut offsets = vec![0u64; offset_count];
    reader
        .read_exact(unsafe {
            std::slice::from_raw_parts_mut(offsets.as_mut_ptr() as *mut u8, offsets_size as usize)
        })
        .map_err(|e| format!("Failed to read offsets in {}: {}", path, e))?;

    Ok(TboFile {
        format_index,
        version,
        flags,
        entity_count,
        channel_count,
        channel_names,
        data,
        offsets,
    })
}
