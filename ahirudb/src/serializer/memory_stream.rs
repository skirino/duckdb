//! MemoryStream.
//!
//! Ported from `duckdb/common/serializer/memory_stream.hpp` and
//! `memory_stream.cpp`.
//!
//! A concrete implementation of `WriteStream` and `ReadStream` that uses a
//! `Vec<u8>` as the underlying buffer. The buffer grows dynamically as needed.

use crate::serializer::{idx_t, read_stream::ReadStream, write_stream::WriteStream};

/// A concrete implementation of `WriteStream` and `ReadStream` using a buffer.
///
/// This is the Rust equivalent of DuckDB's `MemoryStream` C++ class.
/// It stores data in a `Vec<u8>` that grows dynamically as needed.
pub struct MemoryStream {
    data: Vec<u8>,
    position: idx_t,
}

impl MemoryStream {
    /// Create a new empty MemoryStream.
    pub fn new() -> Self {
        MemoryStream {
            data: Vec::new(),
            position: 0,
        }
    }

    /// Create a new MemoryStream with the given capacity.
    pub fn with_capacity(capacity: idx_t) -> Self {
        MemoryStream {
            data: Vec::with_capacity(capacity as usize),
            position: 0,
        }
    }

    /// Get the underlying data buffer.
    pub fn get_data(&self) -> &[u8] {
        &self.data
    }

    /// Get the current position (number of bytes written/read).
    pub fn get_position(&self) -> idx_t {
        self.position
    }

    /// Get the length of the stored data.
    pub fn len(&self) -> idx_t {
        self.data.len() as idx_t
    }

    /// Check if the stream is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Rewind the position to the beginning.
    pub fn rewind(&mut self) {
        self.position = 0;
    }

    /// Set the position.
    pub fn set_position(&mut self, position: idx_t) {
        self.position = position;
    }

    /// Truncate the data to the current position.
    pub fn truncate(&mut self) {
        self.data.truncate(self.position as usize);
    }
}

impl Default for MemoryStream {
    fn default() -> Self {
        Self::new()
    }
}

impl WriteStream for MemoryStream {
    // The `buffer` must reference `write_size` bytes, per the `WriteStream`
    // contract.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn write_data(&mut self, buffer: *const u8, write_size: idx_t) {
        let end = (self.position + write_size) as usize;
        self.data.resize(end, 0);
        unsafe {
            // SAFETY: buffer points to write_size bytes of initialized data,
            // and self.data has been resized to at least end bytes.
            std::ptr::copy_nonoverlapping(
                buffer,
                self.data.as_mut_ptr().add(self.position as usize),
                write_size as usize,
            );
        }
        self.position += write_size;
    }
}

impl ReadStream for MemoryStream {
    // The `buffer` must have room for `read_size` bytes, per the `ReadStream`
    // contract.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn read_data(&mut self, buffer: *mut u8, read_size: idx_t) {
        if self.position + read_size > self.data.len() as idx_t {
            panic!(
                "Failed to deserialize: not enough data in buffer to fulfill read request \
                 (position={}, need={}, have={})",
                self.position,
                read_size,
                self.data.len()
            );
        }
        let start = self.position as usize;
        unsafe {
            // SAFETY: buffer points to read_size bytes of writable memory,
            // and self.data[self.position..self.position+read_size] is valid.
            std::ptr::copy_nonoverlapping(
                self.data.as_ptr().add(start),
                buffer,
                read_size as usize,
            );
        }
        self.position += read_size;
    }
}

#[cfg(test)]
mod tests {
    use std::mem::MaybeUninit;

    use super::*;

    #[test]
    fn test_memory_stream_write() {
        let mut stream = MemoryStream::new();
        let value: i32 = 42;
        stream.write_data(
            &value as *const i32 as *const u8,
            std::mem::size_of::<i32>() as idx_t,
        );
        assert_eq!(stream.len(), std::mem::size_of::<i32>() as idx_t);
        assert_eq!(stream.get_position(), std::mem::size_of::<i32>() as idx_t);
    }

    #[test]
    fn test_memory_stream_read_write_roundtrip() {
        let mut stream = MemoryStream::new();
        let value: i64 = 12345;
        stream.write_data(
            &value as *const i64 as *const u8,
            std::mem::size_of::<i64>() as idx_t,
        );
        stream.rewind();

        let mut buffer = MaybeUninit::<i64>::uninit();
        stream.read_data(
            buffer.as_mut_ptr() as *mut u8,
            std::mem::size_of::<i64>() as idx_t,
        );
        // SAFETY: read_data filled the whole i64 before this read.
        let read_value = unsafe { buffer.assume_init() };
        assert_eq!(read_value, 12345);
    }

    #[test]
    fn test_memory_stream_rewind() {
        let mut stream = MemoryStream::new();
        let value: i32 = 42;
        stream.write_data(
            &value as *const i32 as *const u8,
            std::mem::size_of::<i32>() as idx_t,
        );
        assert_eq!(stream.get_position(), std::mem::size_of::<i32>() as idx_t);

        stream.rewind();
        assert_eq!(stream.get_position(), 0);

        let mut buffer = MaybeUninit::<i32>::uninit();
        stream.read_data(
            buffer.as_mut_ptr() as *mut u8,
            std::mem::size_of::<i32>() as idx_t,
        );
        // SAFETY: read_data filled the whole i32 before this read.
        let read_value = unsafe { buffer.assume_init() };
        assert_eq!(read_value, 42);
    }

    #[test]
    #[should_panic(expected = "not enough data")]
    fn test_memory_stream_read_overflow() {
        let mut stream = MemoryStream::new();
        let mut buffer = MaybeUninit::<i64>::uninit();
        stream.read_data(buffer.as_mut_ptr() as *mut u8, 8); // Try to read 8 bytes from empty stream
    }
}
