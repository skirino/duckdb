//===----------------------------------------------------------------------===//
//                         DuckDB
//
// common/types/row/tuple_data_scatter_gather_rust_bridge.hpp
//
// Internal glue between the C++ `TupleDataCollection` and the Rust
// `tuple_data::scatter_gather` kernels in `ahirudb`. Not a public header: the
// crubit-generated bindings are only included from the sources in
// `src/common/types/row`.
//
//===----------------------------------------------------------------------===//

#pragma once

#include "duckdb/common/types/row/tuple_data_collection.hpp"
#include "duckdb/common/types/vector.hpp"
#include "duckdb/common/vector/dictionary_vector.hpp"
#include "duckdb/common/vector/flat_vector.hpp"
#include "duckdb/common/vector/unified_vector_format.hpp"

#include "crubit/ahirudb.h"

#include <cstdint>
#include <string>

namespace duckdb {

//! Turns the error message of a Rust scatter/gather kernel into an exception.
//! Defined in tuple_data_scatter_gather_rust_bridge.cpp, where
//! `rs::alloc::string::String` is complete.
[[noreturn]] void ThrowTupleDataScatterGatherError(const ::rs::alloc::string::String &error);

//! A Rust byte slice over a DuckDB buffer, read-only.
inline rs_std::SliceRef<const std::uint8_t> RustBytes(const_data_ptr_t data, idx_t byte_count) {
	return rs_std::SliceRef<const std::uint8_t>(reinterpret_cast<const std::uint8_t *>(data), byte_count);
}

//! The mutable form, for the buffers a kernel writes into.
inline rs_std::SliceRef<std::uint8_t> RustBytes(data_ptr_t data, idx_t byte_count) {
	return rs_std::SliceRef<std::uint8_t>(reinterpret_cast<std::uint8_t *>(data), byte_count);
}

//! Every vector buffer holds at least `STANDARD_VECTOR_SIZE` entries and no
//! selection vector indexes past its vector's rows, so this bounds the entry
//! array of any `UnifiedVectorFormat` a kernel reads. It is the same bound the
//! C++ kernels worked against, which read the entry array through a bare
//! pointer: an index inside the vector's own rows is always inside the
//! buffer, and one outside them was undefined behaviour there and is left to
//! the kernel's own row loops here.
static constexpr idx_t RUST_VECTOR_ENTRY_CAPACITY = STANDARD_VECTOR_SIZE;

//! Rust kernels report a buffer or index that does not fit what they were given
//! as an error, where the C++ they replaced read or wrote out of bounds. Every
//! call site funnels its result through here.
template <class T>
T ThrowOnTupleDataError(rs_std::Result<T, ::rs::alloc::string::String> &&result) {
	if (!result.has_value()) {
		ThrowTupleDataScatterGatherError(result.err());
	}
	return std::move(result).value();
}

//! The same, for the kernels that return nothing.
inline void ThrowOnTupleDataError(rs_std::Result<rs_std::unit_t, ::rs::alloc::string::String> &&result) {
	if (!result.has_value()) {
		ThrowTupleDataScatterGatherError(result.err());
	}
}

//! The entry array a `UnifiedVectorFormat` points at, `entry_size` bytes per entry.
inline rs_std::SliceRef<const std::uint8_t> RustFormatEntries(const UnifiedVectorFormat &format, idx_t entry_size) {
	return RustBytes(const_data_ptr_cast(format.data), RUST_VECTOR_ENTRY_CAPACITY * entry_size);
}

//! A selection vector, or an empty slice when it is not set. Rust reads an empty
//! slice as the identity selection, which is what `SelectionVector::get_index`
//! returns when `sel_vector` is null.
inline rs_std::SliceRef<const std::uint32_t> RustSel(const SelectionVector &sel) {
	if (!sel.IsSet()) {
		return rs_std::SliceRef<const std::uint32_t>();
	}
	return rs_std::SliceRef<const std::uint32_t>(sel.data(), sel.Capacity());
}

//! A validity mask as its raw words, or an empty slice when it cannot have
//! nulls. Rust reads an empty slice as "every row is valid", which is what
//! `ValidityMask::RowIsValid` returns when the mask is null.
inline rs_std::SliceRef<const std::uint64_t> RustValidity(const ValidityMask &validity) {
	if (!validity.CanHaveNull()) {
		return rs_std::SliceRef<const std::uint64_t>();
	}
	return rs_std::SliceRef<const std::uint64_t>(validity.GetData(),
	                                             ValidityMask::EntryCount(validity.Capacity()));
}

//! The row (and heap) address arrays the kernels walk, which are
//! `LogicalType::POINTER` vectors.
inline rs_std::SliceRef<const std::uint64_t> RustAddresses(const Vector &addresses) {
	return rs_std::SliceRef<const std::uint64_t>(
	    reinterpret_cast<const std::uint64_t *>(FlatVector::GetData<data_ptr_t>(addresses)),
	    RUST_VECTOR_ENTRY_CAPACITY);
}

inline rs_std::SliceRef<std::uint64_t> RustAddressesMut(Vector &addresses) {
	return rs_std::SliceRef<std::uint64_t>(reinterpret_cast<std::uint64_t *>(FlatVector::GetDataMutable<data_ptr_t>(addresses)),
	                                       RUST_VECTOR_ENTRY_CAPACITY);
}

//! The entry array a scatter target is written into.
inline rs_std::SliceRef<std::uint8_t> RustTargetEntries(Vector &target, idx_t entry_size) {
	return RustBytes(FlatVector::GetDataMutable(target), RUST_VECTOR_ENTRY_CAPACITY * entry_size);
}

//! The validity mask a gather writes its NULLs into, as raw words.
//!
//! `SetInvalid` allocates the mask when the vector has none, which would leave
//! the slice Rust is handed dangling, so the mask is made writable here first
//! and the words are taken after that.
inline rs_std::SliceRef<std::uint64_t> RustTargetValidity(Vector &target) {
	auto &validity = FlatVector::ValidityMutable(target);
	validity.EnsureWritable();
	return rs_std::SliceRef<std::uint64_t>(validity.GetData(), ValidityMask::EntryCount(validity.Capacity()));
}

//! The Rust kernels of the tuple-data scatter/gather module, in the shape the
//! C++ call sites use them. Each is named after the C++ function it replaced,
//! and each is reached through the module it lives in rather than a re-export,
//! so there is one spelling of it in the tree.
namespace rust {
using ahirudb::tuple_data::scatter_gather::gather_collection;
using ahirudb::tuple_data::scatter_gather::gather_fixed;
using ahirudb::tuple_data::scatter_gather::gather_string;
using ahirudb::tuple_data::scatter_gather::gather_struct;
using ahirudb::tuple_data::scatter_gather::gather_within_collection;
using ahirudb::tuple_data::scatter_gather::gather_within_fixed;
using ahirudb::tuple_data::scatter_gather::gather_within_string;
using ahirudb::tuple_data::scatter_gather::gather_within_struct;
using ahirudb::tuple_data::scatter_gather::initialize_validity_mask;
using ahirudb::tuple_data::scatter_gather::scatter_collection;
using ahirudb::tuple_data::scatter_gather::scatter_fixed;
using ahirudb::tuple_data::scatter_gather::scatter_string;
using ahirudb::tuple_data::scatter_gather::scatter_struct;
using ahirudb::tuple_data::scatter_gather::scatter_within_collection;
using ahirudb::tuple_data::scatter_gather::scatter_within_fixed;
using ahirudb::tuple_data::scatter_gather::scatter_within_string;
using ahirudb::tuple_data::scatter_gather::scatter_within_struct;
} // namespace rust

} // namespace duckdb
