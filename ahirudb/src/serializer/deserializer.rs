//! Deserializer trait and BinaryDeserializer implementation.
//!
//! Ported from `duckdb/common/serializer/deserializer.hpp` and
//! `duckdb/common/serializer/binary_deserializer.hpp`.
//!
//! The `Deserializer` trait provides the abstract interface for
//! deserialization, while `BinaryDeserializer` is a concrete implementation
//! that reads from a `ReadStream`.

use std::fmt;

use crate::serializer::{
    MESSAGE_TERMINATOR_FIELD_ID,
    encoding_util::{decode_signed_leb128, decode_unsigned_leb128},
    field_id_t, hugeint_t, idx_t,
    read_stream::ReadStream,
    serialization_data::SerializationData,
    string, string_t, uhugeint_t,
};

/// Rust trait equivalent of the C++ `Deserializer` class.
///
/// This trait provides the interface for deserializing values during
/// DuckDB's serialization format. Implementations must read the
/// exact same binary format as the C++ version.
///
/// # Binary Format
///
/// The format uses:
/// - Fixed-size types read in native byte order (little-endian on modern
///   systems)
/// - Variable-length integers in LEB128 decoding
/// - Strings: varint length + bytes
/// - Lists: varint count + elements
/// - Objects: begin/end markers with properties
pub trait Deserializer: fmt::Debug {
    /// Get the deserialization data stack.
    fn get_serialization_data(&self) -> &SerializationData;

    /// Set the deserialization data.
    fn set_serialization_data(&mut self, data: &SerializationData);

    /// Read a bool value.
    fn read_bool(&mut self) -> bool;

    /// Read a char value.
    fn read_char(&mut self) -> char;

    /// Read a int8_t (signed).
    fn read_int8(&mut self) -> i8;

    /// Read a uint8_t (unsigned).
    fn read_uint8(&mut self) -> u8;

    /// Read a int16_t (signed).
    fn read_int16(&mut self) -> i16;

    /// Read a uint16_t (unsigned).
    fn read_uint16(&mut self) -> u16;

    /// Read a int32_t (signed).
    fn read_int32(&mut self) -> i32;

    /// Read a uint32_t (unsigned).
    fn read_uint32(&mut self) -> u32;

    /// Read a int64_t (signed).
    fn read_int64(&mut self) -> i64;

    /// Read a uint64_t (unsigned).
    fn read_uint64(&mut self) -> u64;

    /// Read a hugeint_t (two int64_t values).
    fn read_hugeint(&mut self) -> hugeint_t;

    /// Read a uhugeint_t (two uint64_t values).
    fn read_uhugeint(&mut self) -> uhugeint_t;

    /// Read a float value.
    fn read_float(&mut self) -> f32;

    /// Read a double value.
    fn read_double(&mut self) -> f64;

    /// Read a string_t value.
    fn read_string_t(&mut self) -> string_t;

    /// Read a string value.
    fn read_string(&mut self) -> string;

    /// Read raw data pointer.
    ///
    /// Returns an owned vector of bytes read from the stream.
    /// The caller owns the returned data and is responsible for its lifetime.
    fn read_data_ptr(&mut self, count: idx_t) -> Vec<u8>;

    /// Begin an object (verify opening marker).
    fn on_object_begin(&mut self);

    /// End an object (verify closing marker).
    fn on_object_end(&mut self);

    /// Begin a list (read count as varint).
    fn on_list_begin(&mut self) -> idx_t;

    /// End a list.
    fn on_list_end(&mut self);

    /// Begin a nullable (read present flag).
    fn on_nullable_begin(&mut self) -> bool;

    /// End a nullable.
    fn on_nullable_end(&mut self);

    /// Read a property begin marker with field id and tag.
    fn on_property_begin(&mut self, field_id: field_id_t, tag: *const u8) -> bool;

    /// Read a property end marker.
    fn on_property_end(&mut self);

    /// Read an optional property begin marker.
    fn on_optional_property_begin(&mut self, field_id: field_id_t, tag: *const u8) -> bool;

    /// Read an optional property end marker.
    fn on_optional_property_end(&mut self, present: bool);

    /// Read a value as a property.
    fn read_property(
        &mut self,
        field_id: field_id_t,
        tag: *const u8,
        value: *mut u8,
        count: idx_t,
    ) -> bool;

    /// Read a value with default.
    fn read_property_with_default(
        &mut self,
        field_id: field_id_t,
        tag: *const u8,
        value: *mut u8,
        count: idx_t,
        default_value: *const u8,
    ) -> bool;

    /// Read an object using a func.
    fn read_object_dyn(
        &mut self,
        field_id: field_id_t,
        tag: *const u8,
        f: &mut dyn FnMut(&mut dyn Deserializer),
    );

