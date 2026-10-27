//! Metadata writer arithmetic: block-boundary crossing and
//! offset/capacity state transitions.
//!

#![allow(nonstandard_style)]

use crate::storage::metadata::pointer::{
    BlockPointer, MetaBlockPointer, get_disk_pointer, idx_t, to_block_pointer,
};

/// One copy segment produced by [`compute_write_data`].
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct WriteSegment {
    pub copy_amount: idx_t,
    pub next_block_after: bool,
}

/// The offset/capacity state after a [`NextBlock`] transition.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct NextBlockState {
    pub offset: idx_t,
    pub capacity: idx_t,
}

/// Compute the copy plan for `MetadataWriter::WriteData`.
///
/// Returns a list of segments; for each, C++ copies `copy_amount` bytes
/// and, when `next_block_after` is true, calls `NextBlock`. The plan
/// crosses block boundaries exactly the way the C++ loop does.
pub fn compute_write_data(
    offset: idx_t,
    mut capacity: idx_t,
    write_size: idx_t,
    metadata_block_size: idx_t,
) -> Vec<WriteSegment> {
    let mut segments = Vec::new();
    let mut offset = offset;
    let mut remaining = write_size;

    while offset + remaining > capacity {
        let copy_amount = capacity - offset;
        segments.push(WriteSegment {
            copy_amount,
            next_block_after: true,
        });
        if copy_amount > 0 {
            remaining -= copy_amount;
        }
        // After NextBlock: offset = sizeof(idx_t), capacity = metadata_block_size
        offset = 8;
        capacity = metadata_block_size;
    }
    if remaining > 0 {
        segments.push(WriteSegment {
            copy_amount: remaining,
            next_block_after: false,
        });
    }
    segments
}

/// Compute the offset/capacity state after `MetadataWriter::NextBlock`.
pub fn compute_next_block_state(metadata_block_size: idx_t) -> NextBlockState {
    NextBlockState {
        offset: 8, // sizeof(idx_t)
        capacity: metadata_block_size,
    }
}

/// `MetadataWriter::GetMetaBlockPointer` - build the disk pointer from the
/// current block index, sub-block index, and offset.
pub fn get_meta_block_pointer(block_index: idx_t, index: u32, offset: u32) -> MetaBlockPointer {
    get_disk_pointer(block_index, index, offset)
}

/// `MetadataWriter::GetBlockPointer` - convert a meta block pointer to a
/// block pointer using the metadata block size.
pub fn get_block_pointer(
    meta_pointer: MetaBlockPointer,
    metadata_block_size: idx_t,
) -> Option<BlockPointer> {
    to_block_pointer(meta_pointer, metadata_block_size).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_write_that_fits_in_one_block_is_a_single_segment() {
        let segments = compute_write_data(0, 100, 50, 8192);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].copy_amount, 50);
        assert!(!segments[0].next_block_after);
    }

    #[test]
    fn a_write_that_crosses_one_boundary_produces_two_segments() {
        let segments = compute_write_data(90, 100, 30, 8192);
        // first segment: 10 bytes, then NextBlock; second: 20 bytes
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].copy_amount, 10);
        assert!(segments[0].next_block_after);
        assert_eq!(segments[1].copy_amount, 20);
        assert!(!segments[1].next_block_after);
    }

    #[test]
    fn a_write_starting_at_capacity_still_transitions() {
        let segments = compute_write_data(100, 100, 10, 8192);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].copy_amount, 0);
        assert!(segments[0].next_block_after);
        assert_eq!(segments[1].copy_amount, 10);
        assert!(!segments[1].next_block_after);
    }

    #[test]
    fn next_block_state_resets_offset_and_capacity() {
        let state = compute_next_block_state(8192);
        assert_eq!(state.offset, 8);
        assert_eq!(state.capacity, 8192);
    }

    #[test]
    fn a_meta_block_pointer_round_trips_through_get_block_pointer() {
        let meta = get_meta_block_pointer(7, 3, 100);
        let block = get_block_pointer(meta, 8192).unwrap();
        assert_eq!(block.block_id, 7);
        assert_eq!(block.offset as idx_t, 3 * 8192 + 100);
    }
}
