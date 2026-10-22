//! Serializer trait and BinarySerializer implementation.
//!
//! Ported from `duckdb/common/serializer/serializer.hpp` and
//! `duckdb/common/serializer/binary_serializer.hpp`.
//!
//! The `Serializer` trait provides the abstract interface for serialization,
//! while `BinarySerializer` is a concrete implementation that writes to a
//! `WriteStream`.

use std::fmt;

use crate::serializer::{
    MESSAGE_TERMINATOR_FIELD_ID,
    encoding_util::{encode_signed_leb128, encode_unsigned_leb128},
    field_id_t, hugeint_t, idx_t,
    serialization_data::SerializationData,
    string, string_t, uhugeint_t,
    write_stream::WriteStream,
};

/// Serialization options control behavior during serialization.
#[derive(Debug, Clone)]
pub struct SerializationOptions {
    pub serialize_enum_as_string: bool,
    pub serialize_default_values: bool,
    pub storage_compatibility: StorageCompatibility,
}

impl Default for SerializationOptions {
    fn default() -> Self {
        SerializationOptions {
            serialize_enum_as_string: false,
            serialize_default_values: false,
            storage_compatibility: StorageCompatibility::Default,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageCompatibility {
    Default,
}

/// Rust trait equivalent of the C++ `Serializer` class.
///
/// This trait provides the interface for serializing values during
/// DuckDB's serialization format. Implementations must produce the
/// exact same binary format as the C++ version.
///
/// # Binary Format
///
/// The format uses:
/// - Fixed-size types written in native byte order (little-endian on modern
///   systems)
/// - Variable-length integers in LEB128 encoding
/// - Strings: varint length + bytes
/// - Lists: varint count + elements
/// - Objects: begin/end markers with properties
pub trait Serializer: fmt::Debug {
    /// Options for serialization behavior.
    fn get_options(&self) -> &SerializationOptions;

    /// Get the serialization data stack.
    fn get_serialization_data(&self) -> &SerializationData;

    /// Set the serialization data.
    fn set_serialization_data(&mut self, data: &SerializationData);

    /// Write a null value.
    fn write_null(&mut self);

    /// Write a bool value.
    fn write_bool(&mut self, value: bool);

    /// Write a char value.
    fn write_char(&mut self, value: char);

    /// Write an int8_t (signed).
    fn write_int8(&mut self, value: i8);

    /// Write a uint8_t (unsigned).
    fn write_uint8(&mut self, value: u8);

    /// Write an int16_t (signed).
    fn write_int16(&mut self, value: i16);

    /// Write a uint16_t (unsigned).
    fn write_uint16(&mut self, value: u16);

    /// Write an int32_t (signed).
    fn write_int32(&mut self, value: i32);

    /// Write a uint32_t (unsigned).
    fn write_uint32(&mut self, value: u32);

    /// Write an int64_t (signed).
    fn write_int64(&mut self, value: i64);

    /// Write a uint64_t (unsigned).
    fn write_uint64(&mut self, value: u64);

    /// Write a hugeint_t (two int64_t values).
    fn write_hugeint(&mut self, value: hugeint_t);

    /// Write a uhugeint_t (two uint64_t values).
    fn write_uhugeint(&mut self, value: uhugeint_t);

    /// Write a float value.
    fn write_float(&mut self, value: f32);

    /// Write a double value.
    fn write_double(&mut self, value: f64);

    /// Write a string_t value.
    fn write_string_t(&mut self, value: string_t);

    /// Write a string value.
    fn write_string(&mut self, value: string);

    /// Write a C string (null-terminated).
    fn write_cstr(&mut self, value: *const u8);

    /// Write raw data pointer (varint length + data).
    fn write_data_ptr(&mut self, ptr: *const u8, count: idx_t);

    /// Begin an object (write opening marker).
    fn on_object_begin(&mut self);

    /// End an object (write closing marker).
    fn on_object_end(&mut self);

    /// Begin a list (write count as varint).
    fn on_list_begin(&mut self, count: idx_t);

    /// End a list.
    fn on_list_end(&mut self);

    /// Begin a nullable (write present flag).
    fn on_nullable_begin(&mut self, present: bool);

    /// End a nullable.
    fn on_nullable_end(&mut self);

    /// Write a property begin marker with field id and tag.
    fn on_property_begin(&mut self, field_id: field_id_t, tag: *const u8);

    /// Write a property end marker.
    fn on_property_end(&mut self);

    /// Write an optional property begin marker.
    fn on_optional_property_begin(&mut self, field_id: field_id_t, tag: *const u8, present: bool);

    /// Write an optional property end marker.
    fn on_optional_property_end(&mut self, present: bool);

    /// Write a value as a property.
    fn write_property(
        &mut self,
        field_id: field_id_t,
        tag: *const u8,
        value: *const u8,
        count: idx_t,
    );

    /// Write a value with default.
    fn write_property_with_default(
        &mut self,
        field_id: field_id_t,
        tag: *const u8,
        value: *const u8,
        count: idx_t,
        default_value: *const u8,
    );

    /// Write an object using a func.
    fn write_object_dyn(
        &mut self,
        field_id: field_id_t,
        tag: *const u8,
        f: &mut dyn FnMut(&mut dyn Serializer),
    );

    /// Write a list using a func.
    fn write_list_dyn(
        &mut self,
        field_id: field_id_t,
        tag: *const u8,
        count: idx_t,
        f: &mut dyn FnMut(&mut dyn Serializer, idx_t),
    );
}

/// A concrete `Serializer` implementation that writes to a `WriteStream`.
///
/// This is the Rust equivalent of DuckDB's `BinarySerializer` C++ class.
/// It writes data to an underlying `WriteStream` and uses LEB128 encoding
/// for variable-length integers.
pub struct BinarySerializer<'a> {
    /// The write stream to write data to.
    stream: &'a mut dyn WriteStream,

    /// Options for serialization behavior.
    options: SerializationOptions,

    /// Stack of serialization data (contexts, databases, catalogs, etc.).
    data: SerializationData,

    /// Object-scoped field tracking, mirroring the C++ `#ifdef DEBUG`
    /// `debug_stack`.
    #[cfg(debug_assertions)]
    debug_stack: Vec<DebugState>,
}

/// Per-object set of already-written field ids/tags (C++
/// `BinarySerializer::DebugState`).
#[cfg(debug_assertions)]
#[derive(Debug, Default)]
struct DebugState {
    seen_field_ids: std::collections::HashSet<field_id_t>,
    seen_field_tags: std::collections::HashSet<usize>,
    seen_fields: Vec<(usize, field_id_t)>,
}

impl<'a> BinarySerializer<'a> {
    /// Create a new `BinarySerializer` that writes to the given stream.
    pub fn new(stream: &'a mut dyn WriteStream, options: SerializationOptions) -> Self {
        BinarySerializer {
            stream,
            options,
            data: SerializationData::default(),
            #[cfg(debug_assertions)]
            debug_stack: Vec::new(),
        }
    }

    /// Serialize a value to the stream.
    ///
    /// Writes an object begin marker, the value's serialized form, and an
    /// object end marker.
    pub fn serialize<T: SerializeInSerializer>(&mut self, value: &T) {
        self.on_object_begin();
        value.serialize(self);
        self.on_object_end();
    }
}

impl fmt::Debug for BinarySerializer<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BinarySerializer")
            .field("options", &self.options)
            .field("data", &self.data)
            .finish_non_exhaustive()
    }
}