    /// Read a list using a func.
    fn read_list_dyn(
        &mut self,
        field_id: field_id_t,
        tag: *const u8,
        f: &mut dyn FnMut(&mut dyn Deserializer, idx_t),
    );
}

/// A concrete `Deserializer` implementation that reads from a `ReadStream`.
///
/// This is the Rust equivalent of DuckDB's `BinaryDeserializer` C++ class.
/// It reads data from an underlying `ReadStream` and uses LEB128 decoding
/// for variable-length integers.
pub struct BinaryDeserializer<'a> {
    /// The read stream to read data from.
    stream: &'a mut dyn ReadStream,

    /// Stack of deserialization data (contexts, databases, catalogs, etc.).
    data: SerializationData,

    /// Nesting level tracking.
    nesting_level: idx_t,

    /// Buffer for peeking at the next field.
    has_buffered_field: bool,
    buffered_field: field_id_t,
}

impl<'a> BinaryDeserializer<'a> {
    /// Create a new `BinaryDeserializer` that reads from the given stream.
    pub fn new(stream: &'a mut dyn ReadStream) -> Self {
        BinaryDeserializer {
            stream,
            data: SerializationData::default(),
            nesting_level: 0,
            has_buffered_field: false,
            buffered_field: 0,
        }
    }

    /// Deserialize a value of type T from the stream.
    ///
    /// This reads an object begin marker, calls the type's Deserialize method,
    /// and then reads the object end marker.
    pub fn deserialize<T: DeserializeInDeserializer>(&mut self) -> T {
        self.on_object_begin();
        let result = T::deserialize(self);
        self.on_object_end();
        result
    }

    /// Get the nesting level.
    pub fn nesting_level(&self) -> idx_t {
        self.nesting_level
    }

    /// Set the nesting level.
    pub fn set_nesting_level(&mut self, level: idx_t) {
        self.nesting_level = level;
    }

    /// Peek at the next field id without consuming it.
    pub fn peek_field(&mut self) -> field_id_t {
        if !self.has_buffered_field {
            self.buffered_field = self.stream.read_u16();
            self.has_buffered_field = true;
        }
        self.buffered_field
    }

    /// Consume the next field id.
    fn consume_field(&mut self) {
        if !self.has_buffered_field {
            self.buffered_field = self.stream.read_u16();
            self.has_buffered_field = true;
        } else {
            self.has_buffered_field = false;
        }
    }

    /// Read the next field id (using the buffered field if one is pending).
    fn next_field(&mut self) -> field_id_t {
        if self.has_buffered_field {
            self.has_buffered_field = false;
            return self.buffered_field;
        }
        self.stream.read_u16()
    }

    /// Read a LEB128 byte sequence, rejecting encodings wider than `bits` bits.
    fn read_varint_bytes(&mut self, bits: u32) -> ([u8; 10], usize) {
        let mut buffer = [0u8; 10];
        let mut count = 0usize;
        let mut shift = 0u32;
        loop {
            if shift >= bits {
                panic!("Failed to decode LEB128 integer: data may be corrupt");
            }
            let byte = self.stream.read_u8();
            buffer[count] = byte;
            count += 1;
            shift += 7;
            if byte & 0x80 == 0 {
                break;
            }
        }
        (buffer, count)
    }

    /// Read an unsigned LEB128 varint of at most `bits` width.
    fn read_varint_unsigned(&mut self, bits: u32) -> u64 {
        let (buffer, count) = self.read_varint_bytes(bits);
        let mut result = 0u64;
        decode_unsigned_leb128(&buffer[..count], &mut result);
        result
    }

    /// Read a signed LEB128 varint of at most `bits` width.
    fn read_varint_signed(&mut self, bits: u32) -> i64 {
        let (buffer, count) = self.read_varint_bytes(bits);
        let mut result = 0i64;
        decode_signed_leb128(&buffer[..count], &mut result);
        result
    }

    /// Read `count` bytes of property data into the caller-provided buffer.
    fn read_data_into(&mut self, value: *mut u8, count: idx_t) {
        let data = self.read_data_ptr(count);
        if count > 0 {
            debug_assert!(
                !value.is_null(),
                "read_data_into requires a non-null buffer"
            );
            unsafe {
                // SAFETY: `value` points to `count` writable bytes (caller
                // contract), and `data` contains exactly
                // `count` bytes.
                std::ptr::copy_nonoverlapping(data.as_ptr(), value, count as usize);
            }
        }
    }
}

impl fmt::Debug for BinaryDeserializer<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BinaryDeserializer")
            .field("data", &self.data)
            .field("nesting_level", &self.nesting_level)
            .finish_non_exhaustive()
    }
}

impl<'a> Deserializer for BinaryDeserializer<'a> {
    fn get_serialization_data(&self) -> &SerializationData {
        &self.data
    }

