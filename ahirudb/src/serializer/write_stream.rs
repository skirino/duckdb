//! WriteStream trait.
//!
//! Ported from `duckdb/common/serializer/write_stream.hpp`.
//!
//! Trait for writing data to a stream during serialization.

use crate::serializer::idx_t;

/// A trait for writing data to a stream during serialization.
///
/// This is the Rust equivalent of DuckDB's `WriteStream` C++ class.
/// It provides a `write_data` method that writes a buffer of bytes to the
/// stream.
///
/// # Binary Format Compatibility
///
/// The Rust WriteStream must produce the exact same binary format as the C++
/// version. All integer types are written in their native byte order (usually
/// little-endian on modern systems), and variable-length integers use LEB128
/// encoding.
pub trait WriteStream {
    /// Writes a set amount of data from the specified buffer into the stream
    /// and moves the stream forward accordingly.
    fn write_data(&mut self, buffer: *const u8, write_size: idx_t);

    /// Write raw bytes from a slice.
    ///
    /// This is equivalent to `write_data` but takes a `&[u8]` slice for safety.
    fn write_bytes(&mut self, data: &[u8]) {
        let len = data.len() as idx_t;
        if len > 0 {
            let ptr = data.as_ptr();
            self.write_data(ptr, len);
        }
    }

    /// Write a u8 value.
    fn write_u8(&mut self, value: u8) {
        self.write_data(&value as *const u8, 1);
    }

    /// Write a u16 value in little-endian order.
    fn write_u16(&mut self, value: u16) {
        self.write_bytes(&value.to_le_bytes());
    }

    /// Write a u32 value in little-endian order.
    fn write_u32(&mut self, value: u32) {
        self.write_bytes(&value.to_le_bytes());
    }

    /// Write a u64 value in little-endian order.
    fn write_u64(&mut self, value: u64) {
        self.write_bytes(&value.to_le_bytes());
    }

    /// Write a f32 value in little-endian order.
    fn write_f32(&mut self, value: f32) {
        self.write_bytes(&value.to_le_bytes());
    }

    /// Write a f64 value in little-endian order.
    fn write_f64(&mut self, value: f64) {
        self.write_bytes(&value.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_bytes_direct() {
        struct TestStream {
            data: Vec<u8>,
        }

        impl WriteStream for TestStream {
            fn write_data(&mut self, buffer: *const u8, write_size: idx_t) {
                // SAFETY: buffer points to write_size bytes of valid data,
                // and self.data has enough capacity.
                let slice = unsafe { std::slice::from_raw_parts(buffer, write_size as usize) };
                self.data.extend_from_slice(slice);
            }
        }

        let mut stream = TestStream { data: Vec::new() };
        stream.write_bytes(&[1, 2, 3, 4]);
        assert_eq!(stream.data, [1, 2, 3, 4]);
    }
}
