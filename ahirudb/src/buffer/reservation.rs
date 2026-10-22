//! Charge arithmetic for `BufferPoolReservation`.

use super::cast;
use super::idx_t;

/// Computes the delta (`new - old`) for a `Resize`, with checked conversions.
pub fn resize_delta(old_size: idx_t, new_size: idx_t) -> Option<i64> {
    let old = cast::checked_i64_from_idx(old_size)?;
    let new = cast::checked_i64_from_idx(new_size)?;
    new.checked_sub(old)
}

/// Computes the value to release on move-assignment of a reservation.
pub fn move_assign_release(old_size: idx_t) -> Option<i64> {
    let v = cast::checked_i64_from_idx(old_size)?;
    v.checked_neg()
}

/// Adds two reservation sizes, returning `None` on overflow.
pub fn merge_size(a: idx_t, b: idx_t) -> Option<idx_t> {
    a.checked_add(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_delta_matches_transition() {
        assert_eq!(resize_delta(10, 25), Some(15));
        assert_eq!(resize_delta(25, 10), Some(-15));
        assert_eq!(resize_delta(5, 5), Some(0));
    }

    #[test]
    fn resize_delta_refuses_bad_conversion() {
        assert_eq!(resize_delta(idx_t::MAX, 0), None);
    }

    #[test]
    fn move_assign_release_negates() {
        assert_eq!(move_assign_release(42), Some(-42));
        assert_eq!(move_assign_release(0), Some(0));
    }

    #[test]
    fn merge_size_adds() {
        assert_eq!(merge_size(3, 4), Some(7));
        assert_eq!(merge_size(idx_t::MAX, 1), None);
    }
}
