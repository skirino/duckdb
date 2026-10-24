//! Sort-key vocabulary ported from
//! `src/include/duckdb/common/sorting/sort_key.hpp`.
//!
//! The C++ `enum class SortKeyType` stays in the public header (it cannot move,
//! because `tuple_data_layout.hpp` and the joins include it); Rust mirrors the
//! numeric values as `u8` constants, the way `ahirudb/src/enums/bridge.rs` does
//! for the other DuckDB enums.
//!
//! The `SortKey<T>` structs themselves stay in C++: they are the on-row layout.
//! What lives here is the *properties* table and the decisions derived from it.

#![deny(unsafe_code)]

// ----------------------------------------------------------------------------
// Sort key type codes (must match `SortKeyType : uint8_t` in sort_key.hpp)
// ----------------------------------------------------------------------------
pub const SORT_KEY_TYPE_INVALID: u8 = 0;
pub const NO_PAYLOAD_FIXED_8: u8 = 1;
pub const NO_PAYLOAD_FIXED_16: u8 = 2;
pub const NO_PAYLOAD_FIXED_24: u8 = 3;
pub const NO_PAYLOAD_FIXED_32: u8 = 4;
pub const NO_PAYLOAD_VARIABLE_32: u8 = 5;
pub const PAYLOAD_FIXED_16: u8 = 6;
pub const PAYLOAD_FIXED_24: u8 = 7;
pub const PAYLOAD_FIXED_32: u8 = 8;
pub const PAYLOAD_VARIABLE_32: u8 = 9;

// ----------------------------------------------------------------------------
// Properties
// ----------------------------------------------------------------------------
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SortKeyProperties {
    pub parts: usize,
    pub inline_length: usize,
    pub constant_size: bool,
    pub has_payload: bool,
    pub heap_size_offset: usize,
    pub width: usize,
}

fn table(sort_key_type: u8) -> Option<SortKeyProperties> {
    match sort_key_type {
        NO_PAYLOAD_FIXED_8 => Some(SortKeyProperties {
            parts: 1,
            inline_length: 8,
            constant_size: true,
            has_payload: false,
            heap_size_offset: 0,
            width: 8,
        }),
        NO_PAYLOAD_FIXED_16 => Some(SortKeyProperties {
            parts: 2,
            inline_length: 16,
            constant_size: true,
            has_payload: false,
            heap_size_offset: 0,
            width: 16,
        }),
        NO_PAYLOAD_FIXED_24 => Some(SortKeyProperties {
            parts: 3,
            inline_length: 24,
            constant_size: true,
            has_payload: false,
            heap_size_offset: 0,
            width: 24,
        }),
        NO_PAYLOAD_FIXED_32 => Some(SortKeyProperties {
            parts: 4,
            inline_length: 32,
            constant_size: true,
            has_payload: false,
            heap_size_offset: 0,
            width: 32,
        }),
        NO_PAYLOAD_VARIABLE_32 => Some(SortKeyProperties {
            parts: 2,
            inline_length: 16,
            constant_size: false,
            has_payload: false,
            heap_size_offset: 16,
            width: 32,
        }),
        PAYLOAD_FIXED_16 => Some(SortKeyProperties {
            parts: 1,
            inline_length: 8,
            constant_size: true,
            has_payload: true,
            heap_size_offset: 0,
            width: 16,
        }),
        PAYLOAD_FIXED_24 => Some(SortKeyProperties {
            parts: 2,
            inline_length: 16,
            constant_size: true,
            has_payload: true,
            heap_size_offset: 0,
            width: 24,
        }),
        PAYLOAD_FIXED_32 => Some(SortKeyProperties {
            parts: 3,
            inline_length: 24,
            constant_size: true,
            has_payload: true,
            heap_size_offset: 0,
            width: 32,
        }),
        PAYLOAD_VARIABLE_32 => Some(SortKeyProperties {
            parts: 1,
            inline_length: 8,
            constant_size: false,
            has_payload: true,
            heap_size_offset: 8,
            width: 32,
        }),
        _ => None,
    }
}