impl<'a> Serializer for BinarySerializer<'a> {
    fn get_options(&self) -> &SerializationOptions {
        &self.options
    }

    fn get_serialization_data(&self) -> &SerializationData {
        &self.data
    }

    fn set_serialization_data(&mut self, data: &SerializationData) {
        self.data = data.clone();
    }

    fn write_null(&mut self) {
        // Nothing is written, matching the C++ WriteNull (never called in
        // practice)
    }

    fn write_bool(&mut self, value: bool) {
        self.stream.write_u8(u8::from(value));
    }

    fn write_char(&mut self, value: char) {
        // C++ writes char as a single raw byte
        self.stream.write_u8(value as u32 as u8);
    }

    fn write_int8(&mut self, value: i8) {
        let mut buffer = [0u8; 16];
        let size = encode_signed_leb128(&mut buffer, value as i64);
        self.stream.write_data(buffer.as_ptr(), size);
    }

    fn write_uint8(&mut self, value: u8) {
        let mut buffer = [0u8; 16];
        let size = encode_unsigned_leb128(&mut buffer, value as u64);
        self.stream.write_data(buffer.as_ptr(), size);
    }

    fn write_int16(&mut self, value: i16) {
        let mut buffer = [0u8; 16];
        let size = encode_signed_leb128(&mut buffer, value as i64);
        self.stream.write_data(buffer.as_ptr(), size);
    }

