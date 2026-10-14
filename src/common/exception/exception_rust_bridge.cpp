#include "exception_rust_bridge.hpp"

#include "crubit/ahirudb.h"

namespace duckdb {

//===--------------------------------------------------------------------===//
// Marshalling
//===--------------------------------------------------------------------===//

//! The Rust mirror of `QueryLocation`. An invalid location keeps its `INVALID_OFFSET`
//! offset, which is how Rust recognises it as unknown.
static ahirudb::exception::types::QueryLocationLayout ToRustQueryLocation(QueryLocation location) {
	ahirudb::exception::types::QueryLocationLayout result;
	result.offset = location.offset;
	result.length = location.length;
	return result;
}

//! The Rust `ExceptionType` is passed as its discriminant, since C++ enums are not bridged.
static uint8_t ToRustExceptionType(ExceptionType type) {
	return static_cast<uint8_t>(type);
}

//! Borrows a DuckDB string. DuckDB strings are not necessarily valid UTF-8, which is why
//! the boundary carries bytes.
static ahirudb::exception::types::ByteSlice ToRustBytes(const string &value) {
	ahirudb::exception::types::ByteSlice result;
	result.ptr = reinterpret_cast<const uint8_t *>(value.data());
	result.len = value.size();
	return result;
}

//! Borrows a list of DuckDB strings. The views point into the `duckdb::string`s the caller
//! owns, which is what keeps them alive for the length of the call.
static vector<ahirudb::exception::types::ByteSlice> ToRustBytes(const vector<string> &values) {
	vector<ahirudb::exception::types::ByteSlice> result;
	result.reserve(values.size());
	for (const auto &value : values) {
		result.push_back(ToRustBytes(value));
	}
	return result;
}

static rs_std::SliceRef<const uint8_t> AsSlice(const string &value) {
	return rs_std::SliceRef<const uint8_t>(reinterpret_cast<const uint8_t *>(value.data()), value.size());
}

static rs_std::SliceRef<const ahirudb::exception::types::ByteSlice>
AsSlice(const vector<ahirudb::exception::types::ByteSlice> &values) {
	return rs_std::SliceRef<const ahirudb::exception::types::ByteSlice>(values.data(), values.size());
}

//! Copies a run of bytes Rust owns into a DuckDB string. An empty run points at Rust's
//! dangling-but-aligned sentinel, which is not a pointer to read from.
static string FromRustBytes(const ahirudb::exception::types::OwnedBytes &bytes) {
	if (bytes.len == 0) {
		return string();
	}
	return string(reinterpret_cast<const char *>(bytes.ptr), bytes.len);
}

//! Moves everything out of a payload. Every payload is owned by Rust until it is handed to
//! `free_payload` exactly once.
static void TakePayload(const ahirudb::exception::types::ExceptionPayload &payload, string &message,
                        unordered_map<string, string> &extra_info) {
	message = FromRustBytes(payload.message);
	for (idx_t entry_index = 0; entry_index < payload.entry_count; entry_index++) {
		const auto &entry = payload.entries[entry_index];
		extra_info[FromRustBytes(entry.key)] = FromRustBytes(entry.value);
	}
}

namespace {

//! Owns a payload for as long as it is in C++ hands. Rust allocates it and Rust frees it, and
//! this is what makes the freeing happen exactly once - including when copying the payload out
//! throws, which assigning a string into a map may do.
struct PayloadOwner {
	explicit PayloadOwner(ahirudb::exception::types::ExceptionPayload owned) : payload(owned) {
	}
	PayloadOwner(const PayloadOwner &) = delete;
	PayloadOwner &operator=(const PayloadOwner &) = delete;

	~PayloadOwner() {
		ahirudb::exception::bridge::free_payload(payload);
	}

