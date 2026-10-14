//! The debug tracker: the state behind `duckdb::AllocatorDebugInfo`.
//!
//! A tracker is created once, in a DEBUG build, for every
//! `PrivateAllocatorData` the `Allocator` constructor has to
//! give one. It records every allocation through the allocator
//! that owns it, and answers four questions for the shell
//! around it: how many bytes are outstanding, whether a free
//! matches the allocation it claims to end, where the
//! outstanding bytes were allocated from, and - when the build
//! asked for stack traces - which pointers are still
//! outstanding.
//!
//! The invariants the C++ asserted only under `D_ASSERT` are
//! total functions here: freeing a pointer the tracker never
//! saw, or with a size that does not match the allocation,
//! answers `false` and changes nothing, rather than silently
//! corrupting the counter. Nothing here can panic on a bad
//! argument; the only way a tracker operation aborts is an
//! allocation failure inside the map, which aborts rather than
//! unwinds, so nothing unwinds across the boundary.

use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{
            AtomicUsize,
            Ordering,
        },
    },
};

/// The state `duckdb::AllocatorDebugInfo` keeps: the number of
/// bytes that are outstanding (allocated and not freed), and -
/// when stack traces are being captured - the size and stack
/// trace of every outstanding pointer.
///
/// One tracker per `PrivateAllocatorData`, created in the
/// constructor of the C++ shell and destroyed exactly once by
/// its destructor: it is never copied and never lent out, so
/// the address the shell holds stays stable for the tracker's
/// whole life.
///
/// The tracker is `Send + Sync` by construction - an atomic
/// counter and a mutex-protected map, nothing else - because
/// the allocator it belongs to is used on whatever thread
/// allocates, which is not necessarily the thread that
/// constructed it.
pub struct DebugTracker {
    /// The number of bytes that have been allocated and not freed.
    outstanding: AtomicUsize,
    /// Whether the pointer map is maintained. Only
    /// `DUCKDB_DEBUG_ALLOCATION` builds capture stack traces;
    /// every other build tracks bytes only, and never touches
    /// the map, which keeps it as cheap as the C++ was.
    capture_stacks: bool,
    /// The outstanding pointers, with their size and stack
    /// trace. Read and written only when `capture_stacks` is
    /// set.
    pointers: Mutex<HashMap<u64, TrackedAllocation>>,
}

/// One outstanding allocation: the size it was allocated with,
/// and the stack trace C++ captured at the allocation.
struct TrackedAllocation {
    size: u64,
    stack: Vec<u8>,
}

impl DebugTracker {
    /// Creates a tracker. `capture_stacks` is the build's
    /// choice: only `DUCKDB_DEBUG_ALLOCATION` builds pass
    /// `true`, the same compile-time decision the C++ made
    /// with its `#ifdef`.
    pub(crate) fn new(capture_stacks: bool) -> Self {
        DebugTracker {
            outstanding: AtomicUsize::new(0),
            capture_stacks,
            pointers: Mutex::new(HashMap::new()),
        }
    }

    /// Records an allocation of `size` bytes at `pointer`,
    /// with the stack trace captured for it. The trace is
    /// recorded only by a tracker that captures stacks; every
    /// other build passes an empty trace and the tracker
    /// ignores whatever it is handed.
    pub fn track_allocate(&self, pointer: u64, size: u64, stack: &[u8]) {
        self.outstanding.fetch_add(size as usize, Ordering::SeqCst);
        if self.capture_stacks
        {
            let mut pointers = self.pointers.lock().unwrap();
            pointers.insert(pointer, TrackedAllocation { size, stack: stack.to_vec() });
        }
    }

