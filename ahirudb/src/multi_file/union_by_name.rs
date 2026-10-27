#![allow(nonstandard_style)]

//! Union-by-name merge decisions.

use crate::multi_file::ops::get_ops_table;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct UnionMergeResult {
    pub needs_cast: bool,
    pub can_merge: bool,
}

/// Decide if two types can be merged in union-by-name
pub fn can_merge_union_types(
    type1: *const core::ffi::c_void,
    type2: *const core::ffi::c_void,
) -> UnionMergeResult {
    if type1.is_null() || type2.is_null() {
        return UnionMergeResult {
            needs_cast: false,
            can_merge: false,
        };
    }
    let ops = get_ops_table();
    if (ops.type_eq)(type1, type2) {
        return UnionMergeResult {
            needs_cast: false,
            can_merge: true,
        };
    }
    let _ = (ops.force_max_type)(type1, type2);
    UnionMergeResult {
        needs_cast: true,
        can_merge: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::manual_dangling_ptr)]
    fn can_merge_with_null_types() {
        let p = 0x1 as *const core::ffi::c_void;
        let res = can_merge_union_types(core::ptr::null(), p);
        assert!(!res.can_merge);
        assert!(!res.needs_cast);
        let res2 = can_merge_union_types(p, core::ptr::null());
        assert!(!res2.can_merge);
    }

    #[test]
    #[allow(clippy::manual_dangling_ptr)]
    fn can_merge_same_pointer_with_dummy_ops() {
        let p = 0x1234 as *const core::ffi::c_void;
        let res = can_merge_union_types(p, p);
        assert!(res.can_merge);
        assert!(!res.needs_cast);
    }

    #[test]
    #[allow(clippy::manual_dangling_ptr)]
    fn can_merge_different_pointers_with_dummy_ops() {
        let p1 = 0x1234 as *const core::ffi::c_void;
        let p2 = 0x5678 as *const core::ffi::c_void;
        let res = can_merge_union_types(p1, p2);
        assert!(res.can_merge);
        assert!(res.needs_cast);
    }
}