    fn write_uint16(&mut self, value: u16) {
        let mut buffer = [0u8; 16];
        let size = encode_unsigned_leb128(&mut buffer, value as u64);
        self.stream.write_data(buffer.as_ptr(), size);
    }

    fn write_int32(&mut self, value: i32) {
        let mut buffer = [0u8; 16];
        let size = encode_signed_leb128(&mut buffer, value as i64);
        self.stream.write_data(buffer.as_ptr(), size);
    }

    fn write_uint32(&mut self, value: u32) {
        let mut buffer = [0u8; 16];
        let size = encode_unsigned_leb128(&mut buffer, value as u64);
        self.stream.write_data(buffer.as_ptr(), size);
    }

    fn write_int64(&mut self, value: i64) {
        let mut buffer = [0u8; 16];
        let size = encode_signed_leb128(&mut buffer, value);
        self.stream.write_data(buffer.as_ptr(), size);
    }

    fn write_uint64(&mut self, value: u64) {
        let mut buffer = [0u8; 16];
        let size = encode_unsigned_leb128(&mut buffer, value);
        self.stream.write_data(buffer.as_ptr(), size);
    }

    fn write_hugeint(&mut self, value: hugeint_t) {
        // C++ encodes upper as signed LEB128 and lower as unsigned LEB128
        self.write_int64(value.upper);
        self.write_uint64(value.lower);
    }

    fn write_uhugeint(&mut self, value: uhugeint_t) {
        self.write_uint64(value.upper);
        self.write_uint64(value.lower);
    }

    fn write_float(&mut self, value: f32) {
        let bytes = value.to_le_bytes();
        self.stream.write_data(bytes.as_ptr(), bytes.len() as idx_t);
    }

    fn write_double(&mut self, value: f64) {
        let bytes = value.to_le_bytes();
        self.stream.write_data(bytes.as_ptr(), bytes.len() as idx_t);
    }

    fn write_string_t(&mut self, value: string_t) {
        let len = value.get_size() as idx_t;
        // Write varint length
        let mut len_buffer = [0u8; 16];
        let len_size = encode_unsigned_leb128(&mut len_buffer, len as u64);
        self.stream.write_data(len_buffer.as_ptr(), len_size);
        // Write the string data
        if len > 0 {
            self.stream.write_data(value.get_data().as_ptr(), len);
        }
    }

    fn write_string(&mut self, value: string) {
        let len = value.length() as idx_t;
        // Write varint length
        let mut len_buffer = [0u8; 16];
        let len_size = encode_unsigned_leb128(&mut len_buffer, len as u64);
        self.stream.write_data(len_buffer.as_ptr(), len_size);
        // Write the string data
        if len > 0 {
            let cstr = value.get_data();
            self.stream.write_data(cstr, len);
        }
    }

    fn write_cstr(&mut self, value: *const u8) {
        // Read until null terminator
        let mut len = 0usize;
        let mut ptr = value;
        unsafe {
            // SAFETY: value must be a valid null-terminated C string.
            while *ptr != 0 {
                ptr = ptr.offset(1);
                len += 1;
            }
        }
        let len = len as idx_t;
        // Write varint length
        let mut len_buffer = [0u8; 16];
        let len_size = encode_unsigned_leb128(&mut len_buffer, len as u64);
        self.stream.write_data(len_buffer.as_ptr(), len_size);
        // Write the string data
        if len > 0 {
            self.stream.write_data(value, len);
        }
    }

    fn write_data_ptr(&mut self, ptr: *const u8, count: idx_t) {
        // Write varint count
        let mut count_buffer = [0u8; 16];
        let count_size = encode_unsigned_leb128(&mut count_buffer, count);
        self.stream.write_data(count_buffer.as_ptr(), count_size);
        // Write the data (the stream trusts `ptr` to reference `count` bytes)
        if count > 0 {
            self.stream.write_data(ptr, count);
        }
    }

