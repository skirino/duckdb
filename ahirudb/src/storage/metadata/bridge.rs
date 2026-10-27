//! The C-ABI surface of the metadata module.
//!
//! Every `pub` function here is an FFI entry point. They forward to the pure
//! kernels in `pointer`, `free_list`, `manager`, `writer` and `reader`, and
//! convert Rust `Result`s into sentinels the C++ shell checks.
//!
//! The few entry points that reach into a buffer C++ owns do so through a raw
//! pointer + length pair, the same shape `serializer/cpp_bridge.rs` uses; the
//! pointer is valid for the duration of the call because C++ holds the lock
//! over the buffer it handed us. The lint is allowed here rather than on each
//! function, the way the serializer module does it.

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use crate::storage::metadata::{
    free_list, manager, pointer,
    pointer::{BlockPointer, MetaBlockPointer},
    reader,
    reader::ReadSegment,
    writer,
    writer::{NextBlockState, WriteSegment},
};

// ---------------------------------------------------------------------------
// Owned list returns
// ---------------------------------------------------------------------------

/// A Rust `Vec<u8>` handed to C++ as a pointer + length. C++ copies the bytes
/// out and calls [`free_block_list`] to release the allocation.
#[repr(C)]
pub struct BlockList {
    pub data: *mut u8,
    pub count: usize,
}

impl BlockList {
    fn from_vec(mut bytes: Vec<u8>) -> Self {
        if bytes.is_empty() {
            return Self {
                data: std::ptr::null_mut(),
                count: 0,
            };
        }
        let count = bytes.len();
        let ptr = bytes.as_mut_ptr();
        std::mem::forget(bytes);
        Self { data: ptr, count }
    }
}

/// # Safety
/// `list` must have come from [`BlockList::from_vec`] and not been freed yet.
pub unsafe fn free_block_list(list: BlockList) {
    if list.data.is_null() {
        return;
    }
    unsafe { drop(Vec::from_raw_parts(list.data, list.count, list.count)) };
}

/// A Rust `Vec<WriteSegment>` handed to C++ as a pointer + length. C++ casts
/// the pointer to its own segment type when iterating.
#[repr(C)]
pub struct WriteSegmentList {
    pub segments: *mut WriteSegment,
    pub count: usize,
}

impl WriteSegmentList {
    fn from_vec(mut segments: Vec<WriteSegment>) -> Self {
        if segments.is_empty() {
            return Self {
                segments: std::ptr::null_mut(),
                count: 0,
            };
        }
        let count = segments.len();
        let ptr = segments.as_mut_ptr();
        std::mem::forget(segments);
        Self {
            segments: ptr,
            count,
        }
    }
}

/// # Safety
/// `list` must have come from [`WriteSegmentList::from_vec`] and not been freed.
pub unsafe fn free_write_segment_list(list: WriteSegmentList) {
    if list.segments.is_null() {
        return;
    }
    unsafe { drop(Vec::from_raw_parts(list.segments, list.count, list.count)) };
}

/// A Rust `Vec<ReadSegment>` handed to C++ as a pointer + length.
#[repr(C)]
pub struct ReadSegmentList {
    pub segments: *mut ReadSegment,
    pub count: usize,
}

impl ReadSegmentList {
    fn from_vec(mut segments: Vec<ReadSegment>) -> Self {
        if segments.is_empty() {
            return Self {
                segments: std::ptr::null_mut(),
                count: 0,
            };
        }
        let count = segments.len();
        let ptr = segments.as_mut_ptr();
        std::mem::forget(segments);
        Self {
            segments: ptr,
            count,
        }
    }
}

/// # Safety
/// `list` must have come from [`ReadSegmentList::from_vec`] and not been freed.
pub unsafe fn free_read_segment_list(list: ReadSegmentList) {
    if list.segments.is_null() {
        return;
    }
    unsafe { drop(Vec::from_raw_parts(list.segments, list.count, list.count)) };
}

// ---------------------------------------------------------------------------
// Manager state
// ---------------------------------------------------------------------------

/// Opaque state for the `MetadataManager` shell. The pure kernels in this
/// module hold no state, but the shell owns this pointer so the
/// `new_…`/`delete_…` pair matches the pattern used by the buffer and
/// allocator modules. It carries one byte so Crubit can bridge the type:
/// zero-sized types are not supported by the generator.
#[repr(C)]
pub struct MetadataManagerState {
    _state: u8,
}

impl MetadataManagerState {
    pub fn new() -> Self {
        Self { _state: 0 }
    }
}

impl Default for MetadataManagerState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn new_metadata_manager() -> *mut MetadataManagerState {
    Box::into_raw(Box::new(MetadataManagerState::new()))
}

