//! ReadStream trait.
//!
//! Ported from `duckdb/common/serializer/read_stream.hpp`.
//!
//! Trait for reading data from a stream during deserialization.

use crate::serializer::idx_t;

/// A trait for reading data from a stream during deserialization.
///
/// This is the Rust equivalent of DuckDB's `ReadStream` C++ class.
/// It provides a `read_data` method that reads a buffer of bytes from the
/// stream.
///
/// # Binary Format Compatibility
///
/// The Rust ReadStream must read the exact same binary format as the C++
/// version. All integer types are read in their native byte order (usually
/// little-endian on modern systems), and variable-length integers use LEB128
/// encoding.
pub trait ReadStream {
    /// Reads a set amount of data from the stream into the specified buffer
    /// and moves the stream forward accordingly.
    fn read_data(&mut self, buffer: *mut u8, read_size: idx_t);

    /// Read raw bytes into a Vec.
    ///
    /// Reads exactly `len` bytes from the stream.
    fn read_bytes(&mut self, len: idx_t) -> Vec<u8> {
        let mut buffer = vec![0u8; len as usize];
        if len > 0 {
            let ptr = buffer.as_mut_ptr();
            self.read_data(ptr, len);
        }
        buffer
    }

    /// Read a u8 value.
    fn read_u8(&mut self) -> u8 {
        let mut buffer = [0u8; 1];
        self.read_data(buffer.as_mut_ptr(), 1);
        buffer[0]
    }

    /// Read a u16 value in little-endian order.
    fn read_u16(&mut self) -> u16 {
        let mut buffer = [0u8; 2];
        self.read_data(buffer.as_mut_ptr(), 2);
        u16::from_le_bytes(buffer)
    }

    /// Read a u32 value in little-endian order.
    fn read_u32(&mut self) -> u32 {
        let mut buffer = [0u8; 4];
        self.read_data(buffer.as_mut_ptr(), 4);
        u32::from_le_bytes(buffer)
    }

    /// Read a u64 value in little-endian order.
    fn read_u64(&mut self) -> u64 {
        let mut buffer = [0u8; 8];
        self.read_data(buffer.as_mut_ptr(), 8);
        u64::from_le_bytes(buffer)
    }

    /// Read a f32 value in little-endian order.
    fn read_f32(&mut self) -> f32 {
        let mut buffer = [0u8; 4];
        self.read_data(buffer.as_mut_ptr(), 4);
        f32::from_le_bytes(buffer)
    }

    /// Read a f64 value in little-endian order.
    fn read_f64(&mut self) -> f64 {
        let mut buffer = [0u8; 8];
        self.read_data(buffer.as_mut_ptr(), 8);
        f64::from_le_bytes(buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_bytes_direct() {
        struct TestStream {
            data: Vec<u8>,
            pos: idx_t,
        }

        impl ReadStream for TestStream {
            fn read_data(&mut self, buffer: *mut u8, read_size: idx_t) {
                let start = self.pos as usize;
                let end = start + read_size as usize;
                unsafe {
                    // SAFETY: `buffer` points to `read_size` writable bytes
                    // (caller contract),
                    // and `self.data[start..end]` holds exactly `read_size`
                    // bytes.
                    std::slice::from_raw_parts_mut(buffer, read_size as usize)
                        .copy_from_slice(&self.data[start..end]);
                }
                self.pos += read_size;
            }
        }

        let mut stream = TestStream {
            data: vec![1, 2, 3, 4, 5],
            pos: 0,
        };
        let result = stream.read_bytes(3);
        assert_eq!(result, [1, 2, 3]);
        assert_eq!(stream.pos, 3);
    }
}
