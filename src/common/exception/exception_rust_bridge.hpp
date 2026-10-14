//===----------------------------------------------------------------------===//
//                         DuckDB
//
// common/exception/exception_rust_bridge.hpp
//
// Internal glue between the C++ exception classes and the Rust exception module in
// `ahirudb`. Not a public header: the crubit-generated bindings are only included from
// the sources in `src/common/exception`.
//
// Crubit cannot bridge a Rust `Result`, and it certainly cannot bridge a Rust exception, so
// nothing here can fail: every Rust entry point answers with a value, and the C++ side
// turns it into the `Exception` that actually gets thrown. The message text is built in
// Rust; building the `Exception` - and with it the JSON payload of `what()` - stays in
// C++, because `std::runtime_error` is what callers catch.
//
//===----------------------------------------------------------------------===//

#pragma once

#include "duckdb/common/common.hpp"
#include "duckdb/common/exception.hpp"
#include "duckdb/common/query_location.hpp"
#include "duckdb/common/unordered_map.hpp"

#include <vector>

namespace duckdb {

//! Every factory writes the message it built into `message` and the `extra_info` that goes
//! with it into `extra_info`, which is what the `Exception` constructors take.
struct ExceptionRustBridge {
	// --- Classification and naming ---
	static string ExceptionTypeToString(ExceptionType type);
	static ExceptionType StringToExceptionType(const string &type);
	static bool InvalidatesTransaction(ExceptionType type);
	static bool InvalidatesDatabase(ExceptionType type);
	static bool IsExecutionError(ExceptionType type);

	//! Adds the `position`/`location` entries of a query location, the way
	//! `Exception::SetQueryLocation` did. Adds nothing for a location that is not known.
	static void SetQueryLocation(QueryLocation error_location, unordered_map<string, string> &extra_info);

	// --- Factories ---
	//! `candidate_message` is what `StringUtil::CandidatesMessage` makes of
	//! `similar_bindings`, which C++ renders.
	static void ColumnNotFound(const string &name, const vector<string> &similar_bindings,
	                           const string &candidate_message, QueryLocation query_location, string &message,
	                           unordered_map<string, string> &extra_info);
	//! `call_string` is `Function::CallToString`, which C++ renders: whether a call is
	//! prefixed with its catalog and schema follows `Function`'s own rules.
	static void NoMatchingFunction(const string &catalog_name, const string &schema_name, const string &name,
	                               const string &call_string, const vector<string> &candidates, string &message,
	                               unordered_map<string, string> &extra_info);
	//! `version_info` is the ` at <unit> <value>` clause of an `AT` lookup, which C++ renders.
	static void MissingEntry(const string &catalog_type, const string &name, const string &suggestion,
	                         const string &version_info, QueryLocation query_location, string &message,
	                         unordered_map<string, string> &extra_info);
	//! `candidate_message` is `StringUtil::CandidatesErrorMessage`, which C++ renders because
	//! it ranks the suggestions by Levenshtein distance first.
	static void MissingEntry(const string &catalog_type, const string &name, const vector<string> &suggestions,
	                         const string &candidate_message, QueryLocation query_location, string &message,
	                         unordered_map<string, string> &extra_info);
	static void EntryAlreadyExists(const string &catalog_type, const string &name, string &message,
	                               unordered_map<string, string> &extra_info);
};

} // namespace duckdb