/// # Safety
/// `ptr` must be a value from [`new_metadata_manager`] that has not been freed.
pub unsafe fn delete_metadata_manager(ptr: *mut MetadataManagerState) {
    if !ptr.is_null() {
        unsafe { drop(Box::from_raw(ptr)) };
    }
}

// ---------------------------------------------------------------------------
// Pointer forwarding
// ---------------------------------------------------------------------------

pub fn get_disk_pointer(block_index: u64, index: u32, offset: u32) -> MetaBlockPointer {
    pointer::get_disk_pointer(block_index, index, offset)
}

/// Returns `None` when the block index is out of range.
pub fn to_block_pointer(
    meta_pointer: MetaBlockPointer,
    metadata_block_size: u64,
) -> Option<BlockPointer> {
    pointer::to_block_pointer(meta_pointer, metadata_block_size).ok()
}

/// Returns `None` when the offset is out of range.
pub fn from_block_pointer(
    block_pointer: BlockPointer,
    metadata_block_size: u64,
) -> Option<MetaBlockPointer> {
    pointer::from_block_pointer(block_pointer, metadata_block_size).ok()
}

// ---------------------------------------------------------------------------
// Free-list forwarding
// ---------------------------------------------------------------------------

pub fn free_blocks_to_integer(free_blocks: *const u8, count: usize) -> u64 {
    if free_blocks.is_null() || count == 0 {
        return 0;
    }
    let slice = unsafe { std::slice::from_raw_parts(free_blocks, count) };
    free_list::free_blocks_to_integer(slice)
}

pub fn blocks_from_integer(free_list: u64) -> BlockList {
    BlockList::from_vec(free_list::blocks_from_integer(free_list))
}

pub fn free_blocks_from_integer(free_list: u64) -> BlockList {
    BlockList::from_vec(free_list::free_blocks_from_integer(free_list))
}

// ---------------------------------------------------------------------------
// Manager forwarding
// ---------------------------------------------------------------------------

pub fn get_metadata_block_size(block_size: u64) -> u64 {
    manager::get_metadata_block_size(block_size)
}

/// Bitwise OR of the current free list and the modified list. The C++ shell
/// checks whether the result is `u64::MAX` (all blocks free → erase the block)
/// and otherwise calls [`free_blocks_from_integer`] to materialise the list.
pub fn merge_modified_free_blocks(current_free: u64, modified: u64) -> u64 {
    current_free | modified
}

pub fn clear_modified_bit(modified: u64, block_index: u32) -> u64 {
    manager::clear_modified_bit(modified, block_index)
}

pub fn bit_is_cleared(modified: u64, block_index: u32) -> bool {
    manager::bit_is_cleared(modified, block_index)
}

pub fn occupied_blocks(free_list: u64) -> u64 {
    manager::occupied_blocks(free_list)
}

// ---------------------------------------------------------------------------
// Writer forwarding
// ---------------------------------------------------------------------------

pub fn compute_write_data(
    offset: u64,
    capacity: u64,
    write_size: u64,
    metadata_block_size: u64,
) -> WriteSegmentList {
    WriteSegmentList::from_vec(writer::compute_write_data(
        offset,
        capacity,
        write_size,
        metadata_block_size,
    ))
}

pub fn compute_next_block_state(metadata_block_size: u64) -> NextBlockState {
    writer::compute_next_block_state(metadata_block_size)
}

pub fn writer_get_meta_block_pointer(
    block_index: u64,
    index: u32,
    offset: u32,
) -> MetaBlockPointer {
    writer::get_meta_block_pointer(block_index, index, offset)
}

// ---------------------------------------------------------------------------
// Reader forwarding
// ---------------------------------------------------------------------------

pub fn compute_read_data(
    offset: u64,
    capacity: u64,
    read_size: u64,
    metadata_block_size: u64,
    next_offset: u64,
) -> ReadSegmentList {
    ReadSegmentList::from_vec(reader::compute_read_data(
        offset,
        capacity,
        read_size,
        metadata_block_size,
        next_offset,
    ))
}

pub fn reader_get_meta_block_pointer(
    block_index: u64,
    index: u32,
    offset: u32,
) -> MetaBlockPointer {
    reader::get_meta_block_pointer(block_index, index, offset)
}

pub fn compute_read_next_block_state(next_offset: u64, metadata_block_size: u64) -> (u64, u64) {
    reader::compute_read_next_block_state(next_offset, metadata_block_size)
}

pub fn chain_link_is_terminal(next_block: u64) -> bool {
    reader::chain_link_is_terminal(next_block)
}
