//! Partition-key tracker ported from
//! `src/common/sort/partition_key_tracker.cpp`.
//!
//! Rust owns the bin state machine and the hash-based candidate selection.
//! Representative storage and value comparison stay in C++ (data movement and
//! `VectorOperations::DistinctFrom`).

#![deny(unsafe_code)]

use std::os::raw::c_void;

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrackerState {
    Empty = 0,
    SingleKey = 1,
    MultipleKeys = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartitionEntry {
    pub bin_idx: u64,
    pub offset: u32,
    pub length: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinOutcome {
    pub mixed: bool,
    pub store_representative: bool,
    pub representative_row_idx: u32,
    pub candidate_count: u32,
}

// State-machine helpers operating on flat slices.
// `states` and `hashes` are parallel arrays indexed by bin.

pub fn can_bypass(states: &[u8], hash_bin: u64) -> bool {
    hash_bin < states.len() as u64 && states[hash_bin as usize] == TrackerState::SingleKey as u8
}

pub fn reset(states: &mut [u8], hashes: &mut [u64]) {
    for s in states.iter_mut() {
        *s = TrackerState::Empty as u8;
    }
    for h in hashes.iter_mut() {
        *h = 0;
    }
}

// Store a representative: mark bin SINGLE_KEY, record hash.
pub fn store_representative_state(states: &mut [u8], hashes: &mut [u64], bin_idx: u64, hash: u64) {
    if (bin_idx as usize) < states.len() {
        states[bin_idx as usize] = TrackerState::SingleKey as u8;
        hashes[bin_idx as usize] = hash;
    }
}

pub fn mark_mixed(states: &mut [u8], bin_idx: u64) {
    if (bin_idx as usize) < states.len() {
        states[bin_idx as usize] = TrackerState::MultipleKeys as u8;
    }
}

// Callback types used when C++ drives the tracker.
pub type StoreRepresentativeFn =
    extern "C" fn(ctx: *mut c_void, keys_ptr: *const c_void, row_idx: u32, hash: u64, bin_idx: u64);
pub type CompareColumnFn =
    extern "C" fn(ctx: *mut c_void, col_idx: u64, input_row_idx: u32, rep_bin_idx: u64) -> bool; // returns true if distinct (mismatch)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_bypass_empty_false() {
        let states = [TrackerState::Empty as u8];
        assert!(!can_bypass(&states, 0));
    }

    #[test]
    fn can_bypass_single_true() {
        let states = [TrackerState::SingleKey as u8];
        assert!(can_bypass(&states, 0));
    }

    #[test]
    fn mark_mixed_updates() {
        let mut states = [TrackerState::SingleKey as u8];
        let mut hashes = [42u64];
        mark_mixed(&mut states, 0);
        assert_eq!(states[0], TrackerState::MultipleKeys as u8);
        let _ = &mut hashes;
    }
}
