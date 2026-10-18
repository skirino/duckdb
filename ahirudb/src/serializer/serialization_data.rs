//! SerializationData.
//!
//! Ported from `duckdb/common/serializer/serialization_data.hpp` and
//! `serialization_data.cpp`.
//!
//! Holds contextual data for serialization/deserialization, including stacks
//! of client contexts, databases, catalogs, enums, parameter data, types,
//! function children, compression info, and custom data.

use std::any::TypeId;

use crate::serializer::idx_t;

/// Holds contextual data for serialization/deserialization.
///
/// This is the Rust equivalent of DuckDB's `SerializationData` C++ class.
/// It provides stacks for various types of contextual information used during
/// serialization, including client contexts, databases, catalogs, enum states,
/// parameter data, logical types, function children, compression info, and
/// custom data per type.
#[derive(Debug, Clone)]
pub struct SerializationData {
    /// Stack of client contexts.
    contexts: Vec<ClientContextRef>,

    /// Stack of database instances.
    databases: Vec<DatabaseInstanceRef>,

    /// Stack of catalog entries.
    catalogs: Vec<CatalogRef>,

    /// Stack of enum values (stored as u64 indices).
    enums: Vec<u64>,

    /// Stack of bound parameter map references.
    parameter_data: Vec<ParameterDataRef>,

    /// Stack of LogicalType references.
    types: Vec<LogicalTypeRef>,

    /// Stack of const_expression_list_t references.
    function_children: Vec<FunctionChildrenRef>,

    /// Stack of CompressionInfo references.
    compression_infos: Vec<CompressionInfoRef>,

    /// Map of custom data stacks keyed by type.
    customs: std::collections::HashMap<TypeId, Vec<Box<dyn CustomData>>>,
}

/// A trait for custom data that can be stored in SerializationData's customs
/// map.
pub trait CustomData: std::fmt::Debug + 'static {
    /// Get the type name used as the key in the customs map.
    fn get_type(&self) -> &'static str;

    /// Clone the custom data.
    fn clone_box(&self) -> Box<dyn CustomData>;

    /// Get the type ID for runtime type checking.
    fn get_type_id(&self) -> TypeId;
}

impl Clone for Box<dyn CustomData> {
    fn clone(&self) -> Box<dyn CustomData> { self.clone_box() }
}

impl PartialEq for dyn CustomData {
    fn eq(&self, other: &dyn CustomData) -> bool {
        self.get_type_id() == other.get_type_id() && self.get_type() == other.get_type()
    }
}

impl Eq for dyn CustomData {}

/// A reference to a ClientContext.
type ClientContextRef = *mut std::ffi::c_void;

/// A reference to a DatabaseInstance.
type DatabaseInstanceRef = *mut std::ffi::c_void;

/// A reference to a Catalog.
type CatalogRef = *mut std::ffi::c_void;

/// A reference to a bound_parameter_map_t.
type ParameterDataRef = *mut std::ffi::c_void;

/// A reference to a LogicalType.
type LogicalTypeRef = *mut std::ffi::c_void;

/// A reference to a const_expression_list_t.
type FunctionChildrenRef = *mut std::ffi::c_void;

/// A reference to a CompressionInfo.
type CompressionInfoRef = *mut std::ffi::c_void;

/// Creates a new empty SerializationData.
impl Default for SerializationData {
    fn default() -> Self {
        Self {
            contexts: Vec::new(),
            databases: Vec::new(),
            catalogs: Vec::new(),
            enums: Vec::new(),
            parameter_data: Vec::new(),
            types: Vec::new(),
            function_children: Vec::new(),
            compression_infos: Vec::new(),
            customs: std::collections::HashMap::new(),
        }
    }
}

impl SerializationData {
    /// Push a client context onto the contexts stack.
    pub fn set_context(&mut self, context: *mut std::ffi::c_void) { self.contexts.push(context); }

    /// Get the top client context.
    pub fn get_context(&self) -> *mut std::ffi::c_void {
        debug_assert!(!self.contexts.is_empty(), "Context stack is empty");
        self.contexts[self.contexts.len() - 1]
    }

    /// Pop the top client context.
    pub fn unset_context(&mut self) { self.contexts.pop(); }

    /// Push a database instance onto the databases stack.
    pub fn set_database(&mut self, db: *mut std::ffi::c_void) { self.databases.push(db); }

    /// Get the top database instance.
    pub fn get_database(&self) -> *mut std::ffi::c_void {
        debug_assert!(!self.databases.is_empty(), "Database stack is empty");
        self.databases[self.databases.len() - 1]
    }

    /// Pop the top database instance.
    pub fn unset_database(&mut self) { self.databases.pop(); }

    /// Push a catalog onto the catalogs stack.
    pub fn set_catalog(&mut self, catalog: *mut std::ffi::c_void) { self.catalogs.push(catalog); }

    /// Get the top catalog.
    pub fn get_catalog(&self) -> *mut std::ffi::c_void {
        debug_assert!(!self.catalogs.is_empty(), "Catalog stack is empty");
        self.catalogs[self.catalogs.len() - 1]
    }

    /// Pop the top catalog.
    pub fn unset_catalog(&mut self) { self.catalogs.pop(); }

    /// Push an enum value onto the enums stack.
    pub fn set_enum(&mut self, value: idx_t) { self.enums.push(value); }

    /// Get the top enum value.
    pub fn get_enum(&self) -> idx_t {
        let val = self.enums[self.enums.len() - 1];
        val as idx_t
    }

    /// Pop the top enum value.
    pub fn unset_enum(&mut self) { self.enums.pop(); }

    /// Push a bound parameter map reference onto the parameter_data stack.
    pub fn set_parameter_data(&mut self, param_data: *mut std::ffi::c_void) {
        self.parameter_data.push(param_data);
    }