    fn set_serialization_data(&mut self, data: &SerializationData) {
        self.data = data.clone();
    }

    fn read_bool(&mut self) -> bool {
        self.stream.read_u8() != 0
    }

    fn read_char(&mut self) -> char {
        self.stream.read_u8() as char
    }

    fn read_int8(&mut self) -> i8 {
        self.read_varint_signed(8) as i8
    }

    fn read_uint8(&mut self) -> u8 {
        self.read_varint_unsigned(8) as u8
    }

    fn read_int16(&mut self) -> i16 {
        self.read_varint_signed(16) as i16
    }

    fn read_uint16(&mut self) -> u16 {
        self.read_varint_unsigned(16) as u16
    }

    fn read_int32(&mut self) -> i32 {
        self.read_varint_signed(32) as i32
    }

    fn read_uint32(&mut self) -> u32 {
        self.read_varint_unsigned(32) as u32
    }

    fn read_int64(&mut self) -> i64 {
        self.read_varint_signed(64)
    }

    fn read_uint64(&mut self) -> u64 {
        self.read_varint_unsigned(64)
    }

    fn read_hugeint(&mut self) -> hugeint_t {
        let upper = self.read_varint_signed(64);
        let lower = self.read_varint_unsigned(64);
        hugeint_t { upper, lower }
    }

    fn read_uhugeint(&mut self) -> uhugeint_t {
        let upper = self.read_varint_unsigned(64);
        let lower = self.read_varint_unsigned(64);
        uhugeint_t { upper, lower }
    }

    fn read_float(&mut self) -> f32 {
        self.stream.read_f32()
    }

    fn read_double(&mut self) -> f64 {
        self.stream.read_f64()
    }

    fn read_string_t(&mut self) -> string_t {
        let len = self.read_varint_unsigned(32);
        let data = self.stream.read_bytes(len);
        string_t {
            size: data.len(),
            data,
        }
    }

    fn read_string(&mut self) -> string {
        let string_t = self.read_string_t();
        string {
            data: String::from_utf8_lossy(string_t.get_data()).into_owned(),
        }
    }

    fn read_data_ptr(&mut self, count: idx_t) -> Vec<u8> {
        let len = self.read_varint_unsigned(64);
        if len != count {
            panic!(
                "Tried to read blob of {} size, but only {} elements are available",
                count, len
            );
        }
        self.stream.read_bytes(len)
    }

    fn on_object_begin(&mut self) {
        self.nesting_level += 1;
    }

    fn on_object_end(&mut self) {
        let field = self.next_field();
        if field != MESSAGE_TERMINATOR_FIELD_ID {
            panic!(
                "Failed to deserialize: expected end of object, but found field id: {}",
                field
            );
        }
        self.nesting_level -= 1;
    }

    fn on_list_begin(&mut self) -> idx_t {
        self.read_varint_unsigned(64)
    }

    fn on_list_end(&mut self) {
        // Nothing needed for binary format
    }

    fn on_nullable_begin(&mut self) -> bool {
        self.read_bool()
    }

    fn on_nullable_end(&mut self) {
        // Nothing needed for binary format
    }

    fn on_property_begin(&mut self, field_id: field_id_t, _tag: *const u8) -> bool {
        let field = self.next_field();
        if field != field_id {
            panic!(
                "Failed to deserialize: field id mismatch, expected: {}, got: {}",
                field_id, field
            );
        }
        true
    }

    fn on_property_end(&mut self) {
        // Nothing needed for binary format
    }

    fn on_optional_property_begin(&mut self, field_id: field_id_t, _tag: *const u8) -> bool {
        let present = self.peek_field() == field_id;
        if present {
            self.consume_field();
        }
        present
    }

    fn on_optional_property_end(&mut self, _present: bool) {
        // Nothing needed for binary format
    }

    fn read_property(
        &mut self,
        field_id: field_id_t,
        tag: *const u8,
        value: *mut u8,
        count: idx_t,
    ) -> bool {
        self.on_property_begin(field_id, tag);
        self.read_data_into(value, count);
        self.on_property_end();
        true
    }

