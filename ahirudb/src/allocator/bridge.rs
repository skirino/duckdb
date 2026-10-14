//! The C-ABI surface of the allocator module: everything C++
//! may call, and nothing else.
//!
//! Every entry point is total: a free the tracker refuses
//! answers `false`, and nothing here fails or panics on a bad
//! argument. The code behind these functions only moves bytes
//! and numbers around, which cannot fail except on allocation
//! failure, which aborts rather than unwinds - so no Rust
//! panic ever has to cross the boundary.
//!
//! The debug tracker itself is reached through the methods
//! Crubit generates on `DebugTracker`; what is in here is the
//! module-level surface: the handle pair that creates and
//! destroys a tracker, the release of a leak report, and the
//! trim gate.

use crate::allocator::{
    debug,
    trim,
};

/// Heap-allocates a debug tracker for a `PrivateAllocatorData`,
/// so that the C++ shell that owns it holds a pointer whose
/// address stays stable for the tracker's whole life.
///
/// `capture_stacks` is the build's choice: only
/// `DUCKDB_DEBUG_ALLOCATION` builds capture stack traces; every
/// other build tracks bytes only.
///
/// # Safety
/// The returned pointer must eventually be passed to
/// [`delete_debug_tracker`], exactly once.
pub fn new_debug_tracker(capture_stacks: bool) -> *mut debug::DebugTracker {
    debug::new_debug_tracker(capture_stacks)
}

/// Deletes a tracker created by [`new_debug_tracker`].
///
/// # Safety
/// `tracker` must come from [`new_debug_tracker`] and must not
/// be used afterwards.
pub unsafe fn delete_debug_tracker(tracker: *mut debug::DebugTracker) {
    unsafe { debug::delete_debug_tracker(tracker) }
}

/// Releases a leak report [`debug::DebugTracker::leak_report`]
/// handed out. C++ copies the report out and calls this
/// exactly once.
pub fn free_leak_report(report: debug::OwnedBytes) {
    // SAFETY: `report` came from `DebugTracker::leak_report`,
    // which built it with `OwnedBytes::from_vec`; this is the
    // one owner of the allocation, so this is where it is
    // freed.
    drop(unsafe { report.into_vec() });
}

/// Whether the caller may call `malloc_trim` at `now_ms`. The
/// interval and the race that decide the answer are Rust's; the
/// trim itself stays in C++.
pub fn should_trim(now_ms: i64) -> bool { trim::should_trim(now_ms) }

#[cfg(test)]
mod tests {
    use super::*;

    /// The bytes of a report Rust owns, read through the
    /// pointer and the length the way C++ reads them.
    fn report_text(report: debug::OwnedBytes) -> String {
        if report.len == 0
        {
            return String::new();
        }
        // SAFETY: `report` came from `leak_report`, which built
        // it with `from_vec`; it is alive until `free_leak_report`
        // takes it back, which is the next statement.
        let bytes = unsafe { std::slice::from_raw_parts(report.ptr, report.len) };
        let text = String::from_utf8_lossy(bytes).into_owned();
        free_leak_report(report);
        text
    }

    #[test]
    fn a_tracker_round_trips_through_the_handle() {
        // SAFETY: the tracker is deleted at the end of the
        // test, exactly once, and not used afterwards.
        let tracker = new_debug_tracker(true);
        unsafe {
            (*tracker).track_allocate(0x1000, 64, b"stack");
            assert_eq!((*tracker).outstanding_bytes(), 64);
            assert!(!(*tracker).is_empty());
            assert!((*tracker).track_free(0x1000, 64));
            assert!((*tracker).is_empty());
            delete_debug_tracker(tracker);
        }
    }

    #[test]
    fn an_untracked_free_is_refused_through_the_handle() {
        // SAFETY: the tracker is deleted at the end of the
        // test, exactly once, and not used afterwards.
        let tracker = new_debug_tracker(true);
        unsafe {
            (*tracker).track_allocate(0x1000, 64, b"stack");
            assert!(!(*tracker).track_free(0x2000, 64));
            assert!(!(*tracker).track_free(0x1000, 32));
            assert_eq!((*tracker).outstanding_bytes(), 64);
            delete_debug_tracker(tracker);
        }
    }

    #[test]
    fn a_reallocate_is_tracked_through_the_handle() {
        // SAFETY: the tracker is deleted at the end of the
        // test, exactly once, and not used afterwards.
        let tracker = new_debug_tracker(true);
        unsafe {
            (*tracker).track_allocate(0x1000, 64, b"old stack");
            assert!((*tracker).track_reallocate(0x1000, 0x2000, 64, 128, b"new stack"));
            assert_eq!((*tracker).outstanding_bytes(), 128);
            assert!((*tracker).track_free(0x2000, 128));
            delete_debug_tracker(tracker);
        }
    }

    #[test]
    fn a_leak_report_crosses_the_boundary_and_back() {
        // SAFETY: the tracker is deleted at the end of the
        // test, exactly once, and not used afterwards.
        let tracker = new_debug_tracker(true);
        unsafe {
            (*tracker).track_allocate(0x1000, 64, b"leaked stack");
            let report = (*tracker).leak_report();
            let text = report_text(report);
            assert!(text.contains("Outstanding allocations found for Allocator\n"));
            assert!(text.contains("Allocation of size 64 at address 0x1000\n"));
            assert!(text.contains("Stack trace:\nleaked stack\n\n"));
            delete_debug_tracker(tracker);
        }
    }

    #[test]
    fn an_empty_leak_report_is_empty_on_both_sides() {
        // SAFETY: the tracker is deleted at the end of the
        // test, exactly once, and not used afterwards.
        let tracker = new_debug_tracker(true);
        unsafe {
            assert_eq!(report_text((*tracker).leak_report()), "");
            delete_debug_tracker(tracker);
        }
    }

    #[test]
    fn a_bytes_only_tracker_round_trips_through_the_handle() {
        // The configuration every build but `DUCKDB_DEBUG_ALLOCATION`
        // runs: no stack traces, so the tracker tracks bytes only.
        // SAFETY: the tracker is deleted at the end of the
        // test, exactly once, and not used afterwards.
        let tracker = new_debug_tracker(false);
        unsafe {
            (*tracker).track_allocate(0x1000, 64, &[]);
            assert!((*tracker).track_free(0x2000, 64)); // no pointer checks
            assert!((*tracker).is_empty());
            delete_debug_tracker(tracker);
        }
    }

    /// The process-wide gate is what the smoke test pins against
    /// a fresh process. What is pinned here is the same interval
    /// from the Rust side: this is the only test that calls the
    /// gate, so its history is predictable - the first call is
    /// due (the gate starts a whole interval in the past), and
    /// the call right after it is not.
    #[test]
    fn the_process_wide_gate_trims_once_per_interval() {
        assert!(should_trim(i64::MAX / 2));
        assert!(!should_trim(i64::MAX / 2));
    }
}
