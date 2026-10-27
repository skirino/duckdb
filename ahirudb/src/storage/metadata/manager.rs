//! Metadata manager arithmetic: block size and modified-block bitmask
//! tracking.
//!
//! The pure kernels here are the ones the `MetadataManager` C++ shell
//! reaches for through the bridge. The free-list helpers themselves live in
//! [`free_list`]; this module only wraps the handful the shell calls.

#![allow(nonstandard_style)]

use crate::storage::metadata::pointer::{METADATA_BLOCK_COUNT, idx_t};

/// `MetadataManager::GetMetadataBlockSize` - divide the storage block size by
/// the number of metadata blocks packed into it, then truncate toward zero
/// in 8-byte units. This is `AlignValueFloor(block_size / METADATA_BLOCK_COUNT)`
/// from the C++ original.
pub fn get_metadata_block_size(block_size: idx_t) -> idx_t {
    AlignValueFloor(block_size / METADATA_BLOCK_COUNT)
}

/// Truncate `n` toward zero in 8-byte units, matching the C++ `AlignValueFloor`
/// default (`val = 8`).
fn AlignValueFloor(n: idx_t) -> idx_t {
    (n / 8) * 8
}

/// `MetadataManager::ClearModifiedBlocks` - clear one bit from a modified
/// list. Returns the cleared list.
pub fn clear_modified_bit(modified: idx_t, block_index: u32) -> idx_t {
    modified & !(1u64 << block_index)
}

/// `MetadataManager::BlockHasBeenCleared` - whether the bit for `block_index`
/// is unset in the modified list.
pub fn bit_is_cleared(modified: idx_t, block_index: u32) -> bool {
    (modified & (1u64 << block_index)) == 0
}

/// `MetadataManager::MarkBlocksAsModified` - build the occupied (non-free)
/// bitmask for a block from its free list.
pub fn occupied_blocks(free_list: idx_t) -> idx_t {
    !free_list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_metadata_block_size_divides_by_metadata_block_count_then_aligns_to_eight() {
        // `AlignValueFloor(block_size / METADATA_BLOCK_COUNT)` with METADATA_BLOCK_COUNT = 64
        assert_eq!(get_metadata_block_size(0), 0);
        assert_eq!(get_metadata_block_size(63), 0);
        assert_eq!(get_metadata_block_size(64), 0); // 64/64=1 -> AlignValueFloor(1)=0
        assert_eq!(get_metadata_block_size(128), 0); // 128/64=2 -> AlignValueFloor(2)=0
        assert_eq!(get_metadata_block_size(512), 8); // 512/64=8 -> AlignValueFloor(8)=8
        assert_eq!(get_metadata_block_size(575), 8); // 575/64=8 -> AlignValueFloor(8)=8
        // default storage block size 262144 -> 4096-byte metadata blocks
        assert_eq!(get_metadata_block_size(262144), 4096);
        assert_eq!(get_metadata_block_size(262151), 4096);
    }

    #[test]
    fn clear_modified_bit_unsets_one_position() {
        let modified = 0b111;
        assert_eq!(clear_modified_bit(modified, 1), 0b101);
        assert_eq!(clear_modified_bit(modified, 0), 0b110);
    }

    #[test]
    fn bit_is_cleared_reads_the_position() {
        assert!(bit_is_cleared(0b101, 1));
        assert!(!bit_is_cleared(0b101, 0));
    }

    #[test]
    fn occupied_blocks_is_the_complement_of_the_free_list() {
        assert_eq!(occupied_blocks(0), idx_t::MAX);
        assert_eq!(occupied_blocks(idx_t::MAX), 0);
        assert_eq!(occupied_blocks(0b010), !0b010);
    }
}