    // Dereferences `value`/`default_value`; each must cover `count` bytes per
    // the trait contract.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn read_property_with_default(
        &mut self,
        field_id: field_id_t,
        tag: *const u8,
        value: *mut u8,
        count: idx_t,
        default_value: *const u8,
    ) -> bool {
        if !self.on_optional_property_begin(field_id, tag) {
            if count > 0 {
                debug_assert!(
                    !value.is_null() && !default_value.is_null(),
                    "null buffer with non-zero count"
                );
                unsafe {
                    // SAFETY: caller provides `count` readable bytes at
                    // `default_value` and `count` writable
                    // bytes at `value`.
                    std::ptr::copy_nonoverlapping(default_value, value, count as usize);
                }
            }
            self.on_optional_property_end(false);
            return false;
        }
        self.read_data_into(value, count);
        self.on_optional_property_end(true);
        true
    }

    fn read_object_dyn(
        &mut self,
        field_id: field_id_t,
        tag: *const u8,
        f: &mut dyn FnMut(&mut dyn Deserializer),
    ) {
        self.on_property_begin(field_id, tag);
        self.on_object_begin();
        f(self);
        self.on_object_end();
        self.on_property_end();
    }

    fn read_list_dyn(
        &mut self,
        field_id: field_id_t,
        tag: *const u8,
        f: &mut dyn FnMut(&mut dyn Deserializer, idx_t),
    ) {
        self.on_property_begin(field_id, tag);
        let count = self.on_list_begin();
        for i in 0..count {
            f(self, i);
        }
        self.on_list_end();
        self.on_property_end();
    }
}

/// Trait for types that can be deserialized from a `Deserializer`.
pub trait DeserializeInDeserializer: Sized {
    /// Deserialize the value from the given deserializer.
    fn deserialize(deserializer: &mut dyn Deserializer) -> Self;
}

// Implement DeserializeInDeserializer for primitive types

impl DeserializeInDeserializer for bool {
    fn deserialize(deserializer: &mut dyn Deserializer) -> Self {
        deserializer.read_bool()
    }
}

impl DeserializeInDeserializer for i8 {
    fn deserialize(deserializer: &mut dyn Deserializer) -> Self {
        deserializer.read_int8()
    }
}

impl DeserializeInDeserializer for u8 {
    fn deserialize(deserializer: &mut dyn Deserializer) -> Self {
        deserializer.read_uint8()
    }
}

impl DeserializeInDeserializer for i16 {
    fn deserialize(deserializer: &mut dyn Deserializer) -> Self {
        deserializer.read_int16()
    }
}

impl DeserializeInDeserializer for u16 {
    fn deserialize(deserializer: &mut dyn Deserializer) -> Self {
        deserializer.read_uint16()
    }
}

impl DeserializeInDeserializer for i32 {
    fn deserialize(deserializer: &mut dyn Deserializer) -> Self {
        deserializer.read_int32()
    }
}

impl DeserializeInDeserializer for u32 {
    fn deserialize(deserializer: &mut dyn Deserializer) -> Self {
        deserializer.read_uint32()
    }
}

impl DeserializeInDeserializer for i64 {
    fn deserialize(deserializer: &mut dyn Deserializer) -> Self {
        deserializer.read_int64()
    }
}

impl DeserializeInDeserializer for u64 {
    fn deserialize(deserializer: &mut dyn Deserializer) -> Self {
        deserializer.read_uint64()
    }
}

impl DeserializeInDeserializer for f32 {
    fn deserialize(deserializer: &mut dyn Deserializer) -> Self {
        deserializer.read_float()
    }
}

impl DeserializeInDeserializer for f64 {
    fn deserialize(deserializer: &mut dyn Deserializer) -> Self {
        deserializer.read_double()
    }
}

impl DeserializeInDeserializer for string_t {
    fn deserialize(deserializer: &mut dyn Deserializer) -> Self {
        deserializer.read_string_t()
    }
}

