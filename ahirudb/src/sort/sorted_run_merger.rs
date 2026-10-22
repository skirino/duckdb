//! Merge-path boundary computation ported from
//! `src/common/sort/sorted_run_merger.cpp`
//! (`TemplatedComputePartitionBoundaries`).

#![deny(unsafe_code)]

use std::os::raw::c_void;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunBoundary {
    pub begin: u64,
    pub end: u64,
}

pub type CompareRunKeysFn =
    extern "C" fn(ctx: *mut c_void, run_a: u64, tuple_a: u64, run_b: u64, tuple_b: u64) -> bool;

pub fn compute_partition_boundaries(
    compare: CompareRunKeysFn,
    ctx: *mut c_void,
    run_boundaries: &mut [RunBoundary],
    run_counts: &[u64],
    partition_size: u64,
    partition_idx: u64,
    num_partitions: u64,
) {
    // Last partition: boundaries are end of each sorted run.
    if partition_idx == num_partitions.saturating_sub(1) && num_partitions > 0 {
        for (run_idx, rb) in run_boundaries.iter_mut().enumerate() {
            let c = run_counts[run_idx];
            rb.begin = c;
            rb.end = c;
        }
        return;
    }

    let mut total_remaining: u64 = (partition_idx + 1) * partition_size;

    let mut active: Vec<u64> = Vec::new();
    for (run_idx, rb) in run_boundaries.iter().enumerate() {
        if rb.begin != rb.end {
            active.push(run_idx as u64);
        }
        // reduce total_remaining by what we already have
        total_remaining = total_remaining.saturating_sub(rb.begin);
    }

    while total_remaining != 0 {
        if active.is_empty() {
            break;
        }
        let k = active.len() as u64;

        // find min run
        let mut min_run: u64 = active[0];
        let rb0 = &run_boundaries[min_run as usize];
        let run_remaining0 = rb0.end - rb0.begin;
        let delta0 = total_remaining.div_ceil(k).min(run_remaining0);
        let mut min_delta = delta0;
        let mut min_pos = rb0.begin + min_delta - 1;

        for &ridx in &active[1..] {
            let rb = &run_boundaries[ridx as usize];
            let run_remaining = rb.end - rb.begin;
            let delta = total_remaining.div_ceil(k).min(run_remaining);
            let pos = rb.begin + delta - 1;
            // is this run's value < current min value?
            if compare(ctx, ridx, pos, min_run, min_pos) {
                min_run = ridx;
                min_delta = delta;
                min_pos = pos;
            }
        }

        // increment boundary begin of min run
        let mrb = &mut run_boundaries[min_run as usize];
        mrb.begin += min_delta;

        // erase from active if begin == end
        if mrb.begin == mrb.end {
            active.retain(|&x| x != min_run);
        }

        total_remaining = total_remaining.saturating_sub(min_delta);
    }

    // end = begin for all
    for rb in run_boundaries.iter_mut() {
        rb.end = rb.begin;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A model comparator: treats run 0 keys as 0,1,2,... and run 1 as 0,2,4,...
    // We'll just test boundary partitioning invariants with a simple compare
    // that compares (run, tuple) lexicographically as keys.
    extern "C" fn cmp(ctx: *mut c_void, ra: u64, ta: u64, rb: u64, tb: u64) -> bool {
        let _ = ctx;
        // key = run * 1000 + tuple  (so run 0 < run 1 at same tuple)
        let ka = ra * 1000 + ta;
        let kb = rb * 1000 + tb;
        ka < kb
    }

    #[test]
    fn last_partition() {
        let mut rb = [
            RunBoundary { begin: 0, end: 5 },
            RunBoundary { begin: 0, end: 5 },
        ];
        let counts = [5u64, 5];
        compute_partition_boundaries(cmp, std::ptr::null_mut(), &mut rb, &counts, 4, 2, 3);
        // partition_idx 2 == num_partitions-1 -> begin=end=count
        assert_eq!(rb[0].begin, 5);
        assert_eq!(rb[0].end, 5);
        assert_eq!(rb[1].begin, 5);
    }

    #[test]
    fn boundaries_no_overlap_gap() {
        // 2 runs of 10, partition_size 4 -> 5 partitions
        let counts = [10u64, 10];
        let num_partitions = (20 + 3) / 4; // 5
        // simulate sequential: partition 0 from initial zeros
        let mut prev_begin = [0u64, 0u64];
        for p in 0..num_partitions {
            let mut rb = [
                RunBoundary {
                    begin: prev_begin[0],
                    end: counts[0],
                },
                RunBoundary {
                    begin: prev_begin[1],
                    end: counts[1],
                },
            ];
            // For non-last, end should be count initially? In real flow end
            // from init = min(count, (p+1)*ps). Set end=count for simplicity?
            // Actually algorithm uses end as upper bound per run. Use count.
            compute_partition_boundaries(
                cmp,
                std::ptr::null_mut(),
                &mut rb,
                &counts,
                4,
                p as u64,
                num_partitions as u64,
            );
            // after: end=begin
            // next partition begin = this end
            prev_begin[0] = rb[0].end;
            prev_begin[1] = rb[1].end;
            // invariant: begin <= count
            assert!(rb[0].begin <= counts[0]);
            assert!(rb[1].begin <= counts[1]);
        }
        // final begins should be counts
        assert_eq!(prev_begin[0], counts[0]);
        assert_eq!(prev_begin[1], counts[1]);
    }
}
