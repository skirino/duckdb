//===----------------------------------------------------------------------===//
//                         DuckDB
//
// storage/buffer/buffer_rust_bridge.hpp
//
// Internal glue between the C++ buffer classes and the Rust buffer module in
// `ahirudb`. Not a public header: the crubit-generated bindings are only
// included from the sources in `src/storage/buffer`.
//
//===----------------------------------------------------------------------===//

#pragma once

#include "duckdb/common/exception.hpp"
#include "duckdb/common/types/data_chunk.hpp"

#include "crubit/ahirudb.h"

#include <cstdint>
#include <string>

namespace duckdb {

//! Turns the error message of a Rust buffer kernel into an exception.
[[noreturn]] void ThrowBufferError(const ::rs::alloc::string::String &error);

//! Funnels a Rust buffer `Result` through the throwing path.
template <class T>
T ThrowOnBufferError(rs_std::Result<T, ::rs::alloc::string::String> &&result) {
	if (!result.has_value()) {
		ThrowBufferError(result.err());
	}
	return std::move(result).value();
}

inline void ThrowOnBufferError(rs_std::Result<rs_std::unit_t, ::rs::alloc::string::String> &&result) {
	if (!result.has_value()) {
		ThrowBufferError(result.err());
	}
}

//! Rust byte slices over DuckDB buffers.
inline rs_std::SliceRef<const std::uint8_t> RustBytes(const_data_ptr_t data, idx_t byte_count) {
	return rs_std::SliceRef<const std::uint8_t>(reinterpret_cast<const std::uint8_t *>(data), byte_count);
}

inline rs_std::SliceRef<std::uint8_t> RustBytes(data_ptr_t data, idx_t byte_count) {
	return rs_std::SliceRef<std::uint8_t>(reinterpret_cast<std::uint8_t *>(data), byte_count);
}

//! Namespace aliases for the Rust buffer kernels.
namespace rust {
using ahirudb::buffer::block_copy::copy_block;
using ahirudb::buffer::block_copy::copy_range;
using ahirudb::buffer::bridge::add_signed;
using ahirudb::buffer::bridge::checked_i64_from_idx;
using ahirudb::buffer::bridge::checked_idx_from_i64;
using ahirudb::buffer::bridge::merge_size;
using ahirudb::buffer::bridge::move_assign_release;
using ahirudb::buffer::bridge::new_dead_counter;
using ahirudb::buffer::bridge::new_memory_usage;
using ahirudb::buffer::bridge::resize_delta;
using ahirudb::buffer::dead_nodes::DeadNodeCounter;
using ahirudb::buffer::memory_usage::MemoryUsage;

} // namespace rust

} // namespace duckdb