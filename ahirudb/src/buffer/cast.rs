//! Checked conversions for the buffer module.

use super::idx_t;

/// Converts an `idx_t` to `i64`, returning `None` when it does not fit.
pub fn checked_i64_from_idx(x: idx_t) -> Option<i64> {
    i64::try_from(x).ok()
}

/// Converts an `i64` to `idx_t`, returning `None` when it is negative.
pub fn checked_idx_from_i64(x: i64) -> Option<idx_t> {
    if x < 0 { None } else { Some(x as idx_t) }
}

/// Adds a signed delta to an unsigned counter, wrapping modulo 2^64 like the
/// original `static_cast<idx_t>(delta)` did. Used by `BlockMemory::ChangeMemoryUsage`.
pub fn add_signed(value: idx_t, delta: i64) -> idx_t {
    value.wrapping_add(delta as idx_t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_i64_round_trips_small_values() {
        assert_eq!(checked_i64_from_idx(0), Some(0));
        assert_eq!(checked_i64_from_idx(42), Some(42));
    }

    #[test]
    fn checked_i64_refuses_out_of_range() {
        assert_eq!(checked_i64_from_idx(idx_t::MAX), None);
    }

    #[test]
    fn checked_idx_refuses_negative() {
        assert_eq!(checked_idx_from_i64(-1), None);
        assert_eq!(checked_idx_from_i64(0), Some(0));
        assert_eq!(checked_idx_from_i64(7), Some(7));
    }

    #[test]
    fn add_signed_wraps_like_the_original_cast() {
        assert_eq!(add_signed(10, -3), 7);
        assert_eq!(add_signed(0, -1), idx_t::MAX);
    }
}
