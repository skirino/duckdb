//! Serializer module - Rust migration of DuckDB C++ serializer/deserializer.
//!
//! This module provides Rust traits and structs for serialization,
//! migrating from the C++ `src/common/serializer` code.
//!
//! # Usage
//!
//! ```
//! use ahirudb::serializer::{
//!     BinaryDeserializer,
//!     BinarySerializer,
//!     Deserializer,
//!     MemoryStream,
//!     SerializationOptions,
//!     Serializer,
//! };
//!
//! // Create a memory stream and a serializer/deserializer pair
//! let mut stream = MemoryStream::new();
//! let mut serializer = BinarySerializer::new(&mut stream, SerializationOptions::default());
//!
//! // Write a value
//! serializer.write_int64(42);
//! drop(serializer);
//!
//! // Read the value back
//! stream.rewind();
//! let mut deserializer = BinaryDeserializer::new(&mut stream);
//! assert_eq!(deserializer.read_int64(), 42);
//! ```

// C++ type names (field_id_t, idx_t, ...) are kept for parity with the original code.
#![allow(non_camel_case_types)]

/// The type used for field IDs in serialization.
pub type field_id_t = u16;

/// The type used for sizes and counts in serialization.
pub type idx_t = u64;

/// Marker for the end of a serialized message.
pub const MESSAGE_TERMINATOR_FIELD_ID: field_id_t = 0xFFFF;

/// A type representing a huge integer (int64 upper + uint64 lower, like C++
/// `hugeint_t`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct hugeint_t {
    pub upper: i64,
    pub lower: u64,
}

/// A type representing an unsigned huge integer (two uint64 values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct uhugeint_t {
    pub upper: u64,
    pub lower: u64,
}

/// A string type used in serialization, similar to DuckDB's string_t.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct string_t {
    pub data: Vec<u8>,
    pub size: usize,
}

impl string_t {
    pub fn get_data(&self) -> &[u8] { &self.data[..self.size] }

    pub fn get_size(&self) -> usize { self.size }
}

/// A string type used in serialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct string {
    pub data: String,
}

impl string {
    pub fn length(&self) -> usize { self.data.len() }

    pub fn get_data(&self) -> *const u8 { self.data.as_ptr() }
}

pub mod cpp_bridge;
pub mod encoding_util;
pub mod write_stream;
pub mod read_stream;
pub mod memory_stream;
pub mod serialization_data;
// The inner module mirrors the C++ `serializer.hpp` file name.
#[allow(clippy::module_inception)]
pub mod serializer;
pub mod deserializer;
pub mod list;

pub use cpp_bridge::{
    CppBinaryDeserializer,
    CppBinarySerializer,
    clear_pending_error,
    has_pending_error,
    pending_error_len,
    pending_error_ptr,
};
pub use deserializer::{
    BinaryDeserializer,
    DeserializeInDeserializer,
    Deserializer,
};
pub use encoding_util::{
    decode_signed_leb128,
    decode_unsigned_leb128,
    encode_signed_leb128,
    encode_unsigned_leb128,
};
pub use list::{
    ListDeserializer,
    ListSerializer,
    deserialize_struct,
    read_list,
    serialize_struct,
    write_list,
};
pub use memory_stream::MemoryStream;
pub use read_stream::ReadStream;
pub use serialization_data::SerializationData;
pub use serializer::{
    BinarySerializer,
    SerializationOptions,
    SerializeInSerializer,
    Serializer,
    StorageCompatibility,
};
pub use write_stream::WriteStream;
