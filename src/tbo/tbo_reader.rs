//! TBO file reader for SDK-side import.
//!
//! Reads per-entity data from .tbo files.
//! Format: [24-byte header][channel names][data][offsets]
//! Header: [4: magic "TBO\0"][4: format_index][4: version][4: flags][4: entity_count][4: channel_count]

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

#[derive(Debug)]
pub struct TboFile {
    pub format_index: u32,
    pub version: u32,
    pub flags: u32,
    pub entity_count: u32,
    pub channel_count: u32,
    pub channel_names: Vec<String>,
    pub data: Vec<f32>,
    pub offsets: Vec<u64>,
}

pub fn read_tbo_file(path: &str) -> Result<TboFile, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open {}: {}", path, e))?;
    let mut reader = file;

    // Read header (24 bytes)
    let mut header = [0u8; 24];
    reader.read_exact(&mut header).map_err(|e| format!("Failed to read header: {}", e))?;

    let magic = &header[0..4];
    if magic != b"TBO\0" {
        return Err(format!("Invalid TBO magic in {}", path));
    }

    let format_index = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    let version = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
    let flags = u32::from_le_bytes([header[12], header[13], header[14], header[15]]);
    let entity_count = u32::from_le_bytes([header[16], header[17], header[18], header[19]]);
    let channel_count = u32::from_le_bytes([header[20], header[21], header[22], header[23]]);

    // Read channel names (null-terminated)
    let mut channel_names = Vec::with_capacity(channel_count as usize);
    for _ in 0..channel_count {
        let mut name_bytes = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            reader.read_exact(&mut byte).map_err(|e| format!("Failed to read channel name: {}", e))?;
            if byte[0] == 0 {
                break;
            }
            name_bytes.push(byte[0]);
        }
        channel_names.push(String::from_utf8_lossy(&name_bytes).to_string());
    }

    // Calculate offsets section size
    let offsets_size = (entity_count as u64) * 8;

    // Read data (from header_end to file_size - offsets_size)
    let file_size = reader.seek(SeekFrom::End(0)).map_err(|e| format!("Failed to seek: {}", e))?;
    let data_start = 24 + channel_names.iter().map(|n| n.len() as u64 + 1).sum::<u64>();
    let data_end = file_size - offsets_size;
    let data_byte_count = (data_end - data_start) as usize;
    let _data_len = data_byte_count / 4;

    reader.seek(SeekFrom::Start(data_start)).map_err(|e| format!("Failed to seek: {}", e))?;

    let data_f32_count = data_byte_count / 4;
    let mut data_u32 = vec![0u32; data_f32_count];
    reader.read_exact(unsafe {
        std::slice::from_raw_parts_mut(
            data_u32.as_mut_ptr() as *mut u8,
            data_byte_count,
        )
    }).map_err(|e| format!("Failed to read data: {}", e))?;
    let data: Vec<f32> = bytemuck::cast_slice(&data_u32).to_vec();

    // Read offsets (last entity_count * 8 bytes)
    let offset_u64_count = offsets_size as usize / 8;
    let mut offset_u64 = vec![0u64; offset_u64_count];
    reader.read_exact(unsafe {
        std::slice::from_raw_parts_mut(
            offset_u64.as_mut_ptr() as *mut u8,
            offsets_size as usize,
        )
    }).map_err(|e| format!("Failed to read offsets: {}", e))?;
    let offsets: Vec<u64> = offset_u64;

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