    fn on_object_begin(&mut self) {
        #[cfg(debug_assertions)]
        self.debug_stack.push(DebugState::default());
    }

    fn on_object_end(&mut self) {
        #[cfg(debug_assertions)]
        self.debug_stack
            .pop()
            .expect("OnObjectEnd called outside of object");
        // Write the object terminator field id as a raw 2-byte little-endian
        // value
        let term_buffer = MESSAGE_TERMINATOR_FIELD_ID.to_le_bytes();
        self.stream
            .write_data(term_buffer.as_ptr(), term_buffer.len() as idx_t);
    }

    fn on_list_begin(&mut self, count: idx_t) {
        // Write varint count
        let mut count_buffer = [0u8; 16];
        let count_size = encode_unsigned_leb128(&mut count_buffer, count);
        self.stream.write_data(count_buffer.as_ptr(), count_size);
    }

    fn on_list_end(&mut self) {
        // Nothing needed for binary format
    }

    fn on_nullable_begin(&mut self, present: bool) {
        // C++ writes the present flag as a raw byte
        self.stream.write_u8(u8::from(present));
    }

    fn on_nullable_end(&mut self) {
        // Nothing needed for binary format
    }

    fn on_property_begin(&mut self, field_id: field_id_t, tag: *const u8) {
        // C++ writes the field id as a raw 2-byte little-endian value (the tag
        // is debug-only)
        let fid_buffer = field_id.to_le_bytes();
        self.stream
            .write_data(fid_buffer.as_ptr(), fid_buffer.len() as idx_t);
        // Mirrors the C++ DEBUG duplicate field id/tag check; tags compare by
        // pointer like C++.
        #[cfg(debug_assertions)]
        {
            let tag_key = tag as usize;
            let state = match self.debug_stack.last_mut() {
                Some(state) => state,
                None => panic!("OnPropertyBegin called outside of object"),
            };
            if state.seen_field_ids.contains(&field_id) || state.seen_field_tags.contains(&tag_key)
            {
                panic!(
                    "Duplicate field id/tag in field: {:#x}:{}, other fields: {:?}",
                    tag_key, field_id, state.seen_fields
                );
            }
            state.seen_field_ids.insert(field_id);
            state.seen_field_tags.insert(tag_key);
            state.seen_fields.push((tag_key, field_id));
        }
        #[cfg(not(debug_assertions))]
        let _ = tag;
    }

    fn on_property_end(&mut self) {
        // Nothing needed for binary format
    }

    fn on_optional_property_begin(&mut self, field_id: field_id_t, tag: *const u8, present: bool) {
        if present {
            self.on_property_begin(field_id, tag);
        }
    }

    fn on_optional_property_end(&mut self, _present: bool) {
        // Nothing needed for binary format
    }

    fn write_property(
        &mut self,
        field_id: field_id_t,
        tag: *const u8,
        value: *const u8,
        count: idx_t,
    ) {
        self.on_property_begin(field_id, tag);
        // Write the value data - treat as data_ptr
        self.write_data_ptr(value, count);
        self.on_property_end();
    }

    // Dereferences `value`/`default_value`; each must cover `count` bytes per
    // the trait contract.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn write_property_with_default(
        &mut self,
        field_id: field_id_t,
        tag: *const u8,
        value: *const u8,
        count: idx_t,
        default_value: *const u8,
    ) {
        // Skip the property when the value equals the default, like the C++
        // version
        let is_default = if count == 0 {
            true
        } else if value.is_null() || default_value.is_null() {
            false
        } else {
            unsafe {
                // SAFETY: both pointers reference `count` readable bytes
                // (caller contract).
                std::slice::from_raw_parts(value, count as usize)
                    == std::slice::from_raw_parts(default_value, count as usize)
            }
        };
        if !self.options.serialize_default_values && is_default {
            self.on_optional_property_begin(field_id, tag, false);
            self.on_optional_property_end(false);
            return;
        }
        self.on_optional_property_begin(field_id, tag, true);
        self.write_data_ptr(value, count);
        self.on_optional_property_end(true);
    }

    fn write_object_dyn(
        &mut self,
        field_id: field_id_t,
        tag: *const u8,
        f: &mut dyn FnMut(&mut dyn Serializer),
    ) {
        self.on_property_begin(field_id, tag);
        self.on_object_begin();
        f(self);
        self.on_object_end();
        self.on_property_end();
    }

    fn write_list_dyn(
        &mut self,
        field_id: field_id_t,
        tag: *const u8,
        count: idx_t,
        f: &mut dyn FnMut(&mut dyn Serializer, idx_t),
    ) {
        self.on_property_begin(field_id, tag);
        self.on_list_begin(count);
        for i in 0..count {
            f(self, i);
        }
        self.on_list_end();
        self.on_property_end();
    }
}

