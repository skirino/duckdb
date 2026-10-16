//===----------------------------------------------------------------------===//
//                         DuckDB
//
// common/multi_file/multi_file_data.cpp
//
// The rules `multi_file_data.hpp` used to spell out inline: which index is valid, what a filter entry means, and what
// an index into one of the two maps refers to. They live in the Rust `multi_file` module now, and this is where its
// answers are turned back into C++. Every one of them is out of line, because a public header cannot include the
// crubit-generated bindings that reach them.
//
//===----------------------------------------------------------------------===//

#include "duckdb/common/multi_file/multi_file_data.hpp"

#include "multi_file_rust_bridge.hpp"

namespace duckdb {

idx_t MultiFileLocalIndex::GetIndex() const {
	return ThrowOnMultiFileError(rust::local_index_get_index(index));
}

bool MultiFileLocalIndex::IsValid() const {
	return rust::local_index_is_valid(index);
}

void MultiFileFilterEntry::Set(MultiFileConstantMapIndex constant_index) {
	MultiFileFilterEntryAccess::FromRust(*this, rust::filter_entry_set_constant(constant_index.index));
}

void MultiFileFilterEntry::Set(MultiFileLocalIndex local_index) {
	MultiFileFilterEntryAccess::FromRust(*this, rust::filter_entry_set_local(local_index.index));
}

bool MultiFileFilterEntry::IsConstant() const {
	return ThrowOnMultiFileError(rust::filter_entry_is_constant(MultiFileFilterEntryAccess::ToRust(*this)));
}

MultiFileConstantMapIndex MultiFileFilterEntry::GetConstantIndex() const {
	return MultiFileConstantMapIndex(
	    ThrowOnMultiFileError(rust::filter_entry_get_constant_index(MultiFileFilterEntryAccess::ToRust(*this))));
}

MultiFileLocalIndex MultiFileFilterEntry::GetLocalIndex() const {
	return MultiFileLocalIndex(
	    ThrowOnMultiFileError(rust::filter_entry_get_local_index(MultiFileFilterEntryAccess::ToRust(*this))));
}

MultiFileFilterEntry &MultiFileFilterMap::operator[](MultiFileGlobalIndex global_index) {
	return filter_map[ResolveMultiFileFilterMapIndex(filter_map.size(), global_index.index)];
}

idx_t ResolveMultiFileLocalIndex(idx_t size, idx_t index) {
	return ThrowOnMultiFileError(rust::local_column_ids_get(size, index));
}

idx_t ResolveMultiFileFilterMapIndex(idx_t size, idx_t index) {
	return ThrowOnMultiFileError(rust::filter_map_get(size, index));
}

idx_t ResolveMultiFileConstantMapIndex(idx_t size, idx_t index) {
	return ThrowOnMultiFileError(rust::constant_map_get(size, index));
}

} // namespace duckdb
