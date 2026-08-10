//! TBO file writer for SDK-side export.
//!
//! Writes per-entity data to .tbo files.
//! Format: [header][channel_names][entity_count][offsets][data]

use std::fs::{self, File};
use std::io::{BufWriter, Write};

pub fn write_tbo_file(
    data: &[f32],
    offsets: &[u64],
    channel_names: &[String],
    directory: &str,
    file_prefix: &str,
) -> Result<String, String> {
    let entity_count = offsets.len();
    if entity_count == 0 {
        return Err("No entries to write".to_string());
    }

    fs::create_dir_all(directory).map_err(|e| format!("Failed to create directory {}: {}", directory, e))?;

    let channel_count = channel_names.len() as u32;

    // Find next available filename
    let mut n = 0;
    let mut filename = format!("{}/{}_{}.tbo", directory, file_prefix, n);
    while std::path::Path::new(&filename).exists() {
        n += 1;
        filename = format!("{}/{}_{}.tbo", directory, file_prefix, n);
    }
    let file = File::create(&filename).map_err(|e| format!("Failed to create {}: {}", filename, e))?;
    let mut writer = BufWriter::new(file);

    // Write header
    writer.write_all(b"TBO\0")
        .map_err(|e| format!("Write error: {}", e))?;
    let version: u32 = 3;
    writer.write_all(&version.to_le_bytes())
        .map_err(|e| format!("Write error: {}", e))?;
    let flags: u32 = 0;
    writer.write_all(&flags.to_le_bytes())
        .map_err(|e| format!("Write error: {}", e))?;
    writer.write_all(&(entity_count as u32).to_le_bytes())
        .map_err(|e| format!("Write error: {}", e))?;
    writer.write_all(&channel_count.to_le_bytes())
        .map_err(|e| format!("Write error: {}", e))?;

    // Write channel names
    for name in channel_names {
        writer.write_all(name.as_bytes())
            .map_err(|e| format!("Write error: {}", e))?;
        writer.write_all(b"\0")
            .map_err(|e| format!("Write error: {}", e))?;
    }

    // Write offsets (byte offsets from file start)
    for &off in offsets {
        writer.write_all(&off.to_le_bytes())
            .map_err(|e| format!("Write error: {}", e))?;
    }

    // Write data
    let data_bytes = bytemuck::cast_slice(data);
    writer.write_all(data_bytes)
        .map_err(|e| format!("Write error: {}", e))?;

    writer.flush().map_err(|e| format!("Flush error: {}", e))?;

    Ok(filename)
}
