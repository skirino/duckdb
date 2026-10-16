//===----------------------------------------------------------------------===//
//                         DuckDB
//
// common/multi_file/multi_file_rust_bridge.hpp
//
// Internal glue between the C++ multi-file reader and the Rust `multi_file` module in `ahirudb`. Not a public header:
// the crubit-generated bindings are only included from the sources in `src/common/multi_file`, and
// `multi_file_data.hpp` and `multi_file_list.hpp` are included by every reader extension.
//
// What is here is the seam: the C++ structs keep their members and their virtual functions, and the rules over those
// members - which index is valid, what a filter entry means, how many files a list has, and when its expansion is over
// - are the Rust module's. The conversions below are the whole of the glue.
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

//! Every call into the Rust module funnels its result through here, so that the message the kernel wrote is what
//! surfaces. A Rust error is a value rather than a throw, because a throw cannot cross the boundary.
template <class T>
T ThrowOnMultiFileError(rs_std::Result<T, ::rs::alloc::string::String> &&result) {
	if (!result.has_value()) {
		ThrowMultiFileError(result.err());
	}
	return std::move(result).value();
}

//! The Rust kernels of the multi-file module, in the shape the C++ call sites use them. Each is named after the C++
//! method it backs, and each is reached through the module it lives in rather than a re-export, so there is one
//! spelling of it in the tree.
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
} // namespace rust

//! A `MultiFileFilterEntry` as the Rust layout - its three members and nothing else - and back.
//!
//! It is a friend of the entry rather than a free function, so that the three members stay private: an extension
//! that includes the public header can call `Set`, `IsSet` and the index getters, and can touch the fields only through
//! this struct - which is defined here, in a header no extension includes.
struct MultiFileFilterEntryAccess {
	static rust::MultiFileFilterEntryLayout ToRust(const MultiFileFilterEntry &entry) {
		// built in one step, rather than default-constructed across the boundary and then overwritten field by field
		return rust::MultiFileFilterEntryLayout::new_(entry.is_set, entry.is_constant, entry.index);
	}

	static void FromRust(MultiFileFilterEntry &entry, const rust::MultiFileFilterEntryLayout &layout) {
		entry.is_set = layout.is_set;
		entry.is_constant = layout.is_constant;
		entry.index = layout.index;
	}
};

//! Rust enum -> C++ enum. Not a `static_cast`: crubit models a fieldless enum as a tagged union whose payload is
//! private, so each variant is named and anything else is refused - the same shape as `ToRustScanType` below, so an
//! enum that gains or reorders a variant fails here rather than yielding the wrong value.
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

//! Not a `static_cast`: crubit models a fieldless enum as a tagged union whose payload is private, so a Rust enum is
//! built from the variant it stands for.
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

//! The Rust count, as the C++ pair it replaces. `MultiFileCount`'s `type` says whether `count` is exact, which the C++
//! struct carries as a second member for the same reason.
inline MultiFileCount FromRust(const rust::MultiFileCount &result) {
	return MultiFileCount(result.get_count(), ToCppExpansionType(result.get_type()));
}

} // namespace duckdb
