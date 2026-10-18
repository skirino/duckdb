//! List and struct support.
//!
//! Ported from `duckdb/common/serializer/serializer.hpp` and
//! `duckdb/common/serializer/deserializer.hpp`.
//!
//! The `List` helpers provide iterator/method-based API for
//! serializing/deserializing lists and structs in the binary format.

use crate::serializer::{
    DeserializeInDeserializer,
    SerializeInSerializer,
    deserializer::Deserializer,
    field_id_t,
    idx_t,
    serializer::Serializer,
};

/// A helper for serializing list elements.
///
/// This is the Rust equivalent of the C++ `Serializer::List` nested class.
/// It provides methods for writing list elements during serialization.
pub struct ListSerializer<'a> {
    /// The serializer to write through.
    serializer: &'a mut dyn Serializer,
    /// Current index within the list.
    idx: idx_t,
    /// Total count of elements in the list (set by OnListBegin).
    count: idx_t,
}

impl<'a> ListSerializer<'a> {
    /// Create a new ListSerializer.
    pub fn new(serializer: &'a mut dyn Serializer, count: idx_t) -> Self {
        ListSerializer { serializer, idx: 0, count }
    }

    /// Write the current element.
    ///
    /// This is equivalent to the C++ `List::WriteElement` method.
    pub fn write_element<T: crate::serializer::SerializeInSerializer>(&mut self, value: &T) {
        let serializer = &mut *self.serializer;
        value.serialize(serializer);
        self.idx += 1;
    }

    /// Write raw data bytes.
    ///
    /// This is equivalent to the C++ `List::WriteElement(data_ptr_t, idx_t)`
    /// method.
    pub fn write_data(&mut self, ptr: *const u8, size: idx_t) {
        let serializer = &mut *self.serializer;
        serializer.write_data_ptr(ptr, size);
        self.idx += 1;
    }

    /// Advance to the next element index.
    pub fn next_idx(&mut self) -> idx_t {
        let current = self.idx;
        self.idx += 1;
        current
    }

    /// Get the current index.
    pub fn get_idx(&self) -> idx_t { self.idx }

    /// Check if we've reached the end of the list.
    pub fn is_done(&self) -> bool { self.idx >= self.count }
}

/// A helper for deserializing list elements.
///
/// This is the Rust equivalent of the C++ `Deserializer::List` nested class.
/// It provides methods for reading list elements during deserialization.
pub struct ListDeserializer<'a> {
    /// The deserializer to read from.
    deserializer: &'a mut dyn Deserializer,
    /// Current index within the list.
    idx: idx_t,
    /// Total count of elements in the list (set by OnListBegin).
    count: idx_t,
}

impl<'a> ListDeserializer<'a> {
    /// Create a new ListDeserializer.
    pub fn new(deserializer: &'a mut dyn Deserializer, count: idx_t) -> Self {
        ListDeserializer { deserializer, idx: 0, count }
    }

    /// Read the current element.
    ///
    /// This is equivalent to the C++ `List::ReadElement` method.
    pub fn read_element<T: crate::serializer::DeserializeInDeserializer>(&mut self) -> T {
        let deserializer = &mut *self.deserializer;
        let value = T::deserialize(deserializer);
        self.idx += 1;
        value
    }

    /// Read raw data bytes.
    ///
    /// This is equivalent to the C++ `List::ReadElement(data_ptr_t &, idx_t)`
    /// method.
    ///
    /// Reads `count` bytes from the deserializer and returns them as an owned
    /// `Vec<u8>`. The caller is responsible for copying the data if a raw
    /// pointer is needed.
    pub fn read_data(&mut self, count: idx_t) -> Vec<u8> {
        let deserializer = &mut *self.deserializer;
        deserializer.read_data_ptr(count)
    }

    /// Advance to the next element index.
    pub fn next_idx(&mut self) -> idx_t {
        let current = self.idx;
        self.idx += 1;
        current
    }

    /// Get the current index.
    pub fn get_idx(&self) -> idx_t { self.idx }

    /// Check if we've reached the end of the list.
    pub fn is_done(&self) -> bool { self.idx >= self.count }
}

