//! TBO file writer for SDK-side export.
//!
//! Writes buffer directly to disk. Buffer must be in TBO format:
//! [24-byte header][channel names][data][offsets]
//! Header: [4: magic "TBO\0"][4: format_index][4: version][4: flags][4: entity_count][4: channel_count]

use std::fs::{self, File};
use std::io::{BufWriter, Write};

pub fn write_tbo_file(
    buffer: &[u8],
    data_ptr: usize,
    offset_ptr: usize,
    directory: &str,
    file_prefix: &str,
) -> Result<String, String> {
    fs::create_dir_all(directory).map_err(|e| format!("Failed to create directory {}: {}", directory, e))?;

    // Find next available filename
    let mut n = 0;
    let mut filename = format!("{}/{}_{}.tbo", directory, file_prefix, n);
    while std::path::Path::new(&filename).exists() {
        n += 1;
        filename = format!("{}/{}_{}.tbo", directory, file_prefix, n);
    }
    let file = File::create(&filename).map_err(|e| format!("Failed to create {}: {}", filename, e))?;
    let mut writer = BufWriter::new(file);

    // Write header + channel names + data (contiguous in buffer)
    writer.write_all(&buffer[..data_ptr])
        .map_err(|e| format!("Write error: {}", e))?;

    // Write offsets (at end of buffer)
    writer.write_all(&buffer[offset_ptr..])
        .map_err(|e| format!("Write error: {}", e))?;

    writer.flush().map_err(|e| format!("Flush error: {}", e))?;

    Ok(filename)
}