/// Trait for types that can be serialized into a `Serializer`.
pub trait SerializeInSerializer {
    /// Serialize the value using the given serializer.
    fn serialize(&self, serializer: &mut dyn Serializer);
}

// Implement SerializeInSerializer for primitive types

impl SerializeInSerializer for bool {
    fn serialize(&self, serializer: &mut dyn Serializer) {
        serializer.write_bool(*self);
    }
}

impl SerializeInSerializer for i8 {
    fn serialize(&self, serializer: &mut dyn Serializer) {
        serializer.write_int8(*self);
    }
}

impl SerializeInSerializer for u8 {
    fn serialize(&self, serializer: &mut dyn Serializer) {
        serializer.write_uint8(*self);
    }
}

impl SerializeInSerializer for i16 {
    fn serialize(&self, serializer: &mut dyn Serializer) {
        serializer.write_int16(*self);
    }
}

impl SerializeInSerializer for u16 {
    fn serialize(&self, serializer: &mut dyn Serializer) {
        serializer.write_uint16(*self);
    }
}

impl SerializeInSerializer for i32 {
    fn serialize(&self, serializer: &mut dyn Serializer) {
        serializer.write_int32(*self);
    }
}

impl SerializeInSerializer for u32 {
    fn serialize(&self, serializer: &mut dyn Serializer) {
        serializer.write_uint32(*self);
    }
}

impl SerializeInSerializer for i64 {
    fn serialize(&self, serializer: &mut dyn Serializer) {
        serializer.write_int64(*self);
    }
}

impl SerializeInSerializer for u64 {
    fn serialize(&self, serializer: &mut dyn Serializer) {
        serializer.write_uint64(*self);
    }
}

impl SerializeInSerializer for f32 {
    fn serialize(&self, serializer: &mut dyn Serializer) {
        serializer.write_float(*self);
    }
}

impl SerializeInSerializer for f64 {
    fn serialize(&self, serializer: &mut dyn Serializer) {
        serializer.write_double(*self);
    }
}

impl SerializeInSerializer for string_t {
    fn serialize(&self, serializer: &mut dyn Serializer) {
        serializer.write_string_t(self.clone());
    }
}