/// Serialize a list of elements.
///
/// This is the Rust equivalent of the C++ `Serializer::WriteList` method.
///
/// # Arguments
///
/// * `serializer` - The serializer to write to
/// * `field_id` - The field id (0 for no property)
/// * `tag` - The tag (null for no tag)
/// * `count` - The number of elements
/// * `func` - A closure called for each element, receiving the serializer and
///   index
pub fn write_list<F>(
    serializer: &mut dyn Serializer,
    field_id: field_id_t,
    tag: *const u8,
    count: idx_t,
    func: F,
) where
    F: FnMut(&mut dyn Serializer, idx_t),
{
    serializer.on_property_begin(field_id, tag);
    serializer.on_list_begin(count);
    let mut func = func;
    for i in 0..count
    {
        func(serializer, i);
    }
    serializer.on_list_end();
    serializer.on_property_end();
}

/// Deserialize a list of elements.
///
/// This is the Rust equivalent of the C++ `Deserializer::ReadList` method.
///
/// # Arguments
///
/// * `deserializer` - The deserializer to read from
/// * `field_id` - The field id (0 for no property)
/// * `tag` - The tag (null for no tag)
/// * `func` - A closure called for each element, receiving the deserializer and
///   index
pub fn read_list<F>(
    deserializer: &mut dyn Deserializer,
    field_id: field_id_t,
    tag: *const u8,
    func: F,
) where
    F: FnMut(&mut dyn Deserializer, idx_t),
{
    deserializer.on_property_begin(field_id, tag);
    let count = deserializer.on_list_begin();
    let mut func = func;
    for i in 0..count
    {
        func(deserializer, i);
    }
    deserializer.on_list_end();
    deserializer.on_property_end();
}

/// Serialize a struct using the `Serialize` trait.
///
/// This is the Rust equivalent of the C++ `Serializer::WriteValue`
/// specialization for types implementing `Serialize(Serializer&)`.
///
/// # Arguments
///
/// * `serializer` - The serializer to write to
/// * `value` - The struct value to serialize
pub fn serialize_struct<T: SerializeInSerializer>(serializer: &mut dyn Serializer, value: &T) {
    value.serialize(serializer);
}