	ahirudb::exception::types::ExceptionPayload payload;
};

} // namespace

//! Calls `build`, which returns a payload, and frees it again once it has been copied out.
template <class BUILD>
static void WithPayload(BUILD &&build, string &message, unordered_map<string, string> &extra_info) {
	PayloadOwner owner(build());
	TakePayload(owner.payload, message, extra_info);
}

//! Same as [`WithPayload`] for the entry points that build `extra_info` only.
template <class BUILD>
static void WithExtraInfo(BUILD &&build, unordered_map<string, string> &extra_info) {
	string ignored_message;
	WithPayload(std::forward<BUILD>(build), ignored_message, extra_info);
}

//===--------------------------------------------------------------------===//
// ExceptionRustBridge
//===--------------------------------------------------------------------===//

string ExceptionRustBridge::ExceptionTypeToString(ExceptionType type) {
	return string(ahirudb::exception::bridge::exception_type_to_string(ToRustExceptionType(type)));
}

ExceptionType ExceptionRustBridge::StringToExceptionType(const string &type) {
	return static_cast<ExceptionType>(ahirudb::exception::bridge::string_to_exception_type(AsSlice(type)));
}

bool ExceptionRustBridge::InvalidatesTransaction(ExceptionType type) {
	return ahirudb::exception::bridge::invalidates_transaction(ToRustExceptionType(type));
}

bool ExceptionRustBridge::InvalidatesDatabase(ExceptionType type) {
	return ahirudb::exception::bridge::invalidates_database(ToRustExceptionType(type));
}

bool ExceptionRustBridge::IsExecutionError(ExceptionType type) {
	return ahirudb::exception::bridge::is_execution_error(ToRustExceptionType(type));
}

void ExceptionRustBridge::SetQueryLocation(QueryLocation error_location, unordered_map<string, string> &extra_info) {
	WithExtraInfo([&]() { return ahirudb::exception::bridge::set_query_location(ToRustQueryLocation(error_location)); },
	              extra_info);
}

void ExceptionRustBridge::ColumnNotFound(const string &name, const vector<string> &similar_bindings,
                                         const string &candidate_message, QueryLocation query_location, string &message,
                                         unordered_map<string, string> &extra_info) {
	auto similar = ToRustBytes(similar_bindings);
	WithPayload(
	    [&]() {
		    return ahirudb::exception::bridge::binder_column_not_found(
		        AsSlice(name), AsSlice(similar), AsSlice(candidate_message), ToRustQueryLocation(query_location));
	    },
	    message, extra_info);
}

void ExceptionRustBridge::NoMatchingFunction(const string &catalog_name, const string &schema_name, const string &name,
                                             const string &call_string, const vector<string> &candidates,
                                             string &message, unordered_map<string, string> &extra_info) {
	auto rust_candidates = ToRustBytes(candidates);
	WithPayload(
	    [&]() {
		    return ahirudb::exception::bridge::binder_no_matching_function(AsSlice(catalog_name), AsSlice(schema_name),
		                                                                   AsSlice(name), AsSlice(call_string),
		                                                                   AsSlice(rust_candidates));
	    },
	    message, extra_info);
}

void ExceptionRustBridge::MissingEntry(const string &catalog_type, const string &name, const string &suggestion,
                                       const string &version_info, QueryLocation query_location, string &message,
                                       unordered_map<string, string> &extra_info) {
	WithPayload(
	    [&]() {
		    return ahirudb::exception::bridge::catalog_missing_entry(AsSlice(catalog_type), AsSlice(name),
		                                                             AsSlice(suggestion), AsSlice(version_info),
		                                                             ToRustQueryLocation(query_location));
	    },
	    message, extra_info);
}

void ExceptionRustBridge::MissingEntry(const string &catalog_type, const string &name,
                                       const vector<string> &suggestions, const string &candidate_message,
                                       QueryLocation query_location, string &message,
                                       unordered_map<string, string> &extra_info) {
	auto rust_suggestions = ToRustBytes(suggestions);
	WithPayload(
	    [&]() {
		    return ahirudb::exception::bridge::catalog_missing_entry_suggestions(
		        AsSlice(catalog_type), AsSlice(name), AsSlice(rust_suggestions), AsSlice(candidate_message),
		        ToRustQueryLocation(query_location));
	    },
	    message, extra_info);
}

void ExceptionRustBridge::EntryAlreadyExists(const string &catalog_type, const string &name, string &message,
                                             unordered_map<string, string> &extra_info) {
	WithPayload(
	    [&]() {
		    return ahirudb::exception::bridge::catalog_entry_already_exists(AsSlice(catalog_type), AsSlice(name));
	    },
	    message, extra_info);
}

} // namespace duckdb
