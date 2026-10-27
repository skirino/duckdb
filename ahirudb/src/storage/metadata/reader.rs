//! Metadata reader arithmetic: block-chain following and
//! offset/capacity state transitions.
//!

#![allow(nonstandard_style)]

use crate::storage::metadata::pointer::from_block_pointer as from_block_pointer_impl;
use crate::storage::metadata::pointer::{BlockPointer, MetaBlockPointer, get_disk_pointer, idx_t};

/// The result of [`compute_read_data`]: a list of copy segments. Each
/// segment copies `copy_amount` bytes from the current block; when
/// `next_block_after` is true, the C++ side calls `ReadNextBlock`.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct ReadSegment {
    pub copy_amount: idx_t,
    pub next_block_after: bool,
}

/// Compute the copy plan for `MetadataReader::ReadData`.
///
/// `next_offset` is the offset `MetadataReader::ReadNextBlock` will install on
/// its first transition this call (it is `pointer.offset` before the first
/// block is loaded, and `sizeof(block_id_t)` = 8 after every subsequent
/// transition). Only the first transition can differ from 8; the C++ shell
/// resets `next_offset` to 8 inside `ReadNextBlock`, so every transition after
/// the first lands at offset 8.
pub fn compute_read_data(
    offset: idx_t,
    mut capacity: idx_t,
    read_size: idx_t,
    metadata_block_size: idx_t,
    next_offset: idx_t,
) -> Vec<ReadSegment> {
    let mut segments = Vec::new();
    let mut offset = offset;
    let mut remaining = read_size;
    let mut first_transition = true;

    while offset + remaining > capacity {
        let to_read = capacity - offset;
        segments.push(ReadSegment {
            copy_amount: to_read,
            next_block_after: true,
        });
        if to_read > 0 {
            remaining -= to_read;
        }
        // After ReadNextBlock: offset = clamp(next_offset, 8) on the first
        // transition, then 8 forever after (the shell resets next_offset).
        offset = if first_transition {
            if next_offset < 8 { 8 } else { next_offset }
        } else {
            8
        };
        first_transition = false;
        capacity = metadata_block_size;
    }
    if remaining > 0 {
        segments.push(ReadSegment {
            copy_amount: remaining,
            next_block_after: false,
        });
    }
    segments
}

/// `MetadataReader::GetMetaBlockPointer` - build the disk pointer from the
/// current block index, sub-block index, and offset.
pub fn get_meta_block_pointer(block_index: idx_t, index: u32, offset: u32) -> MetaBlockPointer {
    get_disk_pointer(block_index, index, offset)
}

/// `MetadataReader::ReadNextBlock` - the next-offset/capacity state after
/// following a block chain link.
pub fn compute_read_next_block_state(
    next_offset: idx_t,
    metadata_block_size: idx_t,
) -> (idx_t, idx_t) {
    let offset = if next_offset < 8 { 8 } else { next_offset };
    (offset, metadata_block_size)
}

/// `MetadataReader::ReadNextBlock` - whether the block chain link at `next_block`
/// is the terminator (-1) or a real pointer.
pub fn chain_link_is_terminal(next_block: idx_t) -> bool {
    next_block == idx_t::MAX as idx_t
}

/// `MetadataReader(manager, BlockPointer)` - convert a block pointer to a
/// meta block pointer.
pub fn from_block_pointer(
    block_pointer: BlockPointer,
    metadata_block_size: idx_t,
) -> Option<MetaBlockPointer> {
    from_block_pointer_impl(block_pointer, metadata_block_size).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_that_fits_in_one_block_is_a_single_segment() {
        let segments = compute_read_data(0, 100, 50, 8192, 8);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].copy_amount, 50);
        assert!(!segments[0].next_block_after);
    }

    #[test]
    fn a_read_that_crosses_one_boundary_produces_two_segments() {
        let segments = compute_read_data(90, 100, 30, 8192, 8);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].copy_amount, 10);
        assert!(segments[0].next_block_after);
        assert_eq!(segments[1].copy_amount, 20);
        assert!(!segments[1].next_block_after);
    }

    #[test]
    fn the_first_transition_uses_next_offset_instead_of_eight() {
        // starting fresh (capacity 0) with a 100-byte pointer offset: the first
        // block read begins at 100, not 8, so the first real copy segment must
        // leave mbs - 100 bytes for the rest of the block.
        let segments = compute_read_data(0, 0, 8200, 8192, 100);
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].copy_amount, 0);
        assert!(segments[0].next_block_after);
        assert_eq!(segments[1].copy_amount, 8192 - 100);
        assert!(segments[1].next_block_after);
        assert_eq!(segments[2].copy_amount, 108);
        assert!(!segments[2].next_block_after);
    }

    #[test]
    fn a_next_offset_below_eight_is_clamped_up_to_eight() {
        let segments = compute_read_data(0, 0, 8200, 8192, 3);
        assert_eq!(segments[1].copy_amount, 8192 - 8);
    }

    #[test]
    fn read_next_block_state_clamps_next_offset_to_block_id_size() {
        let (offset, capacity) = compute_read_next_block_state(4, 8192);
        assert_eq!(offset, 8);
        assert_eq!(capacity, 8192);
        let (offset, _) = compute_read_next_block_state(20, 8192);
        assert_eq!(offset, 20);
    }

    #[test]
    fn chain_link_is_terminal_only_for_minus_one() {
        assert!(chain_link_is_terminal(idx_t::MAX));
        assert!(!chain_link_is_terminal(0));
        assert!(!chain_link_is_terminal(42));
    }

    #[test]
    fn a_block_pointer_round_trips_through_from_block_pointer() {
        let block = BlockPointer::new(7, 3 * 8192 + 100);
        let meta = from_block_pointer(block, 8192).unwrap();
        assert_eq!(meta.get_block_id(), 7);
        assert_eq!(meta.get_block_index(), 3);
        assert_eq!(meta.offset, 100);
    }
}