    /// Ends the allocation of `size` bytes at `pointer`.
    ///
    /// Answers `false` - changing nothing - when the pointer
    /// was never tracked, when it was tracked with a different
    /// size, or when ending it would underflow the counter.
    /// The C++ caller asserts the answer, which is what the
    /// C++ `D_ASSERT`s on the counter and the map did; a free
    /// that answers `false` is a bug in the caller, and a
    /// debug build stops on it.
    pub fn track_free(&self, pointer: u64, size: u64) -> bool {
        if self.capture_stacks
        {
            // The free has to end an allocation of exactly this
            // size, at a pointer the tracker knows. Checked
            // before anything is changed, so that a refused
            // free leaves the tracker exactly as it was.
            let mut pointers = self.pointers.lock().unwrap();
            match pointers.get(&pointer)
            {
                Some(alloc) if alloc.size == size =>
                {
                    pointers.remove(&pointer);
                },
                _ => return false,
            }
        }
        // The decrement refuses to underflow the counter, which
        // is what `D_ASSERT(allocation_count >= size)` asserted
        // in C++. The compare-exchange makes the check and the
        // decrement one step, so two concurrent frees cannot
        // both pass it and wrap the counter.
        let mut current = self.outstanding.load(Ordering::SeqCst);
        loop
        {
            if current < size as usize
            {
                return false;
            }
            match self.outstanding.compare_exchange_weak(
                current,
                current - size as usize,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
    }

    /// Moves the tracking of an allocation from `pointer` to
    /// `new_pointer`: ends `old_size` bytes and records
    /// `new_size` new ones, with the stack trace C++ captured
    /// for the reallocation.
    ///
    /// Answers `false`, changing nothing, when the free half
    /// does not end a tracked allocation - the same conditions
    /// [`track_free`] refuses.
    pub fn track_reallocate(
        &self,
        pointer: u64,
        new_pointer: u64,
        old_size: u64,
        new_size: u64,
        stack: &[u8],
    ) -> bool {
        if !self.track_free(pointer, old_size)
        {
            return false;
        }
        self.track_allocate(new_pointer, new_size, stack);
        true
    }

    /// The number of bytes that are outstanding: allocated and
    /// not freed.
    pub fn outstanding_bytes(&self) -> u64 { self.outstanding.load(Ordering::SeqCst) as u64 }

    /// Whether every tracked allocation has been freed again.
    pub fn is_empty(&self) -> bool { self.outstanding_bytes() == 0 }

    /// The leak report the C++ destructor prints: one block per
    /// outstanding pointer, with its size, its address and the
    /// stack trace captured at its allocation. Empty when
    /// nothing is outstanding, which is also when the C++
    /// prints nothing.
    ///
    /// The address is spelled `0x` and its hex value, which is
    /// what `%p` spelled in the C++. The report is a DEBUG-only
    /// diagnostic printed to stdout; no test pins its bytes.
    pub fn leak_report(&self) -> OwnedBytes {
        let mut report = String::new();
        if self.outstanding_bytes() != 0
        {
            report.push_str("Outstanding allocations found for Allocator\n");
            if self.capture_stacks
            {
                let pointers = self.pointers.lock().unwrap();
                for (pointer, alloc) in pointers.iter()
                {
                    report.push_str(&format!(
                        "Allocation of size {} at address 0x{:x}\n",
                        alloc.size, pointer
                    ));
                    report.push_str("Stack trace:\n");
                    report.push_str(&String::from_utf8_lossy(&alloc.stack));
                    report.push_str("\n\n");
                }
            }
        }
        OwnedBytes::from_vec(report.into_bytes())
    }
}

/// A run of bytes Rust owns: the leak report, which C++ copies
/// out and hands back to [`crate::allocator::bridge::free_leak_report`]
/// exactly once.
#[repr(C)]
pub struct OwnedBytes {
    pub ptr: *mut u8,
    pub len: usize,
}

impl OwnedBytes {
    /// Copies `value` into a fresh allocation.
    ///
    /// `into_boxed_slice` shrinks the allocation to exactly the
    /// length, which is what lets `into_vec` hand the same
    /// shape back to the allocator.
    pub(crate) fn from_vec(value: Vec<u8>) -> Self {
        let value = value.into_boxed_slice();
        let len = value.len();
        Self { ptr: Box::into_raw(value) as *mut u8, len }
    }

    /// Gives the bytes back. Paired with [`Self::from_vec`].
    ///
    /// # Safety
    /// `self` must be a value from [`Self::from_vec`] whose
    /// allocation is still alive.
    pub(crate) unsafe fn into_vec(self) -> Vec<u8> {
        // SAFETY: `from_vec` handed over a `Box<[u8]>` of this
        // length, and `Vec::from` takes that box back without
        // reallocating, so the pointer and the layout the
        // allocator is asked to free are the ones it handed out.
        unsafe { Vec::from(Box::from_raw(std::ptr::slice_from_raw_parts_mut(self.ptr, self.len))) }
    }
}

/// Heap-allocates a tracker, so that the C++ shell that owns a
/// `PrivateAllocatorData` can hold it through a pointer that
/// stays stable for the tracker's whole life.
///
/// # Safety
/// The returned pointer must eventually be passed to
/// [`delete_debug_tracker`], exactly once.
pub fn new_debug_tracker(capture_stacks: bool) -> *mut DebugTracker {
    Box::into_raw(Box::new(DebugTracker::new(capture_stacks)))
}

/// Deletes a tracker created by [`new_debug_tracker`].
///
/// # Safety
/// `tracker` must come from [`new_debug_tracker`] and must not
/// be used afterwards.
pub unsafe fn delete_debug_tracker(tracker: *mut DebugTracker) {
    // SAFETY: the contract above - the pointer came from
    // `Box::into_raw` and is destroyed exactly once, here.
    unsafe { drop(Box::from_raw(tracker)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tracker that captures stack traces, as a
    /// `DUCKDB_DEBUG_ALLOCATION` build has.
    fn capturing() -> DebugTracker { DebugTracker::new(true) }

    /// A tracker that tracks bytes only, as every other DEBUG
    /// build has.
    fn counting() -> DebugTracker { DebugTracker::new(false) }

    /// The bytes of a report Rust owns, read the way C++ reads
    /// them: through the pointer and the length, not through
    /// the value.
    fn report_text(report: OwnedBytes) -> String {
        if report.len == 0
        {
            return String::new();
        }
        // SAFETY: `report` came from `leak_report`, which built
        // it with `from_vec`; it is alive until `into_vec`
        // takes it back, which is the next statement.
        let bytes = unsafe { std::slice::from_raw_parts(report.ptr, report.len) };
        let text = String::from_utf8_lossy(bytes).into_owned();
        drop(unsafe { report.into_vec() });
        text
    }

    #[test]
    fn an_allocation_round_trips_with_exact_accounting() {
        // test_allocator_debug_info.cpp: allocate, free and
        // reallocate all work, and the accounting is exact.
        let tracker = capturing();
        assert!(tracker.is_empty());
        tracker.track_allocate(0x1000, 64, b"stack");
        assert_eq!(tracker.outstanding_bytes(), 64);
        assert!(!tracker.is_empty());

        assert!(tracker.track_free(0x1000, 64));
        assert_eq!(tracker.outstanding_bytes(), 0);
        assert!(tracker.is_empty());
    }

    #[test]
    fn a_reallocate_moves_the_tracking_to_the_new_pointer() {
        let tracker = capturing();
        tracker.track_allocate(0x1000, 64, b"old stack");
        assert!(tracker.track_reallocate(0x1000, 0x2000, 64, 128, b"new stack"));

        // The old pointer is gone: ending it again is refused.
        assert!(!tracker.track_free(0x1000, 64));
        // The new pointer is tracked with the new size and the
        // trace captured at the reallocation.
        assert_eq!(tracker.outstanding_bytes(), 128);
        assert!(tracker.track_free(0x2000, 128));
        assert!(tracker.is_empty());

        let report = report_text(tracker.leak_report());
        assert!(report.is_empty());
    }

    #[test]
    fn a_leak_report_lists_each_outstanding_pointer() {
        let tracker = capturing();
        tracker.track_allocate(0x1000, 64, b"first stack");
        tracker.track_allocate(0x2000, 128, b"second stack");

        let report = report_text(tracker.leak_report());
        assert!(report.contains("Outstanding allocations found for Allocator\n"));
        assert!(report.contains("Allocation of size 64 at address 0x1000\n"));
        assert!(report.contains("Allocation of size 128 at address 0x2000\n"));
        assert!(report.contains("Stack trace:\nfirst stack\n\n"));
        assert!(report.contains("Stack trace:\nsecond stack\n\n"));
    }

    #[test]
    fn a_tracker_without_stacks_reports_sizes_only() {
        // A build that does not capture stack traces still
        // reports that bytes are outstanding, the way the C++
        // printed the header line and nothing else.
        let tracker = counting();
        tracker.track_allocate(0x1000, 64, &[]);
        let report = report_text(tracker.leak_report());
        assert_eq!(report, "Outstanding allocations found for Allocator\n");
        assert!(tracker.track_free(0x1000, 64));
    }

    #[test]
    fn an_empty_tracker_has_an_empty_report() {
        let tracker = capturing();
        assert!(report_text(tracker.leak_report()).is_empty());
    }

    #[test]
    fn a_free_the_tracker_never_saw_is_refused() {
        // The "graceful" property, at the scale of the moved
        // logic: a free of a pointer the tracker never saw, or
        // with the wrong size, answers `false` instead of
        // corrupting the accounting.
        let tracker = capturing();
        tracker.track_allocate(0x1000, 64, b"stack");

        assert!(!tracker.track_free(0x2000, 64)); // never tracked
        assert!(!tracker.track_free(0x1000, 32)); // wrong size
        assert_eq!(tracker.outstanding_bytes(), 64);

        // Nothing was corrupted: the real free still works.
        assert!(tracker.track_free(0x1000, 64));
        assert!(tracker.is_empty());
    }

    #[test]
    fn a_free_that_would_underflow_the_counter_is_refused() {
        let tracker = counting();
        tracker.track_allocate(0x1000, 64, &[]);
        // More bytes freed than were allocated: the counter
        // reports the mismatch rather than wrapping.
        assert!(!tracker.track_free(0x1000, 65));
        assert_eq!(tracker.outstanding_bytes(), 64);
        assert!(tracker.track_free(0x1000, 64));
        assert!(!tracker.track_free(0x1000, 1));
        assert_eq!(tracker.outstanding_bytes(), 0);
    }

    #[test]
    fn a_counter_tracker_does_not_check_pointers() {
        // Every DEBUG build that is not `DUCKDB_DEBUG_ALLOCATION`
        // has no pointer map: the C++ checked nothing but the
        // counter, so a free of an unknown pointer is not an
        // error - only the bytes count.
        let tracker = counting();
        tracker.track_allocate(0x1000, 64, &[]);
        assert!(tracker.track_free(0x2000, 64)); // any pointer ends the bytes
        assert!(tracker.is_empty());
    }

    #[test]
    fn a_moved_tracker_keeps_its_state() {
        // test_column_data_allocator.cpp: a copied handle
        // preserves the managed state. Rust moves are by
        // value: the moved-to tracker keeps the state, the
        // source is inert, and there is no aliasing through
        // which the accounting could be freed twice.
        let tracker = capturing();
        tracker.track_allocate(0x1000, 64, b"stack");
        let moved = tracker;

        assert_eq!(moved.outstanding_bytes(), 64);
        assert!(!moved.is_empty());
        assert!(moved.track_free(0x1000, 64));
        assert!(moved.is_empty());
    }

    #[test]
    fn two_trackers_account_independently() {
        let first = capturing();
        let second = capturing();
        first.track_allocate(0x1000, 64, b"first");
        second.track_allocate(0x1000, 128, b"second"); // the same pointer value

        assert_eq!(first.outstanding_bytes(), 64);
        assert_eq!(second.outstanding_bytes(), 128);

        // Ending the allocation through one tracker does not
        // end the other's.
        assert!(first.track_free(0x1000, 64));
        assert!(first.is_empty());
        assert!(!second.is_empty());
        assert!(!second.track_free(0x1000, 64)); // the wrong size for second
        assert!(second.track_free(0x1000, 128));
        assert!(second.is_empty());
    }

    /// The tracker is used on whatever thread allocates, which
    /// is not necessarily the thread that constructed it: the
    /// C++ `BlockAllocator` test pins state used on a different
    /// thread than it was constructed on, and destroyed on the
    /// home thread. The tracker is `Send + Sync` by
    /// construction, so the same property holds here.
    #[test]
    fn the_tracker_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DebugTracker>();
    }

    #[test]
    fn concurrent_allocations_and_frees_account_exactly() {
        // N threads allocate and free concurrently on one
        // tracker, through the stack-map path (traces passed,
        // as `DUCKDB_DEBUG_ALLOCATION` builds do). The
        // outstanding bytes are exactly what is still
        // outstanding, and the tracker ends up empty.
        const THREADS: usize = 4;
        const ALLOCATIONS: u64 = 256;
        const SIZE: u64 = 64;

        let tracker = capturing();
        let tracker_ref = &tracker;
        let pointers: Vec<Vec<u64>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..THREADS)
                .map(|thread| {
                    scope.spawn(move || {
                        let mut pointers = Vec::with_capacity(ALLOCATIONS as usize);
                        for allocation in 0..ALLOCATIONS
                        {
                            let pointer = 0x1000 + thread as u64 * ALLOCATIONS + allocation;
                            tracker_ref.track_allocate(pointer, SIZE, b"concurrent stack");
                            pointers.push(pointer);
                        }
                        pointers
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect()
        });

        assert_eq!(tracker.outstanding_bytes(), THREADS as u64 * ALLOCATIONS * SIZE);

        std::thread::scope(|scope| {
            for thread_pointers in &pointers
            {
                scope.spawn(move || {
                    for &pointer in thread_pointers
                    {
                        assert!(tracker_ref.track_free(pointer, SIZE));
                    }
                });
            }
        });
        assert_eq!(tracker.outstanding_bytes(), 0);
        assert!(tracker.is_empty());
    }

    /// The essence of test_custom_allocator.cpp: a custom
    /// backend keeps its own counter (`MyAllocateData::memory_counter`),
    /// moving up on allocate, down on free, and by the delta
    /// on reallocate. The tracker's outstanding bytes equal the
    /// simulated counter after every step of a mixed sequence.
    #[test]
    fn a_custom_backends_counter_matches_the_tracker() {
        struct CustomBackend {
            tracker: DebugTracker,
            counter: u64,
        }

        impl CustomBackend {
            fn allocate(&mut self, pointer: u64, size: u64) {
                self.counter += size;
                self.tracker
                    .track_allocate(pointer, size, b"custom backend");
                assert_eq!(self.tracker.outstanding_bytes(), self.counter);
            }

            fn free(&mut self, pointer: u64, size: u64) {
                self.counter -= size;
                assert!(self.tracker.track_free(pointer, size));
                assert_eq!(self.tracker.outstanding_bytes(), self.counter);
            }

            fn reallocate(&mut self, pointer: u64, new_pointer: u64, old_size: u64, new_size: u64) {
                self.counter -= old_size;
                self.counter += new_size;
                assert!(self.tracker.track_reallocate(
                    pointer,
                    new_pointer,
                    old_size,
                    new_size,
                    b"custom backend"
                ));
                assert_eq!(self.tracker.outstanding_bytes(), self.counter);
            }
        }

        let mut backend = CustomBackend { tracker: DebugTracker::new(true), counter: 0 };
        assert_eq!(backend.counter, 0);

        backend.allocate(0x1000, 4096);
        backend.allocate(0x2000, 512);
        backend.reallocate(0x1000, 0x3000, 4096, 8192);
        backend.free(0x2000, 512);
        backend.allocate(0x4000, 1024);
        backend.free(0x3000, 8192);
        backend.free(0x4000, 1024);

        assert_eq!(backend.counter, 0);
        assert!(backend.tracker.is_empty());
    }
}