    /// Get the top bound parameter map reference.
    pub fn get_parameter_data(&self) -> *mut std::ffi::c_void {
        debug_assert!(!self.parameter_data.is_empty(), "Parameter data stack is empty");
        self.parameter_data[self.parameter_data.len() - 1]
    }

    /// Pop the top bound parameter map reference.
    pub fn unset_parameter_data(&mut self) { self.parameter_data.pop(); }

    /// Push a LogicalType reference onto the types stack.
    pub fn set_type(&mut self, logical_type: *mut std::ffi::c_void) {
        self.types.push(logical_type);
    }

    /// Get the top LogicalType reference.
    pub fn get_type(&self) -> *mut std::ffi::c_void {
        debug_assert!(!self.types.is_empty(), "LogicalType stack is empty");
        self.types[self.types.len() - 1]
    }

    /// Pop the top LogicalType reference.
    pub fn unset_type(&mut self) { self.types.pop(); }

    /// Push a function children reference onto the function_children stack.
    pub fn set_function_children(&mut self, children: *mut std::ffi::c_void) {
        self.function_children.push(children);
    }

    /// Get the top function children reference.
    pub fn get_function_children(&self) -> *mut std::ffi::c_void {
        debug_assert!(!self.function_children.is_empty(), "Function children stack is empty");
        self.function_children[self.function_children.len() - 1]
    }

    /// Pop the top function children reference.
    pub fn unset_function_children(&mut self) { self.function_children.pop(); }

    /// Push a CompressionInfo reference onto the compression_infos stack.
    pub fn set_compression_info(&mut self, compression_info: *mut std::ffi::c_void) {
        self.compression_infos.push(compression_info);
    }

    /// Get the top CompressionInfo reference.
    pub fn get_compression_info(&self) -> *mut std::ffi::c_void {
        debug_assert!(!self.compression_infos.is_empty(), "Compression info stack is empty");
        self.compression_infos[self.compression_infos.len() - 1]
    }

    /// Pop the top CompressionInfo reference.
    pub fn unset_compression_info(&mut self) { self.compression_infos.pop(); }

    /// Set a custom data entry.
    pub fn set_custom<T: CustomData + 'static>(&mut self, data: &T) {
        let entry = data.clone_box();
        self.customs
            .entry(TypeId::of::<T>())
            .or_default()
            .push(entry);
    }

    /// Get the top custom data entry of type T.
    pub fn get_custom<T: CustomData + 'static>(&self) -> &T {
        let customs = self
            .customs
            .get(&TypeId::of::<T>())
            .expect("no stack for custom data type");
        let top = customs.last().expect("empty stack for custom data");
        debug_assert_eq!(top.get_type_id(), TypeId::of::<T>(), "custom data type mismatch");
        unsafe {
            // SAFETY: the stack is keyed by TypeId::of::<T>() and the debug
            // assert re-checks the stored value's type, so `top` is
            // a valid `T`.
            &*(top.as_ref() as *const dyn CustomData as *const T)
        }
    }

    /// Check if a custom data entry of type T exists.
    pub fn has_custom<T: CustomData + 'static>(&self) -> bool {
        self.customs
            .get(&TypeId::of::<T>())
            .is_some_and(|c| !c.is_empty())
    }

    /// Unset (pop) the top custom data entry of type T.
    pub fn unset_custom<T: CustomData + 'static>(&mut self) {
        if let Some(customs) = self.customs.get_mut(&TypeId::of::<T>())
        {
            customs.pop();
        }
    }
}

/// Exception thrown when SerializationData operations fail.
pub struct SerializationDataError(String);

impl std::fmt::Display for SerializationDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) }
}

impl std::fmt::Debug for SerializationDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SerializationDataError: {}", self.0)
    }
}

impl std::error::Error for SerializationDataError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialization_data_default() {
        let data = SerializationData::default();
        assert!(data.contexts.is_empty());
        assert!(data.databases.is_empty());
        assert!(data.catalogs.is_empty());
        assert!(data.enums.is_empty());
        assert!(data.parameter_data.is_empty());
        assert!(data.types.is_empty());
        assert!(data.function_children.is_empty());
        assert!(data.compression_infos.is_empty());
        assert!(data.customs.is_empty());
    }

    #[test]
    fn test_serialization_data_push_pop_context() {
        let mut data = SerializationData::default();
        let ctx = std::ptr::null_mut();
        data.set_context(ctx);
        assert!(!data.contexts.is_empty());
        data.unset_context();
        assert!(data.contexts.is_empty());
    }

    #[test]
    fn test_serialization_data_push_pop_enum() {
        let mut data = SerializationData::default();
        data.set_enum(42);
        assert_eq!(data.get_enum(), 42);
        data.unset_enum();
        assert!(data.enums.is_empty());
    }

    #[derive(Debug, Clone)]
    struct TestCustom {
        value: i32,
    }

    impl CustomData for TestCustom {
        fn get_type(&self) -> &'static str { "TestCustom" }

        fn clone_box(&self) -> Box<dyn CustomData> { Box::new(self.clone()) }

        fn get_type_id(&self) -> TypeId { TypeId::of::<TestCustom>() }
    }

    #[test]
    fn test_serialization_data_custom_roundtrip() {
        let mut data = SerializationData::default();
        assert!(!data.has_custom::<TestCustom>());
        data.set_custom(&TestCustom { value: 5 });
        assert!(data.has_custom::<TestCustom>());
        assert_eq!(data.get_custom::<TestCustom>().value, 5);
        data.unset_custom::<TestCustom>();
        assert!(!data.has_custom::<TestCustom>());
    }
}
