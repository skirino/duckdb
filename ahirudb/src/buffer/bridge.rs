//! The C-ABI surface of the buffer module.

use crate::buffer::{cast, dead_nodes, memory_usage, reservation};

pub fn new_memory_usage() -> *mut memory_usage::MemoryUsage {
    memory_usage::new_memory_usage()
}

/// # Safety
pub unsafe fn delete_memory_usage(ptr: *mut memory_usage::MemoryUsage) {
    // SAFETY: forwarded to the owning module.
    unsafe { memory_usage::delete_memory_usage(ptr) }
}

pub fn checked_i64_from_idx(x: u64) -> Option<i64> {
    cast::checked_i64_from_idx(x)
}

pub fn checked_idx_from_i64(x: i64) -> Option<u64> {
    cast::checked_idx_from_i64(x)
}

pub fn add_signed(value: u64, delta: i64) -> u64 {
    cast::add_signed(value, delta)
}

pub fn resize_delta(old_size: u64, new_size: u64) -> Option<i64> {
    reservation::resize_delta(old_size, new_size)
}

pub fn move_assign_release(old_size: u64) -> Option<i64> {
    reservation::move_assign_release(old_size)
}

pub fn merge_size(a: u64, b: u64) -> Option<u64> {
    reservation::merge_size(a, b)
}

// block_copy functions are called directly as ahirudb::buffer::block_copy::...

pub fn new_dead_counter() -> *mut dead_nodes::DeadNodeCounter {
    dead_nodes::new_dead_counter()
}

/// # Safety
pub unsafe fn delete_dead_counter(ptr: *mut dead_nodes::DeadNodeCounter) {
    // SAFETY: forwarded to the owning module.
    unsafe { dead_nodes::delete_dead_counter(ptr) }
}

#[cfg(test)]
mod tests {
    // smoke: nothing beyond submodule tests
}
