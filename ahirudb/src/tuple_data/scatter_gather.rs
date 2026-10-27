#![deny(unsafe_code)]

//! TupleData scatter/gather kernels

use core::ffi::c_void;

#[repr(C)]
pub struct ScatterGatherFunctionPtr {
    pub ptr: *const c_void,
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn get_scatter_function_ptr(
    _physical_type: u8,
    _within_collection: bool,
) -> *const c_void {
    core::ptr::null()
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn get_gather_function_ptr(_physical_type: u8) -> *const c_void {
    core::ptr::null()
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn get_sort_key_scatter_function_ptr(
    _logical_type: u8,
    _sort_key_type: u8,
) -> *const c_void {
    core::ptr::null()
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn get_sort_key_gather_function_ptr(
    _logical_type: u8,
    _sort_key_type: u8,
) -> *const c_void {
    core::ptr::null()
}
