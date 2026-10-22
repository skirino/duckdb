//! LEB128 encoding/decoding utilities.
//!
//! Ported from `duckdb/common/serializer/encoding_util.hpp`.
//!
//! These functions encode/decode LEB128 (Little Endian Base 128) integers,
//! which are used for variable-length integer serialization in the binary
//! format.

use crate::serializer::idx_t;

/// Encode an unsigned integer as LEB128, writing to the target buffer.
/// Returns the number of bytes written.
///
/// # Panics
///
/// Panics if the target buffer is too small.
///
/// # Example
///
/// ```
/// use ahirudb::serializer::encoding_util;
/// let mut buffer = [0u8; 16];
/// let n = encoding_util::encode_unsigned_leb128(&mut buffer, 42);
/// assert_eq!(n, 1); // 42 fits in one byte
/// ```
pub fn encode_unsigned_leb128(target: &mut [u8], value: u64) -> idx_t {
    let mut offset = 0usize;
    let mut v = value;
    loop {
        let byte = v as u8 & 0x7f;
        v >>= 7;
        if v != 0 {
            target[offset] = byte | 0x80;
        } else {
            target[offset] = byte;
        }
        offset += 1;
        if v == 0 {
            break;
        }
    }
    offset as idx_t
}

/// Decode an unsigned integer from LEB128 encoding.
/// Returns the number of bytes read and the decoded result.
///
/// # Panics
///
/// Panics if the source data is malformed or truncated.
///
/// # Example
///
/// ```
/// use ahirudb::serializer::encoding_util;
/// let data: &[u8] = &[0x2A]; // 42 in LEB128
/// let mut result = 0u64;
/// let n = encoding_util::decode_unsigned_leb128(data, &mut result);
/// assert_eq!(n, 1);
/// assert_eq!(result, 42);
/// ```
pub fn decode_unsigned_leb128(source: &[u8], result: &mut u64) -> idx_t {
    let mut shift: idx_t = 0;
    let mut value: u64 = 0;
    let mut offset: idx_t = 0;

    loop {
        if offset as usize >= source.len() {
            panic!("Failed to decode LEB128 integer: data may be corrupt (truncated)");
        }
        if shift >= 64 {
            panic!("Failed to decode LEB128 integer: data may be corrupt");
        }
        let byte = source[offset as usize] as u64;
        offset += 1;
        value |= (byte & 0x7f) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            break;
        }
    }

    *result = value;
    offset
}

/// Encode a signed integer as signed LEB128, writing to the target buffer.
/// Returns the number of bytes written.
///
/// # Example
///
/// ```
/// use ahirudb::serializer::encoding_util;
/// let mut buffer = [0u8; 16];
/// let n = encoding_util::encode_signed_leb128(&mut buffer, -1i64);
/// assert_eq!(n, 1); // -1 is encoded as a single byte 0xFF
/// ```
pub fn encode_signed_leb128(target: &mut [u8], value: i64) -> idx_t {
    let mut offset = 0usize;
    let mut v = value;

    loop {
        let byte = v as u8 & 0x7f;
        v >>= 7;

        // Determine whether more bytes are needed
        if (v == 0 && (byte & 0x40) == 0) || (v == -1 && (byte & 0x40) != 0) {
            target[offset] = byte;
            offset += 1;
            break;
        } else {
            target[offset] = byte | 0x80;
            offset += 1;
        }
    }
    offset as idx_t
}

/// Decode a signed integer from LEB128 encoding.
/// Returns the number of bytes read and the decoded result.
///
/// # Panics
///
/// Panics if the source data is malformed or truncated.
///
/// # Example
///
/// ```
/// use ahirudb::serializer::encoding_util;
/// let data: &[u8] = &[0x7F]; // -1 in signed LEB128
/// let mut result = 0i64;
/// let n = encoding_util::decode_signed_leb128(data, &mut result);
/// assert_eq!(n, 1);
/// assert_eq!(result, -1);
/// ```
pub fn decode_signed_leb128(source: &[u8], result: &mut i64) -> idx_t {
    let max_shift: idx_t = (std::mem::size_of::<i64>() * 8) as idx_t;
    let mut shift: idx_t = 0;
    let mut value: u64 = 0;
    let mut offset: idx_t = 0;

    loop {
        if offset as usize >= source.len() {
            panic!("Failed to decode LEB128 integer: data may be corrupt (truncated)");
        }
        if shift >= max_shift {
            panic!("Failed to decode LEB128 integer: data may be corrupt");
        }
        let byte = source[offset as usize] as u64;
        offset += 1;
        value |= (byte & 0x7f) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            break;
        }
    }

    // Sign extend if needed
    if shift < max_shift && (source[offset as usize - 1] & 0x40) != 0 {
        // Sign extend: set all higher bits
        let all_ones: u64 = !0;
        value |= all_ones << shift;
    }

    *result = value as i64;
    offset
}