pub fn properties(sort_key_type: u8) -> Option<SortKeyProperties> {
    table(sort_key_type)
}

pub fn inline_length(sort_key_type: u8) -> Option<usize> {
    table(sort_key_type).map(|p| p.inline_length)
}

pub fn is_constant_size(sort_key_type: u8) -> Option<bool> {
    table(sort_key_type).map(|p| p.constant_size)
}

pub fn has_payload(sort_key_type: u8) -> Option<bool> {
    table(sort_key_type).map(|p| p.has_payload)
}

// ----------------------------------------------------------------------------
// Derived decisions
// ----------------------------------------------------------------------------

/// `SortedRun::Finalize`: does the sorted data have to be physically reordered?
/// C++: `!SortKeyUtils::IsConstantSize(type) ||
/// SortKeyUtils::HasPayload(type)`.
pub fn needs_reorder(sort_key_type: u8) -> bool {
    match table(sort_key_type) {
        Some(p) => !p.constant_size || p.has_payload,
        None => false,
    }
}

/// `TemplatedSort`: does the sort need a second pass over the keys?
/// C++: `is_index_sort ? false : !CONSTANT_SIZE || INLINE_LENGTH !=
/// sizeof(uint64_t)`.
pub fn requires_next_sort(sort_key_type: u8, is_index_sort: bool) -> bool {
    if is_index_sort {
        return false;
    }
    match table(sort_key_type) {
        Some(p) => !p.constant_size || p.inline_length != 8,
        None => false,
    }
}

/// `TemplatedSort`: how many bytes of the key the radix sort looks at.
/// C++: `MinValue<idx_t>(layout.GetSortWidth(), sizeof(uint64_t))`.
pub fn ska_sort_width(sort_width: usize) -> usize {
    sort_width.min(8)
}

/// Byte swap a 64-bit value if running on a little-endian target
/// (DuckDB's `BSwapIfLE`).
pub fn bswap_if_le(value: u64) -> u64 {
    #[cfg(target_endian = "little")]
    {
        value.swap_bytes()
    }
    #[cfg(not(target_endian = "little"))]
    {
        value
    }
}

// ----------------------------------------------------------------------------
// Tests
// ----------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_nine_types_have_properties() {
        for t in 1u8..=9 {
            assert!(properties(t).is_some(), "type {} missing", t);
        }
        assert!(properties(0).is_none());
        assert!(properties(10).is_none());
    }

    #[test]
    fn needs_reorder_matches_cpp() {
        // constant size, no payload -> false
        assert!(!needs_reorder(NO_PAYLOAD_FIXED_8));
        assert!(!needs_reorder(NO_PAYLOAD_FIXED_32));
        // variable size -> true
        assert!(needs_reorder(NO_PAYLOAD_VARIABLE_32));
        // payload -> true
        assert!(needs_reorder(PAYLOAD_FIXED_16));
        assert!(needs_reorder(PAYLOAD_VARIABLE_32));
    }

    #[test]
    fn requires_next_sort_matches_cpp() {
        // index sort -> false always
        for t in 1u8..=9 {
            assert!(!requires_next_sort(t, true));
        }
        // NO_PAYLOAD_FIXED_8: const, inline 8 -> false
        assert!(!requires_next_sort(NO_PAYLOAD_FIXED_8, false));
        // others with const but inline != 8 -> true
        assert!(requires_next_sort(NO_PAYLOAD_FIXED_16, false));
        // variable -> true
        assert!(requires_next_sort(NO_PAYLOAD_VARIABLE_32, false));
    }

    #[test]
    fn ska_sort_width_caps_at_8() {
        assert_eq!(ska_sort_width(0), 0);
        assert_eq!(ska_sort_width(8), 8);
        assert_eq!(ska_sort_width(100), 8);
    }

    #[test]
    fn bswap_round_trips() {
        let v: u64 = 0x0123_4567_89ab_cdef;
        // applying twice returns original on any endianness
        assert_eq!(bswap_if_le(bswap_if_le(v)), v);
    }
}
