//! TBO file format: constants, layout math, and header writing.
//!
//! Layout: [24-byte header][channel names][padding to 16][data] ... [offsets]
//!
//! Header: [4 magic "TBO\0"][4 format_index][4 version][4 flags][4 entity_count][4 channel_count]
//!
//! The top u64 of the offset region is seeded with `data_start`, so a buffer (or file)
//! holding N entities stores N + 1 offsets.

/// File format magic: b"TBO\0".
pub const MAGIC: [u8; 4] = *b"TBO\0";
/// Little-endian u32 encoding of [`MAGIC`], as stored in the header.
pub const MAGIC_U32: u32 = u32::from_le_bytes(MAGIC);
/// Size of the fixed header, in bytes.
pub const HEADER_SIZE: usize = 24;
/// Format version supported by this build.
pub const TBO_VERSION: u32 = 4;

/// Round up to a 16-byte boundary.
pub const fn align16(value: usize) -> usize {
    (value + 15) & !15
}

/// Total byte size of the null-terminated channel-name block.
pub fn channel_names_size(channel_names: &[String]) -> usize {
    channel_names.iter().map(|n| n.len() + 1).sum()
}

/// Buffer-/file-relative byte offset where the data region starts, given the
/// channel names following the header.
pub fn data_start(channel_names: &[String]) -> usize {
    align16(HEADER_SIZE + channel_names_size(channel_names))
}

/// Number of u64 offsets needed for `entity_count` entities (one per entity end
/// plus the seeded `data_start`).
pub fn offset_count(entity_count: u64) -> usize {
    entity_count as usize + 1
}

/// Write the 24-byte header and channel-name block at `base`, and seed the top
/// u64 of the buffer (the first offset, in write order) with `data_start`.
pub fn write_header(
    base: *mut u8,
    buffer_size: usize,
    format_index: u32,
    channel_names: &[String],
    data_start: usize,
) {
    unsafe {
        std::ptr::write_unaligned(base.add(0) as *mut u32, MAGIC_U32);
        std::ptr::write_unaligned(base.add(4) as *mut u32, format_index);
        std::ptr::write_unaligned(base.add(8) as *mut u32, TBO_VERSION);
        std::ptr::write_unaligned(base.add(12) as *mut u32, 0); // flags
        std::ptr::write_unaligned(base.add(16) as *mut u32, 0); // entity_count
        std::ptr::write_unaligned(base.add(20) as *mut u32, channel_names.len() as u32);
        let mut offset = HEADER_SIZE;
        for name in channel_names {
            let bytes = name.as_bytes();
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), base.add(offset), bytes.len());
            offset += bytes.len();
            *base.add(offset) = 0;
            offset += 1;
        }
        let top = base as usize + buffer_size - 8;
        std::ptr::write_unaligned(top as *mut u64, data_start as u64);
    }
}

/// Update the entity_count field (header offset 16) of an existing header.
pub fn set_entity_count(base: *mut u8, count: u32) {
    unsafe {
        std::ptr::write_unaligned(base.add(16) as *mut u32, count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align16_rounds_up_to_boundary() {
        assert_eq!(align16(0), 0);
        assert_eq!(align16(15), 16);
        assert_eq!(align16(16), 16);
        assert_eq!(align16(17), 32);
        assert_eq!(align16(24), 32);
    }

    #[test]
    fn data_start_with_no_channels() {
        // The header alone still rounds up to the 16-byte boundary.
        assert_eq!(data_start(&[]), align16(HEADER_SIZE));
        assert_eq!(data_start(&[]), 32);
    }

    #[test]
    fn data_start_accounts_for_null_terminated_names() {
        // header 24 + "a\0" (2) + "bc\0" (3) = 29 -> aligned to 32
        let names = vec!["a".to_string(), "bc".to_string()];
        assert_eq!(channel_names_size(&names), 5);
        assert_eq!(data_start(&names), 32);
    }

    #[test]
    fn offset_count_includes_seed_offset() {
        assert_eq!(offset_count(0), 1);
        assert_eq!(offset_count(4), 5);
    }

    #[test]
    fn magic_constant_matches_bytes() {
        assert_eq!(MAGIC, *b"TBO\0");
        assert_eq!(MAGIC_U32, u32::from_le_bytes(MAGIC));
    }
}
