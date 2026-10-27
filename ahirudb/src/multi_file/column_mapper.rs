#![allow(nonstandard_style)]

//! MultiFileColumnMapper decisions: mapping plan construction.

use crate::multi_file::data::idx_t;
use crate::multi_file::ops::get_ops_table;

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ColumnMappingOutcome {
    CONSTANT,
    LOCAL,
    DEFAULT,
    VIRTUAL,
    SKIP_COLUMN,
    ERROR_NOT_FOUND,
}

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum FilterConversion {
    COPY,
    CAST,
    CANNOT,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct ColumnPlanEntry {
    pub outcome: ColumnMappingOutcome,
    pub local_index: idx_t,
    pub filter_conversion: FilterConversion,
    pub trivial_mappable: bool,
    pub needs_cast_map: bool,
    pub child_start: u32,
    pub child_count: u32,
    pub default_kind: u8,
    pub column_map_start: u32,
    pub column_map_count: u32,
    pub expr_kind: u8,
}

/// Trivial T2 kernel: is the local type trivially mappable to the global type?
pub fn is_trivially_mappable(
    local_type: *const core::ffi::c_void,
    global_type: *const core::ffi::c_void,
) -> bool {
    if local_type.is_null() || global_type.is_null() {
        return false;
    }
    let ops = get_ops_table();
    (ops.type_eq)(local_type, global_type)
}

pub fn is_trivially_mappable_with_check(
    local_type: *const core::ffi::c_void,
    global_type: *const core::ffi::c_void,
) -> Result<bool, String> {
    if local_type.is_null() || global_type.is_null() {
        return Err("is_trivially_mappable called with null type".to_string());
    }
    Ok(is_trivially_mappable(local_type, global_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::manual_dangling_ptr)]
    fn trivially_mappable_with_null_pointers() {
        let p = 0x1 as *const core::ffi::c_void;
        assert!(!is_trivially_mappable(core::ptr::null(), core::ptr::null()));
        assert!(!is_trivially_mappable(core::ptr::null(), p));
        assert!(!is_trivially_mappable(p, core::ptr::null()));
    }

    #[test]
    #[allow(clippy::manual_dangling_ptr)]
    fn trivially_mappable_with_check_errors_on_null() {
        let p = 0x1 as *const core::ffi::c_void;
        assert!(is_trivially_mappable_with_check(core::ptr::null(), p).is_err());
        assert!(is_trivially_mappable_with_check(p, core::ptr::null()).is_err());
    }

    #[test]
    #[allow(clippy::manual_dangling_ptr)]
    fn trivially_mappable_same_pointer_with_dummy_ops() {
        let p = 0x1234 as *const core::ffi::c_void;
        // dummy ops returns true when pointers are equal and non-null
        assert!(is_trivially_mappable(p, p));
    }

    #[test]
    #[allow(clippy::manual_dangling_ptr)]
    fn trivially_mappable_different_pointers_with_dummy_ops() {
        let p1 = 0x1234 as *const core::ffi::c_void;
        let p2 = 0x5678 as *const core::ffi::c_void;
        assert!(!is_trivially_mappable(p1, p2));
    }
}