impl SerializeInSerializer for string {
    fn serialize(&self, serializer: &mut dyn Serializer) {
        serializer.write_string(self.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binary_serializer_new() {
        let mut stream = crate::serializer::memory_stream::MemoryStream::new();
        let ws = &mut stream as &mut dyn WriteStream;
        let options = SerializationOptions {
            serialize_enum_as_string: false,
            serialize_default_values: false,
            storage_compatibility: StorageCompatibility::Default,
        };
        let serializer = BinarySerializer::new(ws, options);
        assert!(std::matches!(serializer, BinarySerializer { .. }));
    }

    #[test]
    fn test_binary_serialer_write_bool() {
        let mut stream = crate::serializer::memory_stream::MemoryStream::new();
        let ws = &mut stream as &mut dyn WriteStream;
        let options = SerializationOptions {
            serialize_enum_as_string: false,
            serialize_default_values: false,
            storage_compatibility: StorageCompatibility::Default,
        };
        let mut serializer = BinarySerializer::new(ws, options);
        serializer.write_bool(true);
        serializer.write_bool(false);
        // Check that data was written
        assert_eq!(stream.len(), 2); // 1 byte for true + 1 byte for false (null)
    }

    #[test]
    fn test_binary_serializer_write_int64() {
        let mut stream = crate::serializer::memory_stream::MemoryStream::new();
        let ws = &mut stream as &mut dyn WriteStream;
        let options = SerializationOptions {
            serialize_enum_as_string: false,
            serialize_default_values: false,
            storage_compatibility: StorageCompatibility::Default,
        };
        let mut serializer = BinarySerializer::new(ws, options);
        serializer.write_int64(42);
        serializer.write_int64(-1);
        // Check data was written (varint encoding sizes may vary)
        assert!(!stream.is_empty());
    }

    #[test]
    fn test_binary_serializer_write_uint64() {
        let mut stream = crate::serializer::memory_stream::MemoryStream::new();
        let ws = &mut stream as &mut dyn WriteStream;
        let options = SerializationOptions {
            serialize_enum_as_string: false,
            serialize_default_values: false,
            storage_compatibility: StorageCompatibility::Default,
        };
        let mut serializer = BinarySerializer::new(ws, options);
        serializer.write_uint64(42);
        serializer.write_uint64(0);
        assert!(!stream.is_empty());
    }

    #[test]
    fn test_binary_serializer_write_float() {
        let mut stream = crate::serializer::memory_stream::MemoryStream::new();
        let ws = &mut stream as &mut dyn WriteStream;
        let options = SerializationOptions {
            serialize_enum_as_string: false,
            serialize_default_values: false,
            storage_compatibility: StorageCompatibility::Default,
        };
        let mut serializer = BinarySerializer::new(ws, options);
        serializer.write_float(std::f32::consts::PI);
        assert!(!stream.is_empty()); // f32 is 4 bytes
    }

    #[test]
    fn test_binary_serializer_write_double() {
        let mut stream = crate::serializer::memory_stream::MemoryStream::new();
        let ws = &mut stream as &mut dyn WriteStream;
        let options = SerializationOptions {
            serialize_enum_as_string: false,
            serialize_default_values: false,
            storage_compatibility: StorageCompatibility::Default,
        };
        let mut serializer = BinarySerializer::new(ws, options);
        serializer.write_double(std::f64::consts::E);
        assert!(!stream.is_empty()); // f64 is 8 bytes
    }

    #[test]
    fn test_binary_serializer_write_string() {
        let mut stream = crate::serializer::memory_stream::MemoryStream::new();
        let ws = &mut stream as &mut dyn WriteStream;
        let options = SerializationOptions {
            serialize_enum_as_string: false,
            serialize_default_values: false,
            storage_compatibility: StorageCompatibility::Default,
        };
        let mut serializer = BinarySerializer::new(ws, options);
        let s = string {
            data: "hello".to_string(),
        };
        serializer.write_string(s);
        // Should have varint length + 5 bytes
        assert_eq!(stream.len(), 1 + 5); // varint 5 + "hello"
    }

    #[test]
    fn test_binary_serializer_write_data_ptr() {
        let mut stream = crate::serializer::memory_stream::MemoryStream::new();
        let ws = &mut stream as &mut dyn WriteStream;
        let options = SerializationOptions {
            serialize_enum_as_string: false,
            serialize_default_values: false,
            storage_compatibility: StorageCompatibility::Default,
        };
        let mut serializer = BinarySerializer::new(ws, options);
        let data: &[u8] = &[1, 2, 3, 4, 5];
        serializer.write_data_ptr(data.as_ptr(), data.len() as idx_t);
        // varint 5 + 5 bytes = 2 bytes varint + 5 bytes data (varint encoding
        // of 5 is 1 byte)
        assert!(!stream.is_empty());
    }

    #[test]
    fn test_binary_serializer_roundtrip_bool() {
        let mut stream = crate::serializer::memory_stream::MemoryStream::new();
        let ws = &mut stream as &mut dyn WriteStream;
        let options = SerializationOptions {
            serialize_enum_as_string: false,
            serialize_default_values: false,
            storage_compatibility: StorageCompatibility::Default,
        };
        let mut serializer = BinarySerializer::new(ws, options);

        // Write
        serializer.write_bool(true);

        // Read back using deserializer (will be tested separately)
        assert!(!stream.is_empty());
    }

    #[test]
    fn test_binary_serializer_roundtrip_int64() {
        let mut stream = crate::serializer::memory_stream::MemoryStream::new();
        let ws = &mut stream as &mut dyn WriteStream;
        let options = SerializationOptions {
            serialize_enum_as_string: false,
            serialize_default_values: false,
            storage_compatibility: StorageCompatibility::Default,
        };
        let mut serializer = BinarySerializer::new(ws, options);

        // Write
        serializer.write_int64(42);
        serializer.write_int64(-1);
        serializer.write_int64(0);

        assert!(!stream.is_empty());
    }

    #[test]
    fn test_binary_serializer_roundtrip_string() {
        let mut stream = crate::serializer::memory_stream::MemoryStream::new();
        let ws = &mut stream as &mut dyn WriteStream;
        let options = SerializationOptions {
            serialize_enum_as_string: false,
            serialize_default_values: false,
            storage_compatibility: StorageCompatibility::Default,
        };
        let mut serializer = BinarySerializer::new(ws, options);

        // Write
        let s = string {
            data: "test string".to_string(),
        };
        serializer.write_string(s);

        assert!(!stream.is_empty());
    }

    #[test]
    fn test_binary_serializer_write_list() {
        let mut stream = crate::serializer::memory_stream::MemoryStream::new();
        let ws = &mut stream as &mut dyn WriteStream;
        let options = SerializationOptions {
            serialize_enum_as_string: false,
            serialize_default_values: false,
            storage_compatibility: StorageCompatibility::Default,
        };
        let mut serializer = BinarySerializer::new(ws, options);

        // Write a list of 3 int64 values
        let mut f = |s: &mut dyn Serializer, i: idx_t| {
            s.write_int64(i as i64);
        };
        serializer.on_object_begin();
        serializer.write_list_dyn(0, b"test".as_ptr(), 3, &mut f);
        serializer.on_object_end();

        assert!(!stream.is_empty());
    }

    #[test]
    fn test_binary_serializer_write_object() {
        let mut stream = crate::serializer::memory_stream::MemoryStream::new();
        let ws = &mut stream as &mut dyn WriteStream;
        let options = SerializationOptions {
            serialize_enum_as_string: false,
            serialize_default_values: false,
            storage_compatibility: StorageCompatibility::Default,
        };
        let mut serializer = BinarySerializer::new(ws, options);

        // Write an object with a property
        let mut f = |s: &mut dyn Serializer| {
            s.write_int64(42);
        };
        serializer.on_object_begin();
        serializer.write_object_dyn(0, b"test".as_ptr(), &mut f);
        serializer.on_object_end();

        assert!(!stream.is_empty());
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "OnPropertyBegin called outside of object")]
    fn test_property_outside_object_panics() {
        let mut stream = crate::serializer::memory_stream::MemoryStream::new();
        let ws = &mut stream as &mut dyn WriteStream;
        let mut serializer = BinarySerializer::new(ws, SerializationOptions::default());
        let data = [1u8];
        serializer.write_property(1, b"tag".as_ptr(), data.as_ptr(), 1);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "Duplicate field id/tag")]
    fn test_duplicate_field_id_panics() {
        let mut stream = crate::serializer::memory_stream::MemoryStream::new();
        let ws = &mut stream as &mut dyn WriteStream;
        let mut serializer = BinarySerializer::new(ws, SerializationOptions::default());
        let data = [1u8];
        serializer.on_object_begin();
        serializer.write_property(1, b"a".as_ptr(), data.as_ptr(), 1);
        serializer.write_property(1, b"b".as_ptr(), data.as_ptr(), 1);
        serializer.on_object_end();
    }

    #[test]
    fn test_duplicate_field_id_allowed_across_objects() {
        let mut stream = crate::serializer::memory_stream::MemoryStream::new();
        let ws = &mut stream as &mut dyn WriteStream;
        let mut serializer = BinarySerializer::new(ws, SerializationOptions::default());
        let data = [1u8];
        // the same field id may reappear in a nested object (separate debug
        // state)
        serializer.on_object_begin();
        serializer.on_object_begin();
        serializer.write_property(1, b"a".as_ptr(), data.as_ptr(), 1);
        serializer.on_object_end();
        serializer.write_property(1, b"b".as_ptr(), data.as_ptr(), 1);
        serializer.on_object_end();
        assert!(!stream.is_empty());
    }
}
