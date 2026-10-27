//===----------------------------------------------------------------------===//
//                         DuckDB
//
// storage/metadata/metadata_rust_bridge.hpp
//
// Internal glue between the C++ metadata classes and the Rust metadata
// module in `ahirudb`. Not a public header: the crubit-generated bindings
// are only included from the sources in `src/storage/metadata`.
//===----------------------------------------------------------------------===//

#pragma once

#include "duckdb/common/exception.hpp"
#include "duckdb/storage/block.hpp"
#include "duckdb/storage/database_size.hpp"

#include "crubit/ahirudb.h"

#include <cstdint>
#include <vector>

namespace duckdb {

//! Turns the error message of a Rust metadata kernel into an exception.
[[noreturn]] void ThrowMetadataError(const ::rs::alloc::string::String &error);

//! Funnels a Rust metadata `Result` through the throwing path.
template <class T>
T ThrowOnMetadataError(rs_std::Result<T, ::rs::alloc::string::String> &&result) {
	if (!result.has_value()) {
		ThrowMetadataError(result.err());
	}
	return std::move(result).value();
}

inline void ThrowOnMetadataError(rs_std::Result<rs_std::unit_t, ::rs::alloc::string::String> &&result) {
	if (!result.has_value()) {
		ThrowMetadataError(result.err());
	}
}

//! Namespace aliases for the Rust metadata kernels.
namespace rust {
namespace metadata = ahirudb::storage::metadata;

using MetadataManagerState = metadata::bridge::MetadataManagerState;
using BlockList = metadata::bridge::BlockList;
using WriteSegmentList = metadata::bridge::WriteSegmentList;
using ReadSegmentList = metadata::bridge::ReadSegmentList;
using WriteSegment = metadata::writer::WriteSegment;
using ReadSegment = metadata::reader::ReadSegment;
using NextBlockState = metadata::writer::NextBlockState;
// The Rust pointer types shadow the C++ `duckdb::MetaBlockPointer`/
// `duckdb::BlockPointer` inside this namespace, so the conversion helpers
// below name their Rust parameter types without a `duckdb::` prefix.
using MetaBlockPointer = metadata::pointer::MetaBlockPointer;
using BlockPointer = metadata::pointer::BlockPointer;

// The C++ `MetaBlockPointer`/`BlockPointer` and their Rust counterparts are
// layout-compatible (same field order, same widths), so the conversions are
// field copies rather than re-encodings.

inline MetaBlockPointer ToRustMetaBlockPointer(const duckdb::MetaBlockPointer &ptr) {
	MetaBlockPointer result;
	result.block_pointer = ptr.block_pointer;
	result.offset = ptr.offset;
	result.unused_padding = ptr.unused_padding;
	return result;
}

inline duckdb::MetaBlockPointer ToCppMetaBlockPointer(const MetaBlockPointer &ptr) {
	duckdb::MetaBlockPointer result;
	result.block_pointer = ptr.block_pointer;
	result.offset = ptr.offset;
	result.unused_padding = ptr.unused_padding;
	return result;
}

inline BlockPointer ToRustBlockPointer(const duckdb::BlockPointer &ptr) {
	BlockPointer result;
	result.block_id = ptr.block_id;
	result.offset = ptr.offset;
	result.unused_padding = ptr.unused_padding;
	return result;
}

inline duckdb::BlockPointer ToCppBlockPointer(const BlockPointer &ptr) {
	duckdb::BlockPointer result;
	result.block_id = ptr.block_id;
	result.offset = ptr.offset;
	result.unused_padding = ptr.unused_padding;
	return result;
}

//! Build a Rust free-block slice over a C++ `vector<uint8_t>`.
inline rs_std::SliceRef<const std::uint8_t> RustFreeBlocks(const std::vector<std::uint8_t> &blocks) {
	return rs_std::SliceRef<const std::uint8_t>(
	    blocks.empty() ? nullptr : reinterpret_cast<const std::uint8_t *>(blocks.data()), blocks.size());
}

//! Copy a Rust `BlockList` into a C++ `vector<uint8_t>` and free the Rust side.
//!
//! An empty list (free_list == 0) clears `out`, matching the original
//! `MetadataBlock::FreeBlocksFromInteger` semantics that call `clear()` before
//! returning. Without this, a stale free-block list would survive across a
//! re-encode and corrupt the on-disk bitmask.
inline void CopyBlockList(const BlockList &list, std::vector<std::uint8_t> &out) {
	if (list.data != nullptr && list.count > 0) {
		out.assign(reinterpret_cast<const std::uint8_t *>(list.data),
		           reinterpret_cast<const std::uint8_t *>(list.data) + list.count);
	} else {
		out.clear();
	}
	metadata::bridge::free_block_list(list);
}

} // namespace rust

} // namespace duckdb
