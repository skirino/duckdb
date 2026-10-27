//===----------------------------------------------------------------------===//
//                         DuckDB
//
// common/multi_file/multi_file_rust_bridge.hpp
//
// Internal glue between the C++ multi-file reader and the Rust `multi_file` module in `ahirudb`. Not a public header:
// the crubit-generated bindings are only included from the sources in `src/common/multi_file`, and
// `multi_file_data.hpp` and `multi_file_list.hpp` are included by every reader extension.
//
//===----------------------------------------------------------------------===//

#pragma once

#include "duckdb/common/common.hpp"
#include "duckdb/common/exception.hpp"
#include "duckdb/common/multi_file/multi_file_data.hpp"
#include "duckdb/common/multi_file/multi_file_list.hpp"

#include "crubit/ahirudb.h"

namespace duckdb {

//! Turns the error message of a Rust kernel into the exception the C++ it replaced threw. Defined in
//! multi_file_rust_bridge.cpp, where `rs::alloc::string::String` is complete.
[[noreturn]] void ThrowMultiFileError(const ::rs::alloc::string::String &error);

//===--------------------------------------------------------------------===//
// Call scope bookkeeping (mirrors row_operations)
//===--------------------------------------------------------------------===//

namespace multi_file_bridge {

//! Marks the [`CallIntoRust`] invocation that a multi-file trampoline error may be attributed to.
class CallScope {
public:
	CallScope();
	~CallScope();

private:
	friend void RethrowPendingError(const CallScope &call_scope);

	//! Identifies this call
	uint64_t index;
	//! The scope that was active before this one
	uint64_t previous;
};

//! Rethrows the exception that a trampoline caught during `call_scope`, if any.
void RethrowPendingError(const CallScope &call_scope);
//! Discards the exception a trampoline caught during a call that never came back through `CallIntoRust`.
void ClearPendingError();

//! Runs `fn` and rethrows the exception that a trampoline caught on this thread.
template <class F>
auto CallIntoRust(F &&fn) -> decltype(fn()) {
	ClearPendingError();
	CallScope call_scope;
	if constexpr (std::is_void_v<decltype(fn())>) {
		fn();
		RethrowPendingError(call_scope);
	} else {
		auto result = fn();
		RethrowPendingError(call_scope);
		return result;
	}
}

//! Unwraps a Rust `Result`, turning an `Err` message into an exception.
template <class T>
T ThrowOnMultiFileError(rs_std::Result<T, ::rs::alloc::string::String> &&result) {
	if (!result.has_value()) {
		ThrowMultiFileError(result.err());
	}
	return std::move(result).value();
}

//! The same, for the entry points that return nothing.
inline void ThrowOnMultiFileError(rs_std::Result<rs_std::unit_t, ::rs::alloc::string::String> &&result) {
	if (!result.has_value()) {
		ThrowMultiFileError(result.err());
	}
}

} // namespace multi_file_bridge

using multi_file_bridge::CallScope;
using multi_file_bridge::CallIntoRust;
using multi_file_bridge::RethrowPendingError;
using multi_file_bridge::ClearPendingError;
using multi_file_bridge::ThrowOnMultiFileError;

//===--------------------------------------------------------------------===//
// Rust module bindings
//===--------------------------------------------------------------------===//

//! The Rust kernels of the multi-file module, in the shape the C++ call sites use them.
namespace rust {
using ahirudb::multi_file::data::constant_map_get;
using ahirudb::multi_file::data::filter_entry_get_constant_index;
using ahirudb::multi_file::data::filter_entry_get_local_index;
using ahirudb::multi_file::data::filter_entry_is_constant;
using ahirudb::multi_file::data::filter_entry_set_constant;
using ahirudb::multi_file::data::filter_entry_set_local;
using ahirudb::multi_file::data::filter_map_get;
using ahirudb::multi_file::data::local_column_ids_get;
using ahirudb::multi_file::data::local_index_get_index;
using ahirudb::multi_file::data::local_index_is_valid;
using ahirudb::multi_file::data::MultiFileFilterEntryLayout;

using ahirudb::multi_file::list::expand_result;
using ahirudb::multi_file::list::file_count;
using ahirudb::multi_file::list::FileExpandResult;
using ahirudb::multi_file::list::FileExpansionType;
using ahirudb::multi_file::list::MultiFileCount;
using ahirudb::multi_file::list::MultiFileListScanType;
using ahirudb::multi_file::list::scan_fetches_unconditionally;

using ahirudb::multi_file::reader::PreOpenConstantKind;
using ahirudb::multi_file::reader::PreOpenConstantResult;
using ahirudb::multi_file::reader::resolve_pre_open_constant;
using ahirudb::multi_file::reader::resolve_pre_open_constant_with_file_index;

using ahirudb::multi_file::column_mapper::ColumnMappingOutcome;
using ahirudb::multi_file::column_mapper::ColumnPlanEntry;
using ahirudb::multi_file::column_mapper::FilterConversion;
using ahirudb::multi_file::column_mapper::is_trivially_mappable;
using ahirudb::multi_file::column_mapper::is_trivially_mappable_with_check;
} // namespace rust

//! A `MultiFileFilterEntry` as the Rust layout
struct MultiFileFilterEntryAccess {
	static rust::MultiFileFilterEntryLayout ToRust(const MultiFileFilterEntry &entry) {
		return rust::MultiFileFilterEntryLayout::new_(entry.is_set, entry.is_constant, entry.index);
	}

	static void FromRust(MultiFileFilterEntry &entry, const rust::MultiFileFilterEntryLayout &layout) {
		entry.is_set = layout.is_set;
		entry.is_constant = layout.is_constant;
		entry.index = layout.index;
	}
};

//! Rust enum -> C++ enum conversions
inline FileExpandResult ToCppExpandResult(rust::FileExpandResult result) {
	if (result == rust::FileExpandResult::MakeNO_FILES()) {
		return FileExpandResult::NO_FILES;
	}
	if (result == rust::FileExpandResult::MakeSINGLE_FILE()) {
		return FileExpandResult::SINGLE_FILE;
	}
	if (result == rust::FileExpandResult::MakeMULTIPLE_FILES()) {
		return FileExpandResult::MULTIPLE_FILES;
	}
	throw InternalException("Unknown file expand result");
}

inline FileExpansionType ToCppExpansionType(rust::FileExpansionType type) {
	if (type == rust::FileExpansionType::MakeALL_FILES_EXPANDED()) {
		return FileExpansionType::ALL_FILES_EXPANDED;
	}
	if (type == rust::FileExpansionType::MakeNOT_ALL_FILES_KNOWN()) {
		return FileExpansionType::NOT_ALL_FILES_KNOWN;
	}
	throw InternalException("Unknown file expansion type");
}

inline rust::MultiFileListScanType ToRustScanType(MultiFileListScanType scan_type) {
	switch (scan_type) {
	case MultiFileListScanType::ALWAYS_FETCH:
		return rust::MultiFileListScanType::MakeALWAYS_FETCH();
	case MultiFileListScanType::FETCH_IF_AVAILABLE:
		return rust::MultiFileListScanType::MakeFETCH_IF_AVAILABLE();
	default:
		throw InternalException("Unknown multi-file list scan type");
	}
}

inline MultiFileCount FromRust(const rust::MultiFileCount &result) {
	return MultiFileCount(result.get_count(), ToCppExpansionType(result.get_type()));
}

} // namespace duckdb
