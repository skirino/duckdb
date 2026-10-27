//! Free-list bitmask arithmetic for the metadata block layer.
//!

#![allow(nonstandard_style)]

use crate::storage::metadata::pointer::idx_t;

/// Pack a set of free sub-blocks into a 64-bit bitmask.
///
/// Each bit position corresponds to one of the 64 metadata sub-blocks.
pub fn free_blocks_to_integer(free_blocks: &[u8]) -> idx_t {
    let mut result: idx_t = 0;
    for &block in free_blocks {
        debug_assert!((block as idx_t) < 64);
        let mask = 1u64 << block as idx_t;
        result |= mask;
    }
    result
}

/// Unpack a 64-bit bitmask into a sorted list of free sub-blocks.
///
/// The C++ `BlocksFromInteger` iterates from the high bit down to the low,
/// so the returned vector is in descending order (63 first, 0 last).
pub fn blocks_from_integer(free_list: idx_t) -> Vec<u8> {
    let mut blocks = Vec::new();
    for i in (0u32..64).rev() {
        let index = i;
        let mask = 1u64 << index;
        if free_list & mask != 0 {
            blocks.push(index as u8);
        }
    }
    blocks
}

/// Set the free list from a 64-bit bitmask.
///
/// An empty bitmask (0) clears the list; otherwise it is unpacked with
/// [`blocks_from_integer`].
pub fn free_blocks_from_integer(free_list: idx_t) -> Vec<u8> {
    if free_list == 0 {
        return Vec::new();
    }
    blocks_from_integer(free_list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_free_list_is_zero() {
        assert_eq!(free_blocks_to_integer(&[]), 0);
    }

    #[test]
    fn a_full_free_list_is_all_ones() {
        let mut blocks: Vec<u8> = (0u8..64).collect();
        let integer = free_blocks_to_integer(&blocks);
        assert_eq!(integer, idx_t::MAX);
        // round-trip: the unpacked list is descending
        let back = blocks_from_integer(integer);
        blocks.sort();
        let mut back_sorted = back.clone();
        back_sorted.sort();
        assert_eq!(back_sorted, blocks);
    }

    #[test]
    fn a_single_block_round_trips() {
        for i in 0u8..64 {
            let integer = free_blocks_to_integer(&[i]);
            assert_eq!(integer, 1u64 << i as u32);
            let back = blocks_from_integer(integer);
            assert_eq!(back, vec![i]);
        }
    }

    #[test]
    fn blocks_from_integer_is_descending() {
        let integer = free_blocks_to_integer(&[0, 63, 31]);
        let back = blocks_from_integer(integer);
        assert_eq!(back, vec![63, 31, 0]);
    }

    #[test]
    fn zero_yields_an_empty_list() {
        assert!(free_blocks_from_integer(0).is_empty());
    }

    #[test]
    fn every_subset_round_trips() {
        // Exhaustive round-trip over all 2^64 subsets is infeasible, so test
        // the boundary cases and a spread of patterns instead.
        let cases: Vec<idx_t> = vec![
            0,
            1,
            1 << 63,
            0x5555_5555_5555_5555,
            0xaaaa_aaaa_aaaa_aaaa,
            0x0000_0000_0000_000f,
            0xf000_0000_0000_0000,
            0x1234_5678_9abc_def0,
            idx_t::MAX,
        ];
        for integer in cases {
            let blocks = blocks_from_integer(integer);
            let repacked = free_blocks_to_integer(&blocks);
            assert_eq!(repacked, integer);
            let from_integer = free_blocks_from_integer(integer);
            assert_eq!(from_integer, blocks);
        }
    }
}
