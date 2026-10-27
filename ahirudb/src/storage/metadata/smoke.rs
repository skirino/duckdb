//! Cross-side smoke test for the metadata block-chain streaming kernels.
//!
//! Verifies that the Rust writer and reader kernels are complementary: a
//! write plan produced by [`compute_write_data`] and a read plan produced by
//! [`compute_read_data`] cover the same byte range with the same number of
//! block-boundary transitions. This is the Rust-side half of the design's
//! smoke test; the C++ half lives in the crubit smoke test binary and is
//! exercised by the SQL/storage suites. Together they pin the
//! block-boundary split, the next-block link, and the zero-fill on flush.

#[cfg(test)]
mod tests {
    use super::super::pointer::{get_disk_pointer, idx_t};
    use super::super::reader::compute_read_data;
    use super::super::writer::compute_write_data;

    const METADATA_BLOCK_SIZE: idx_t = 4096;

    /// The writer and reader plans must cover the same byte range with the
    /// same number of block-boundary transitions.
    #[test]
    fn write_and_read_plans_are_complementary() {
        let cases: Vec<(idx_t, idx_t, idx_t)> = vec![
            (0, METADATA_BLOCK_SIZE, 100),                       // single block
            (0, METADATA_BLOCK_SIZE, METADATA_BLOCK_SIZE),       // exactly one block
            (0, METADATA_BLOCK_SIZE, METADATA_BLOCK_SIZE + 512), // two blocks
            (0, METADATA_BLOCK_SIZE, 2 * METADATA_BLOCK_SIZE + 1024), // three blocks
            (100, METADATA_BLOCK_SIZE, 50),                      // offset within block
            (100, METADATA_BLOCK_SIZE, METADATA_BLOCK_SIZE - 50), // fills to end of block
        ];
        for (offset, capacity, write_size) in cases {
            let write_segments =
                compute_write_data(offset, capacity, write_size, METADATA_BLOCK_SIZE);
            let read_segments =
                compute_read_data(offset, capacity, write_size, METADATA_BLOCK_SIZE, 8);

            let write_total: idx_t = write_segments.iter().map(|s| s.copy_amount).sum();
            let read_total: idx_t = read_segments.iter().map(|s| s.copy_amount).sum();
            assert_eq!(
                write_total, write_size,
                "write total (offset={}, size={})",
                offset, write_size
            );
            assert_eq!(
                read_total, write_size,
                "read total (offset={}, size={})",
                offset, write_size
            );

            let write_transitions = write_segments.iter().filter(|s| s.next_block_after).count();
            let read_transitions = read_segments.iter().filter(|s| s.next_block_after).count();
            assert_eq!(
                write_transitions, read_transitions,
                "transition count (offset={}, size={})",
                offset, write_size
            );
        }
    }

    /// A write that fits in one block produces a single non-transitioning
    /// segment, and the matching read does too.
    #[test]
    fn single_block_write_and_read_have_no_transitions() {
        let write_segments = compute_write_data(0, METADATA_BLOCK_SIZE, 100, METADATA_BLOCK_SIZE);
        let read_segments = compute_read_data(0, METADATA_BLOCK_SIZE, 100, METADATA_BLOCK_SIZE, 8);
        assert_eq!(write_segments.len(), 1);
        assert!(!write_segments[0].next_block_after);
        assert_eq!(read_segments.len(), 1);
        assert!(!read_segments[0].next_block_after);
    }

    /// A write that crosses one boundary produces two segments with one
    /// transition each; the read plan matches.
    #[test]
    fn two_block_write_and_read_have_one_transition() {
        let write_segments = compute_write_data(
            0,
            METADATA_BLOCK_SIZE,
            METADATA_BLOCK_SIZE + 512,
            METADATA_BLOCK_SIZE,
        );
        let read_segments = compute_read_data(
            0,
            METADATA_BLOCK_SIZE,
            METADATA_BLOCK_SIZE + 512,
            METADATA_BLOCK_SIZE,
            8,
        );
        assert_eq!(write_segments.len(), 2);
        assert!(write_segments[0].next_block_after);
        assert!(!write_segments[1].next_block_after);
        assert_eq!(read_segments.len(), 2);
        assert!(read_segments[0].next_block_after);
        assert!(!read_segments[1].next_block_after);
    }

    /// The meta block pointer for each block in the chain encodes the block
    /// index and sub-block index correctly.
    #[test]
    fn meta_block_pointers_encode_block_and_subblock() {
        let ptr0 = get_disk_pointer(0, 0, 0);
        assert_eq!(ptr0.get_block_id(), 0);
        assert_eq!(ptr0.get_block_index(), 0);
        let ptr1 = get_disk_pointer(1, 0, 8);
        assert_eq!(ptr1.get_block_id(), 1);
        assert_eq!(ptr1.get_block_index(), 0);
        assert_eq!(ptr1.offset, 8);
    }
}
