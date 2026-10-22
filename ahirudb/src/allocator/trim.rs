//! The trim gate: the rate limit behind `duckdb::Allocator::MallocTrim`.
//!
//! Returning free memory to the OS is expensive, so it is
//! rate-limited: a caller may trim only once per interval, and
//! of the callers that arrive within the interval only the
//! first - the one that wins a single compare-exchange on the
//! tick of the last trim - gets to. The gate holds that state
//! and decides the race; the `malloc_trim` call itself stays in
//! C++, which is what keeps the crate free of a libc
//! dependency.

use std::sync::atomic::{AtomicI64, Ordering};

/// How long the gate holds after a trim, in milliseconds.
const TRIM_INTERVAL_MS: i64 = 100;

/// The state the gate keeps: the tick, in milliseconds, of the
/// last trim. It starts a whole interval in the past, so that
/// the first call is always due, however early it arrives.
struct TrimGate {
    last_trim_tick_ms: AtomicI64,
}

impl TrimGate {
    const fn new() -> Self {
        TrimGate {
            last_trim_tick_ms: AtomicI64::new(-TRIM_INTERVAL_MS),
        }
    }

    /// Whether a trim may happen at `now_ms`.
    ///
    /// A caller that trimmed less than [`TRIM_INTERVAL_MS`] ago
    /// answers `false` without touching the tick, and so does a
    /// caller that loses the race to record its tick: only one
    /// caller per interval wins, and only the winner answers
    /// `true`.
    fn should_trim(&self, now_ms: i64) -> bool {
        let last_trim_tick_ms = self.last_trim_tick_ms.load(Ordering::Relaxed);
        if now_ms - last_trim_tick_ms < TRIM_INTERVAL_MS {
            return false; // We trimmed less than TRIM_INTERVAL_MS ago
        }
        // We successfully updated the tick, we can trim. A
        // failure means another thread updated it since we
        // loaded it, so that thread is the one that trims.
        // `compare_exchange` is the strong variant: it only
        // fails when the value actually changed, which is the
        // C++ `compare_exchange_strong` the gate mirrors.
        self.last_trim_tick_ms
            .compare_exchange(
                last_trim_tick_ms,
                now_ms,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok()
    }
}

/// The gate every `Allocator::MallocTrim` consults. The rate
/// limit is per process, not per allocator, so the gate is a
/// static: one interval shared by every allocator in the
/// process, exactly as the C++'s function-local static was.
static TRIM_GATE: TrimGate = TrimGate::new();

/// Whether the caller may call `malloc_trim` at `now_ms` -
/// milliseconds since some fixed point, as
/// `duckdb::TimePoint::GetTickMs` measures them. The C++
/// caller trims when this answers `true`, and only then.
pub fn should_trim(now_ms: i64) -> bool {
    TRIM_GATE.should_trim(now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_call_trims() {
        let gate = TrimGate::new();
        // The gate starts a whole interval in the past, so the
        // first call is due however early it arrives - even at
        // a negative tick, which is where the C++'s static
        // started.
        assert!(gate.should_trim(0));
    }

    #[test]
    fn a_trim_holds_for_the_interval() {
        let gate = TrimGate::new();
        assert!(gate.should_trim(0));
        assert!(!gate.should_trim(0));
        assert!(!gate.should_trim(TRIM_INTERVAL_MS - 1));
        // The interval is up at exactly TRIM_INTERVAL_MS after
        // the trim that recorded tick 0.
        assert!(gate.should_trim(TRIM_INTERVAL_MS));
        assert!(!gate.should_trim(TRIM_INTERVAL_MS));
    }

    #[test]
    fn only_one_caller_per_interval_wins() {
        // The gate is the module's one piece of lock-free
        // concurrent logic: N concurrent callers of the same
        // tick, exactly one trim.
        const CALLERS: usize = 8;
        const NOW: i64 = 1000;

        let gate = TrimGate::new();
        let winners = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..CALLERS)
                .map(|_| scope.spawn(|| gate.should_trim(NOW)))
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .filter(|won| *won)
                .count()
        });
        assert_eq!(winners, 1);
        // The winner recorded its tick, so the interval holds
        // again.
        assert!(!gate.should_trim(NOW));
    }

    #[test]
    fn a_caller_within_the_interval_of_a_race_does_not_trim() {
        // A caller that arrives while another thread is
        // recording its tick answers `false` either way: its
        // compare-exchange finds the new tick, and the interval
        // from it is not up.
        let gate = TrimGate::new();
        assert!(gate.should_trim(0));
        let callers = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| scope.spawn(|| gate.should_trim(1)))
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .filter(|won| *won)
                .count()
        });
        assert_eq!(callers, 0);
    }
}