impl DeserializeInDeserializer for string {
    fn deserialize(deserializer: &mut dyn Deserializer) -> Self {
        deserializer.read_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serializer::{
        memory_stream::MemoryStream,
        serializer::{BinarySerializer, SerializationOptions, SerializeInSerializer, Serializer},
        write_stream::WriteStream,
    };

    fn serialize(f: impl FnOnce(&mut BinarySerializer)) -> MemoryStream {
        let mut stream = MemoryStream::new();
        {
            let mut serializer =
                BinarySerializer::new(&mut stream, SerializationOptions::default());
            f(&mut serializer);
        }
        stream
    }

    fn bytes_of(f: impl FnOnce(&mut BinarySerializer)) -> Vec<u8> {
        serialize(f).get_data().to_vec()
    }

    fn assert_roundtrip(
        write: impl FnOnce(&mut BinarySerializer),
        read: impl FnOnce(&mut BinaryDeserializer),
    ) {
        let mut stream = serialize(write);
        stream.rewind();
        let mut deserializer = BinaryDeserializer::new(&mut stream);
        read(&mut deserializer);
    }

    #[test]
    fn test_deserializer_new() {
        let mut stream = MemoryStream::new();
        let deserializer = BinaryDeserializer::new(&mut stream);
        assert_eq!(deserializer.nesting_level(), 0);
    }

    #[test]
    fn test_bool_roundtrip() {
        assert_eq!(bytes_of(|s| s.write_bool(true)), [1]);
        assert_eq!(bytes_of(|s| s.write_bool(false)), [0]);
        assert_roundtrip(
            |s| {
                s.write_bool(true);
                s.write_bool(false);
                s.write_bool(true);
            },
            |d| {
                assert!(d.read_bool());
                assert!(!d.read_bool());
                assert!(d.read_bool());
            },
        );
    }

    #[test]
    fn test_char_roundtrip() {
        assert_eq!(bytes_of(|s| s.write_char('A')), [0x41]);
        assert_roundtrip(
            |s| {
                s.write_char('A');
                s.write_char('\0');
                s.write_char('\u{7f}');
            },
            |d| {
                assert_eq!(d.read_char(), 'A');
                assert_eq!(d.read_char(), '\0');
                assert_eq!(d.read_char(), '\u{7f}');
            },
        );
    }

    #[test]
    fn test_int8_roundtrip() {
        assert_roundtrip(
            |s| {
                s.write_int8(0);
                s.write_int8(-1);
                s.write_int8(127);
                s.write_int8(-128);
            },
            |d| {
                assert_eq!(d.read_int8(), 0);
                assert_eq!(d.read_int8(), -1);
                assert_eq!(d.read_int8(), 127);
                assert_eq!(d.read_int8(), -128);
            },
        );
    }

    #[test]
    fn test_uint8_roundtrip() {
        assert_roundtrip(
            |s| {
                s.write_uint8(0);
                s.write_uint8(127);
                s.write_uint8(128);
                s.write_uint8(255);
            },
            |d| {
                assert_eq!(d.read_uint8(), 0);
                assert_eq!(d.read_uint8(), 127);
                assert_eq!(d.read_uint8(), 128);
                assert_eq!(d.read_uint8(), 255);
            },
        );
    }

    #[test]
    fn test_int16_roundtrip() {
        assert_roundtrip(
            |s| {
                s.write_int16(i16::MIN);
                s.write_int16(-1);
                s.write_int16(0);
                s.write_int16(i16::MAX);
            },
            |d| {
                assert_eq!(d.read_int16(), i16::MIN);
                assert_eq!(d.read_int16(), -1);
                assert_eq!(d.read_int16(), 0);
                assert_eq!(d.read_int16(), i16::MAX);
            },
        );
    }

    #[test]
    fn test_uint16_roundtrip() {
        assert_roundtrip(
            |s| {
                s.write_uint16(0);
                s.write_uint16(u16::MAX);
            },
            |d| {
                assert_eq!(d.read_uint16(), 0);
                assert_eq!(d.read_uint16(), u16::MAX);
            },
        );
    }

    #[test]
    fn test_int32_roundtrip() {
        assert_roundtrip(
            |s| {
                s.write_int32(i32::MIN);
                s.write_int32(-1000);
                s.write_int32(0);
                s.write_int32(i32::MAX);
            },
            |d| {
                assert_eq!(d.read_int32(), i32::MIN);
                assert_eq!(d.read_int32(), -1000);
                assert_eq!(d.read_int32(), 0);
                assert_eq!(d.read_int32(), i32::MAX);
            },
        );
    }

    #[test]
    fn test_uint32_roundtrip() {
        assert_roundtrip(
            |s| {
                s.write_uint32(0);
                s.write_uint32(128);
                s.write_uint32(u32::MAX);
            },
            |d| {
                assert_eq!(d.read_uint32(), 0);
                assert_eq!(d.read_uint32(), 128);
                assert_eq!(d.read_uint32(), u32::MAX);
            },
        );
    }

    #[test]
    fn test_int64_roundtrip() {
        assert_roundtrip(
            |s| {
                s.write_int64(i64::MIN);
                s.write_int64(-1);
                s.write_int64(0);
                s.write_int64(i64::MAX);
            },
            |d| {
                assert_eq!(d.read_int64(), i64::MIN);
                assert_eq!(d.read_int64(), -1);
                assert_eq!(d.read_int64(), 0);
                assert_eq!(d.read_int64(), i64::MAX);
            },
        );
    }

    #[test]
    fn test_uint64_roundtrip() {
        assert_roundtrip(
            |s| {
                s.write_uint64(0);
                s.write_uint64(1u64 << 63);
                s.write_uint64(u64::MAX);
            },
            |d| {
                assert_eq!(d.read_uint64(), 0);
                assert_eq!(d.read_uint64(), 1u64 << 63);
                assert_eq!(d.read_uint64(), u64::MAX);
            },
        );
    }

    #[test]
    fn test_varint_byte_format() {
        // LEB128 encodings must match the C++ EncodingUtil byte-for-byte
        assert_eq!(bytes_of(|s| s.write_uint32(0)), [0x00]);
        assert_eq!(bytes_of(|s| s.write_uint32(127)), [0x7f]);
        assert_eq!(bytes_of(|s| s.write_uint32(128)), [0x80, 0x01]);
        assert_eq!(bytes_of(|s| s.write_int32(-1)), [0x7f]);
        assert_eq!(
            bytes_of(|s| s.write_int64(i64::MIN)),
            [0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x7f]
        );
    }

    #[test]
    fn test_float_double_roundtrip() {
        assert_eq!(bytes_of(|s| s.write_float(2.0)), 2.0f32.to_le_bytes());
        assert_roundtrip(
            |s| {
                s.write_float(std::f32::consts::PI);
                s.write_float(f32::NEG_INFINITY);
                s.write_double(-std::f64::consts::E);
                s.write_double(f64::INFINITY);
            },
            |d| {
                assert_eq!(d.read_float(), std::f32::consts::PI);
                assert_eq!(d.read_float(), f32::NEG_INFINITY);
                assert_eq!(d.read_double(), -std::f64::consts::E);
                assert_eq!(d.read_double(), f64::INFINITY);
            },
        );
    }

    #[test]
    fn test_string_roundtrip() {
        assert_roundtrip(
            |s| {
                s.write_string(string {
                    data: "hello world".to_string(),
                });
                s.write_string(string {
                    data: String::new(),
                });
            },
            |d| {
                assert_eq!(
                    d.read_string(),
                    string {
                        data: "hello world".to_string()
                    }
                );
                assert_eq!(
                    d.read_string(),
                    string {
                        data: String::new()
                    }
                );
            },
        );
    }

    #[test]
    fn test_string_t_roundtrip() {
        assert_roundtrip(
            |s| {
                s.write_string_t(string_t {
                    data: b"abc123".to_vec(),
                    size: 3,
                });
                s.write_string_t(string_t {
                    data: Vec::new(),
                    size: 0,
                });
            },
            |d| {
                // only the first `size` bytes of a string_t are written
                assert_eq!(
                    d.read_string_t(),
                    string_t {
                        data: b"abc".to_vec(),
                        size: 3
                    }
                );
                assert_eq!(
                    d.read_string_t(),
                    string_t {
                        data: Vec::new(),
                        size: 0
                    }
                );
            },
        );
    }

    #[test]
    fn test_hugeint_roundtrip() {
        assert_roundtrip(
            |s| {
                s.write_hugeint(hugeint_t {
                    upper: -12345,
                    lower: u64::MAX,
                });
                s.write_hugeint(hugeint_t {
                    upper: i64::MIN,
                    lower: 0,
                });
            },
            |d| {
                assert_eq!(
                    d.read_hugeint(),
                    hugeint_t {
                        upper: -12345,
                        lower: u64::MAX
                    }
                );
                assert_eq!(
                    d.read_hugeint(),
                    hugeint_t {
                        upper: i64::MIN,
                        lower: 0
                    }
                );
            },
        );
    }

    #[test]
    fn test_uhugeint_roundtrip() {
        assert_roundtrip(
            |s| {
                s.write_uhugeint(uhugeint_t {
                    upper: u64::MAX,
                    lower: 0xdeadbeef,
                });
                s.write_uhugeint(uhugeint_t { upper: 0, lower: 1 });
            },
            |d| {
                assert_eq!(
                    d.read_uhugeint(),
                    uhugeint_t {
                        upper: u64::MAX,
                        lower: 0xdeadbeef
                    }
                );
                assert_eq!(d.read_uhugeint(), uhugeint_t { upper: 0, lower: 1 });
            },
        );
    }

    #[test]
    fn test_object_roundtrip() {
        assert_eq!(bytes_of(|s| s.serialize(&42i64)), [0x2a, 0xff, 0xff]);
        let mut stream = serialize(|s| s.serialize(&(-7i64)));
        stream.rewind();
        let mut d = BinaryDeserializer::new(&mut stream);
        assert_eq!(d.deserialize::<i64>(), -7);
        assert_eq!(d.nesting_level(), 0);
    }

    #[test]
    fn test_object_dyn_roundtrip() {
        assert_roundtrip(
            |s| {
                s.on_object_begin();
                s.write_object_dyn(3, b"obj".as_ptr(), &mut |s| {
                    s.write_int64(42);
                });
                s.on_object_end();
            },
            |d| {
                d.on_object_begin();
                d.read_object_dyn(3, b"obj".as_ptr(), &mut |d| {
                    assert_eq!(d.read_int64(), 42);
                });
                d.on_object_end();
            },
        );
    }

    #[test]
    fn test_list_dyn_roundtrip() {
        let values = [10i64, -20, 30];
        assert_roundtrip(
            |s| {
                s.on_object_begin();
                s.write_list_dyn(4, b"list".as_ptr(), 3, &mut |s, i| {
                    s.write_int64(values[i as usize]);
                });
                s.on_object_end();
            },
            |d| {
                d.on_object_begin();
                let mut read = Vec::new();
                d.read_list_dyn(4, b"list".as_ptr(), &mut |d, _i| {
                    read.push(d.read_int64());
                });
                d.on_object_end();
                assert_eq!(read, values);
            },
        );
    }

    #[derive(Debug, PartialEq)]
    struct Point {
        x: i32,
        y: i32,
    }

    impl SerializeInSerializer for Point {
        fn serialize(&self, serializer: &mut dyn Serializer) {
            serializer.on_property_begin(0, b"x".as_ptr());
            serializer.write_int32(self.x);
            serializer.on_property_end();
            serializer.on_property_begin(1, b"y".as_ptr());
            serializer.write_int32(self.y);
            serializer.on_property_end();
        }
    }

    impl DeserializeInDeserializer for Point {
        fn deserialize(deserializer: &mut dyn Deserializer) -> Self {
            deserializer.on_property_begin(0, b"x".as_ptr());
            let x = deserializer.read_int32();
            deserializer.on_property_end();
            deserializer.on_property_begin(1, b"y".as_ptr());
            let y = deserializer.read_int32();
            deserializer.on_property_end();
            Point { x, y }
        }
    }

    #[test]
    fn test_struct_roundtrip() {
        let point = Point { x: -3, y: 7 };
        assert_eq!(
            bytes_of(|s| s.serialize(&point)),
            [0x00, 0x00, 0x7d, 0x01, 0x00, 0x07, 0xff, 0xff]
        );
        assert_roundtrip(
            |s| s.serialize(&point),
            |d| assert_eq!(d.deserialize::<Point>(), point),
        );
    }

    #[test]
    fn test_byte_format_property_object() {
        // field id 7 as raw u16 LE, value -1 as signed LEB128, nested + outer
        // terminators as raw u16 LE
        assert_eq!(
            bytes_of(|s| {
                s.on_object_begin();
                s.write_object_dyn(7, b"tag".as_ptr(), &mut |s| s.write_int64(-1));
                s.on_object_end();
            }),
            [0x07, 0x00, 0x7f, 0xff, 0xff, 0xff, 0xff]
        );
    }

    #[test]
    fn test_byte_format_string() {
        assert_eq!(
            bytes_of(|s| s.write_string(string {
                data: "hello".to_string()
            })),
            [0x05, b'h', b'e', b'l', b'l', b'o']
        );
    }

    #[test]
    fn test_nullable_roundtrip() {
        assert_roundtrip(
            |s| {
                s.on_nullable_begin(true);
                s.write_int64(-5);
                s.on_nullable_end();
                s.on_nullable_begin(false);
                s.on_nullable_end();
            },
            |d| {
                assert!(d.on_nullable_begin());
                assert_eq!(d.read_int64(), -5);
                d.on_nullable_end();
                assert!(!d.on_nullable_begin());
                d.on_nullable_end();
            },
        );
    }

    #[test]
    fn test_data_ptr_roundtrip() {
        assert_roundtrip(
            |s| {
                let data = [1u8, 2, 3, 4, 5];
                s.write_data_ptr(data.as_ptr(), 5);
            },
            |d| assert_eq!(d.read_data_ptr(5), vec![1, 2, 3, 4, 5]),
        );
    }

    #[test]
    fn test_property_roundtrip() {
        let mut stream = serialize(|s| {
            let data = [10u8, 20, 30];
            s.on_object_begin();
            s.write_property(1, b"tag".as_ptr(), data.as_ptr(), 3);
            s.on_object_end();
        });
        assert_eq!(
            stream.get_data(),
            [0x01, 0x00, 0x03, 0x0a, 0x14, 0x1e, 0xff, 0xff]
        );
        stream.rewind();
        let mut d = BinaryDeserializer::new(&mut stream);
        let mut buf = [0u8; 3];
        assert!(d.read_property(1, b"tag".as_ptr(), buf.as_mut_ptr(), 3));
        assert_eq!(buf, [10, 20, 30]);
    }

    #[test]
    fn test_property_with_default_present() {
        let mut stream = serialize(|s| {
            let value = [1u8, 2];
            let default = [9u8, 9];
            s.on_object_begin();
            s.write_property_with_default(2, b"tag".as_ptr(), value.as_ptr(), 2, default.as_ptr());
            s.on_object_end();
        });
        assert_eq!(
            stream.get_data(),
            [0x02, 0x00, 0x02, 0x01, 0x02, 0xff, 0xff]
        );
        stream.rewind();
        let mut d = BinaryDeserializer::new(&mut stream);
        let mut buf = [0u8; 2];
        let default = [9u8, 9];
        assert!(d.read_property_with_default(
            2,
            b"tag".as_ptr(),
            buf.as_mut_ptr(),
            2,
            default.as_ptr()
        ));
        assert_eq!(buf, [1, 2]);
    }

    #[test]
    fn test_property_with_default_absent() {
        let mut stream = serialize(|s| {
            let value = [9u8, 9];
            let default = [9u8, 9];
            s.on_object_begin();
            s.write_property_with_default(2, b"tag".as_ptr(), value.as_ptr(), 2, default.as_ptr());
            let other = [7u8, 7];
            s.write_property(3, b"other".as_ptr(), other.as_ptr(), 2);
            s.on_object_end();
        });
        // the default-equal value was skipped, only field 3 is on the wire
        assert_eq!(
            stream.get_data(),
            [0x03, 0x00, 0x02, 0x07, 0x07, 0xff, 0xff]
        );
        stream.rewind();
        let mut d = BinaryDeserializer::new(&mut stream);
        let mut buf = [0u8; 2];
        let default = [9u8, 9];
        assert!(!d.read_property_with_default(
            2,
            b"tag".as_ptr(),
            buf.as_mut_ptr(),
            2,
            default.as_ptr()
        ));
        assert_eq!(buf, [9, 9]);
        // the buffered field 3 is still available afterwards
        let mut other = [0u8; 2];
        assert!(d.read_property(3, b"other".as_ptr(), other.as_mut_ptr(), 2));
        assert_eq!(other, [7, 7]);
    }

    #[test]
    fn test_optional_property_and_deleted_property() {
        let mut stream = serialize(|s| {
            let data = [1u8, 2, 3];
            s.on_object_begin();
            s.write_property(5, b"five".as_ptr(), data.as_ptr(), 3);
            s.on_object_end();
        });
        stream.rewind();
        let mut d = BinaryDeserializer::new(&mut stream);
        // field 4 was deleted: the optional read reports absence without
        // consuming field 5
        assert!(!d.on_optional_property_begin(4, b"four".as_ptr()));
        d.on_optional_property_end(false);
        // field 5 is present: consume it (deleted-property pattern reads and
        // discards it)
        assert!(d.on_optional_property_begin(5, b"five".as_ptr()));
        assert_eq!(d.read_data_ptr(3), vec![1, 2, 3]);
        d.on_optional_property_end(true);
    }

    #[test]
    #[should_panic(expected = "field id mismatch")]
    fn test_field_id_mismatch_panics() {
        let mut stream = serialize(|s| {
            let data = [1u8, 2, 3, 4];
            s.on_object_begin();
            s.write_property(5, b"tag".as_ptr(), data.as_ptr(), 4);
            s.on_object_end();
        });
        stream.rewind();
        let mut d = BinaryDeserializer::new(&mut stream);
        let mut buf = [0u8; 4];
        d.read_property(7, b"tag".as_ptr(), buf.as_mut_ptr(), 4);
    }

    #[test]
    #[should_panic(expected = "expected end of object")]
    fn test_object_end_terminator_mismatch_panics() {
        let data = [0x2au8, 0x01, 0x00];
        let mut stream = MemoryStream::new();
        stream.write_data(data.as_ptr(), data.len() as idx_t);
        stream.rewind();
        let mut d = BinaryDeserializer::new(&mut stream);
        d.deserialize::<i64>();
    }

    #[test]
    #[should_panic(expected = "Failed to decode LEB128")]
    fn test_overlong_varint_panics() {
        let data = [0x80u8; 11];
        let mut stream = MemoryStream::new();
        stream.write_data(data.as_ptr(), data.len() as idx_t);
        stream.rewind();
        let mut d = BinaryDeserializer::new(&mut stream);
        d.read_uint64();
    }

    #[test]
    #[should_panic(expected = "Failed to decode LEB128")]
    fn test_overlong_string_length_panics() {
        let data = [0x80u8; 6];
        let mut stream = MemoryStream::new();
        stream.write_data(data.as_ptr(), data.len() as idx_t);
        stream.rewind();
        let mut d = BinaryDeserializer::new(&mut stream);
        d.read_string_t();
    }

    #[test]
    #[should_panic(expected = "Tried to read blob")]
    fn test_data_ptr_size_mismatch_panics() {
        let mut stream = serialize(|s| {
            let data = [1u8, 2, 3, 4, 5];
            s.write_data_ptr(data.as_ptr(), 5);
        });
        stream.rewind();
        let mut d = BinaryDeserializer::new(&mut stream);
        d.read_data_ptr(3);
    }

    #[test]
    #[should_panic(expected = "not enough data")]
    fn test_read_past_end_panics() {
        let mut stream = MemoryStream::new();
        let mut d = BinaryDeserializer::new(&mut stream);
        d.read_int64();
    }
}
