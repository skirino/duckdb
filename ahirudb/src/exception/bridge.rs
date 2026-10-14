//! The C-ABI surface of the exception module: everything C++ may call, and
//! nothing else.
//!
//! Crubit cannot bridge a Rust `Result`, so no function here fails - a name
//! that belongs to no exception type answers the same default the C++ original
//! answered, and the factories cannot fail at all. That is what keeps a Rust
//! panic from ever having to cross the boundary: the code behind these
//! functions only builds strings, which cannot panic except on allocation
//! failure.

use crate::exception::{
    classification,
    extra_info,
    factories,
    types::{
        self,
        ByteSlice,
        ExceptionPayload,
        ExceptionType,
        QueryLocationLayout,
    },
};

/// The name an exception type is reported under, or `"Unknown"` when it has
/// none. Matching `Exception::ExceptionTypeToString`.
pub fn exception_type_to_string(exception_type: u8) -> &'static str {
    types::exception_type_to_string(exception_type)
}

/// The exception type a name stands for, or `INVALID` when it names none.
/// Matching `Exception::StringToExceptionType`.
pub fn string_to_exception_type(name: &[u8]) -> u8 {
    match types::string_to_exception_type(name)
    {
        Some(exception_type) => exception_type as u8,
        None => ExceptionType::INVALID as u8,
    }
}

/// Whether this exception type invalidates the current transaction.
/// Matching `Exception::InvalidatesTransaction`.
pub fn invalidates_transaction(exception_type: u8) -> bool {
    match ExceptionType::from_u8(exception_type)
    {
        Some(exception_type) => classification::invalidates_transaction(exception_type),
        // A discriminant that names no type is as fatal as any other unknown failure.
        None => true,
    }
}

/// Whether this exception type invalidates the entire database.
/// Matching `Exception::InvalidatesDatabase`.
pub fn invalidates_database(exception_type: u8) -> bool {
    match ExceptionType::from_u8(exception_type)
    {
        Some(exception_type) => classification::invalidates_database(exception_type),
        None => false,
    }
}

/// Whether this exception type can occur during query execution.
/// Matching `Exception::IsExecutionError`.
pub fn is_execution_error(exception_type: u8) -> bool {
    match ExceptionType::from_u8(exception_type)
    {
        Some(exception_type) => classification::is_execution_error(exception_type),
        None => false,
    }
}

/// The `extra_info` entries `Exception::SetQueryLocation` adds. The message of
/// the payload is empty; this entry point only builds entries.
pub fn set_query_location(query_location: QueryLocationLayout) -> ExceptionPayload {
    types::into_payload(Vec::new(), extra_info::set_query_location(query_location))
}

/// `BinderException::ColumnNotFound`. `candidate_message` is rendered by C++.
pub fn binder_column_not_found(
    name: &[u8],
    similar_bindings: &[ByteSlice],
    candidate_message: &[u8],
    query_location: QueryLocationLayout,
) -> ExceptionPayload {
    factories::binder_column_not_found(
        name,
        &borrow_all(similar_bindings),
        candidate_message,
        query_location,
    )
}

/// `BinderException::NoMatchingFunction`. `call_string` is rendered by C++.
pub fn binder_no_matching_function(
    catalog_name: &[u8],
    schema_name: &[u8],
    name: &[u8],
    call_string: &[u8],
    candidates: &[ByteSlice],
) -> ExceptionPayload {
    factories::binder_no_matching_function(
        catalog_name,
        schema_name,
        name,
        call_string,
        &borrow_all(candidates),
    )
}

/// `CatalogException::MissingEntry(const EntryLookupInfo &, const string
/// &suggestion)`. `version_info` is rendered by C++.
pub fn catalog_missing_entry(
    catalog_type: &[u8],
    name: &[u8],
    suggestion: &[u8],
    version_info: &[u8],
    query_location: QueryLocationLayout,
) -> ExceptionPayload {
    factories::catalog_missing_entry(catalog_type, name, suggestion, version_info, query_location)
}

/// `CatalogException::MissingEntry(const string &type, const Identifier &name,
/// ...)`. `candidate_message` is rendered by C++.
pub fn catalog_missing_entry_suggestions(
    catalog_type: &[u8],
    name: &[u8],
    suggestions: &[ByteSlice],
    candidate_message: &[u8],
    query_location: QueryLocationLayout,
) -> ExceptionPayload {
    factories::catalog_missing_entry_suggestions(
        catalog_type,
        name,
        &borrow_all(suggestions),
        candidate_message,
        query_location,
    )
}

/// `CatalogException::EntryAlreadyExists`.
pub fn catalog_entry_already_exists(catalog_type: &[u8], name: &[u8]) -> ExceptionPayload {
    factories::catalog_entry_already_exists(catalog_type, name)
}

/// Releases the message and the entries of a payload. C++ calls this exactly
/// once per payload, after copying them out.
pub fn free_payload(payload: ExceptionPayload) { types::free_payload(payload); }

/// Borrows every element of a list C++ handed over. The views borrow from C++
/// strings that outlive the call that made them.
fn borrow_all(slices: &[ByteSlice]) -> Vec<&[u8]> {
    slices.iter().map(|slice| slice.as_slice()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exception::types::free_payload;

    #[test]
    fn the_type_name_round_trips_through_the_bridge() {
        assert_eq!("Binder", exception_type_to_string(24));
        assert_eq!("Unknown", exception_type_to_string(45));
        assert_eq!(ExceptionType::BINDER as u8, string_to_exception_type(b"Binder"));
        assert_eq!(ExceptionType::INVALID as u8, string_to_exception_type(b"not a type"));
    }

    #[test]
    fn an_unknown_discriminant_is_treated_as_an_internal_error() {
        assert!(invalidates_transaction(200));
        assert!(!invalidates_database(200));
        assert!(!is_execution_error(200));
    }

    #[test]
    fn the_classifications_are_the_ones_the_cpp_had() {
        assert!(!invalidates_transaction(ExceptionType::BINDER as u8));
        assert!(invalidates_database(ExceptionType::FATAL as u8));
        assert!(is_execution_error(ExceptionType::CONVERSION as u8));
    }

    #[test]
    fn the_factories_reach_the_bridge_intact() {
        let payload = catalog_entry_already_exists(b"Table", b"tbl");
        let message = payload.message.as_slice().to_vec();
        free_payload(payload);
        assert_eq!("Table with name \"tbl\" already exists!".as_bytes(), message);
    }

    #[test]
    fn a_list_of_views_becomes_a_list_of_slices() {
        let values: Vec<Vec<u8>> = vec![b"a".to_vec(), b"bb".to_vec()];
        let slices: Vec<ByteSlice> = values.iter().map(|v| ByteSlice::new(v)).collect();
        let borrowed = borrow_all(&slices);
        assert_eq!(vec![b"a".as_slice(), b"bb".as_slice()], borrowed);
    }
}
