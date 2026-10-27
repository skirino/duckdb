#![allow(nonstandard_style)]

//! The function table that lets Rust kernels call back into C++ for facts
//! only the C++ side has (`LogicalType`, `Value`, filter evaluation, etc.).

use std::sync::OnceLock;

use crate::multi_file::data::idx_t;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct MultiFileOps {
    pub type_id: extern "C" fn(*const core::ffi::c_void) -> u8,
    pub type_eq: extern "C" fn(*const core::ffi::c_void, *const core::ffi::c_void) -> bool,
    pub force_max_type: extern "C" fn(
        *const core::ffi::c_void,
        *const core::ffi::c_void,
    ) -> *const core::ffi::c_void,
    pub value_try_cast: extern "C" fn(*const core::ffi::c_void, u8) -> bool,
    pub value_is_null: extern "C" fn(*const core::ffi::c_void) -> bool,
    pub evaluate_filter_against_constant:
        extern "C" fn(*const core::ffi::c_void, *const core::ffi::c_void) -> bool,
    pub expr_constant:
        extern "C" fn(*const core::ffi::c_void, *mut *const core::ffi::c_void) -> bool,
    pub virtual_column_binding:
        extern "C" fn(*const core::ffi::c_void, idx_t, *const core::ffi::c_void) -> u8,
}

static MULTI_FILE_OPS: OnceLock<MultiFileOps> = OnceLock::new();

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn duckdb_multi_file_set_ops(ops: MultiFileOps) {
    let _ = MULTI_FILE_OPS.set(ops);
}

pub fn get_ops_table() -> &'static MultiFileOps {
    MULTI_FILE_OPS.get_or_init(|| MultiFileOps {
        type_id: dummy_type_id,
        type_eq: dummy_type_eq,
        force_max_type: dummy_force_max_type,
        value_try_cast: dummy_value_try_cast,
        value_is_null: dummy_value_is_null,
        evaluate_filter_against_constant: dummy_eval,
        expr_constant: dummy_expr_constant,
        virtual_column_binding: dummy_vcb,
    })
}

extern "C" fn dummy_type_id(_ty: *const core::ffi::c_void) -> u8 {
    0
}
extern "C" fn dummy_type_eq(a: *const core::ffi::c_void, b: *const core::ffi::c_void) -> bool {
    a == b && !a.is_null()
}
extern "C" fn dummy_force_max_type(
    a: *const core::ffi::c_void,
    _b: *const core::ffi::c_void,
) -> *const core::ffi::c_void {
    a
}
extern "C" fn dummy_value_try_cast(_val: *const core::ffi::c_void, _target: u8) -> bool {
    false
}
extern "C" fn dummy_value_is_null(_val: *const core::ffi::c_void) -> bool {
    false
}
extern "C" fn dummy_eval(
    _filter: *const core::ffi::c_void,
    _val: *const core::ffi::c_void,
) -> bool {
    true
}
#[allow(unsafe_code)]
extern "C" fn dummy_expr_constant(
    _expr: *const core::ffi::c_void,
    out: *mut *const core::ffi::c_void,
) -> bool {
    if !out.is_null() {
        // SAFETY: C passes a valid out pointer if non-null for writing the result handle
        unsafe {
            *out = core::ptr::null();
        }
    }
    false
}
extern "C" fn dummy_vcb(
    _reader: *const core::ffi::c_void,
    _column_id: idx_t,
    _type: *const core::ffi::c_void,
) -> u8 {
    0
}

#[repr(C)]
pub struct MultiFileCallScope {
    _index: u64,
    _previous: u64,
}