/// Deserialize a struct using the `Deserialize` trait.
///
/// This is the Rust equivalent of the C++ `Deserializer::ReadValue`
/// specialization for types implementing `Deserialize(Deserializer&)`.
///
/// # Arguments
///
/// * `deserializer` - The deserializer to read from
/// * `returns` The deserialized struct value
pub fn deserialize_struct<T: DeserializeInDeserializer>(deserializer: &mut dyn Deserializer) -> T {
    T::deserialize(deserializer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serializer::{
        BinaryDeserializer,
        BinarySerializer,
        SerializationData,
        SerializationOptions,
        hugeint_t,
        memory_stream::MemoryStream,
        string,
        string_t,
        uhugeint_t,
    };

    #[derive(Debug)]
    struct TestSerializer {
        options: SerializationOptions,
        data: SerializationData,
        written: Vec<i64>,
    }

    impl TestSerializer {
        fn new() -> Self {
            TestSerializer {
                options: SerializationOptions::default(),
                data: SerializationData::default(),
                written: Vec::new(),
            }
        }
    }

    impl Serializer for TestSerializer {
        fn get_options(&self) -> &SerializationOptions { &self.options }

        fn get_serialization_data(&self) -> &SerializationData { &self.data }

        fn set_serialization_data(&mut self, data: &SerializationData) { self.data = data.clone(); }

        fn write_null(&mut self) {}

        fn write_bool(&mut self, _: bool) {}

        fn write_char(&mut self, _: char) {}

        fn write_int8(&mut self, _: i8) {}

        fn write_uint8(&mut self, _: u8) {}

        fn write_int16(&mut self, _: i16) {}

        fn write_uint16(&mut self, _: u16) {}

        fn write_int32(&mut self, _: i32) {}

        fn write_uint32(&mut self, _: u32) {}

        fn write_int64(&mut self, value: i64) { self.written.push(value); }

        fn write_uint64(&mut self, _: u64) {}

        fn write_hugeint(&mut self, _: hugeint_t) {}

        fn write_uhugeint(&mut self, _: uhugeint_t) {}

        fn write_float(&mut self, _: f32) {}

        fn write_double(&mut self, _: f64) {}

        fn write_string_t(&mut self, _: string_t) {}

        fn write_string(&mut self, _: string) {}

        fn write_cstr(&mut self, _: *const u8) {}

        fn write_data_ptr(&mut self, _: *const u8, _: idx_t) {}

        fn on_object_begin(&mut self) {}

        fn on_object_end(&mut self) {}

        fn on_list_begin(&mut self, _: idx_t) {}

        fn on_list_end(&mut self) {}

        fn on_nullable_begin(&mut self, _: bool) {}

        fn on_nullable_end(&mut self) {}

        fn on_property_begin(&mut self, _: field_id_t, _: *const u8) {}

        fn on_property_end(&mut self) {}

        fn on_optional_property_begin(&mut self, _: field_id_t, _: *const u8, _: bool) {}

        fn on_optional_property_end(&mut self, _: bool) {}

        fn write_property(&mut self, _: field_id_t, _: *const u8, _: *const u8, _: idx_t) {}

        fn write_property_with_default(
            &mut self,
            _: field_id_t,
            _: *const u8,
            _: *const u8,
            _: idx_t,
            _: *const u8,
        ) {
        }

        fn write_object_dyn(
            &mut self,
            _: field_id_t,
            _: *const u8,
            _: &mut dyn FnMut(&mut dyn Serializer),
        ) {
        }

        fn write_list_dyn(
            &mut self,
            _: field_id_t,
            _: *const u8,
            _: idx_t,
            _: &mut dyn FnMut(&mut dyn Serializer, idx_t),
        ) {
        }
    }

    #[derive(Debug)]
    struct TestDeserializer {
        data: SerializationData,
        list_count: idx_t,
        read_values: Vec<i64>,
    }

    impl TestDeserializer {
        fn new(list_count: idx_t, read_values: Vec<i64>) -> Self {
            TestDeserializer { data: SerializationData::default(), list_count, read_values }
        }
    }

    impl Deserializer for TestDeserializer {
        fn get_serialization_data(&self) -> &SerializationData { &self.data }

        fn set_serialization_data(&mut self, data: &SerializationData) { self.data = data.clone(); }

        fn read_bool(&mut self) -> bool { false }
        fn read_char(&mut self) -> char { 'a' }
        fn read_int8(&mut self) -> i8 { 0 }
        fn read_uint8(&mut self) -> u8 { 0 }
        fn read_int16(&mut self) -> i16 { 0 }
        fn read_uint16(&mut self) -> u16 { 0 }
        fn read_int32(&mut self) -> i32 { 0 }
        fn read_uint32(&mut self) -> u32 { 0 }
        fn read_int64(&mut self) -> i64 {
            if self.read_values.is_empty()
            {
                0
            }
            else
            {
                self.read_values.remove(0)
            }
        }
        fn read_uint64(&mut self) -> u64 { 0 }
        fn read_hugeint(&mut self) -> hugeint_t { hugeint_t { upper: 0, lower: 0 } }
        fn read_uhugeint(&mut self) -> uhugeint_t { uhugeint_t { upper: 0, lower: 0 } }
        fn read_float(&mut self) -> f32 { 0.0 }
        fn read_double(&mut self) -> f64 { 0.0 }
        fn read_string_t(&mut self) -> string_t { string_t { data: vec![], size: 0 } }
        fn read_string(&mut self) -> string { string { data: String::new() } }
        fn read_data_ptr(&mut self, _: idx_t) -> Vec<u8> { Vec::new() }
        fn on_object_begin(&mut self) {}
        fn on_object_end(&mut self) {}
        fn on_list_begin(&mut self) -> idx_t { self.list_count }
        fn on_list_end(&mut self) {}
        fn on_nullable_begin(&mut self) -> bool { false }
        fn on_nullable_end(&mut self) {}
        fn on_property_begin(&mut self, _: field_id_t, _: *const u8) -> bool { true }
        fn on_property_end(&mut self) {}
        fn on_optional_property_begin(&mut self, _: field_id_t, _: *const u8) -> bool { false }
        fn on_optional_property_end(&mut self, _: bool) {}
        fn read_property(&mut self, _: field_id_t, _: *const u8, _: *mut u8, _: idx_t) -> bool {
            false
        }
        fn read_property_with_default(
            &mut self,
            _: field_id_t,
            _: *const u8,
            _: *mut u8,
            _: idx_t,
            _: *const u8,
        ) -> bool {
            false
        }
        fn read_object_dyn(
            &mut self,
            _: field_id_t,
            _: *const u8,
            _: &mut dyn FnMut(&mut dyn Deserializer),
        ) {
        }
        fn read_list_dyn(
            &mut self,
            _: field_id_t,
            _: *const u8,
            _: &mut dyn FnMut(&mut dyn Deserializer, idx_t),
        ) {
        }
    }

    #[test]
    fn test_list_serializer_new() {
        let mut s = TestSerializer::new();
        {
            let mut ls = ListSerializer::new(&mut s, 3);
            assert!(!ls.is_done());
            assert_eq!(ls.get_idx(), 0);
            ls.write_element(&1i64);
            assert_eq!(ls.get_idx(), 1);
            let data = [1u8, 2];
            ls.write_data(data.as_ptr(), 2);
            assert_eq!(ls.get_idx(), 2);
            assert_eq!(ls.next_idx(), 2);
            assert_eq!(ls.get_idx(), 3);
            assert!(ls.is_done());
        }
        assert_eq!(s.written, vec![1]);
    }

    #[test]
    fn test_list_deserializer_new() {
        let mut d = TestDeserializer::new(3, vec![11, 22, 33]);
        {
            let mut ld = ListDeserializer::new(&mut d, 3);
            assert!(!ld.is_done());
            assert_eq!(ld.get_idx(), 0);
            let value: i64 = ld.read_element();
            assert_eq!(value, 11);
            assert_eq!(ld.get_idx(), 1);
            assert_eq!(ld.next_idx(), 1);
        }
        assert_eq!(d.read_values, vec![22, 33]);
    }

    #[test]
    fn test_write_list_func() {
        let mut s = TestSerializer::new();
        write_list(&mut s, 0, std::ptr::null(), 3, |s, i| {
            s.write_int64(i as i64);
        });
        assert_eq!(s.written, vec![0, 1, 2]);
    }

    #[test]
    fn test_read_list_func() {
        let mut d = TestDeserializer::new(2, vec![11, 22]);
        let mut seen = Vec::new();
        read_list(&mut d, 0, std::ptr::null(), |d, i| {
            assert_eq!(i, seen.len() as idx_t);
            seen.push(d.read_int64());
        });
        assert_eq!(seen, vec![11, 22]);
    }

    #[test]
    fn test_list_roundtrip() {
        let mut stream = MemoryStream::new();
        {
            let mut s = BinarySerializer::new(&mut stream, SerializationOptions::default());
            s.on_object_begin();
            write_list(&mut s, 9, b"vals".as_ptr(), 3, |s, i| {
                s.write_int64(i as i64 * 10);
            });
            s.on_object_end();
        }
        stream.rewind();
        let mut d = BinaryDeserializer::new(&mut stream);
        d.on_object_begin();
        let mut values = Vec::new();
        read_list(&mut d, 9, b"vals".as_ptr(), |d, _i| {
            values.push(d.read_int64());
        });
        d.on_object_end();
        assert_eq!(values, vec![0, 10, 20]);
    }

    #[test]
    fn test_serialize_deserialize_struct() {
        let mut stream = MemoryStream::new();
        {
            let mut s = BinarySerializer::new(&mut stream, SerializationOptions::default());
            serialize_struct(&mut s, &(-42i64));
        }
        stream.rewind();
        let mut d = BinaryDeserializer::new(&mut stream);
        let value: i64 = deserialize_struct(&mut d);
        assert_eq!(value, -42);
    }

    #[test]
    fn test_list_element_data_roundtrip() {
        let mut stream = MemoryStream::new();
        {
            let mut s = BinarySerializer::new(&mut stream, SerializationOptions::default());
            let data = [7u8, 8, 9];
            s.write_data_ptr(data.as_ptr(), 3);
        }
        stream.rewind();
        let mut d = BinaryDeserializer::new(&mut stream);
        let mut ld = ListDeserializer::new(&mut d, 1);
        assert_eq!(ld.read_data(3), vec![7, 8, 9]);
    }
}
