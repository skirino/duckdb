//===----------------------------------------------------------------------===//
//                         DuckDB
//
// vector/vector_rust_bridge.hpp
//
// Internal glue between the C++ vector buffers and the Rust vector module in
// `ahirudb`. Not a public header: the crubit-generated bindings are only included
// from the sources in `src/common/vector`.
//
//===----------------------------------------------------------------------===//

#pragma once

#include "duckdb/common/exception.hpp"
#include "duckdb/common/types/vector.hpp"
#include "duckdb/common/types/vector_buffer.hpp"
#include "duckdb/common/vector/flat_vector.hpp"
#include "duckdb/common/vector/unified_vector_format.hpp"

#include "crubit/ahirudb.h"

#include <cstdint>
#include <string>

namespace duckdb {

//! Turns the error message of a Rust vector kernel into an exception. Defined in
//! vector_rust_bridge.cpp, where `rs::alloc::string::String` is complete.
[[noreturn]] void ThrowVectorError(const ::rs::alloc::string::String &error);

//! A Rust byte slice over a DuckDB buffer. The vector kernels read and write
//! through `data_ptr_t`s, and a `SliceRef` is the boundary type that carries one
//! without either side having to know about the other's allocator.
inline rs_std::SliceRef<const std::uint8_t> RustBytes(const_data_ptr_t data, idx_t byte_count) {
	return rs_std::SliceRef<const std::uint8_t>(reinterpret_cast<const std::uint8_t *>(data), byte_count);
}

//! The mutable form, for the buffers a kernel writes into.
inline rs_std::SliceRef<std::uint8_t> RustBytes(data_ptr_t data, idx_t byte_count) {
	return rs_std::SliceRef<std::uint8_t>(reinterpret_cast<std::uint8_t *>(data), byte_count);
}

//! Rust kernels report a buffer or index that does not fit what they were given
//! as an error, where the C++ they replaced read or wrote out of bounds. Every
//! call site funnels its result through here, so the message is what surfaces,
//! and the throwing happens out of line so the message is formatted once.
template <class T>
T ThrowOnVectorError(rs_std::Result<T, ::rs::alloc::string::String> &&result) {
	if (!result.has_value()) {
		ThrowVectorError(result.err());
	}
	return std::move(result).value();
}

//! The same, for the kernels that return nothing.
inline void ThrowOnVectorError(rs_std::Result<rs_std::unit_t, ::rs::alloc::string::String> &&result) {
	if (!result.has_value()) {
		ThrowVectorError(result.err());
	}
}

//! The fixed-width entry array of `vector`'s buffer, as raw bytes, where each
//! entry is `entry_size` bytes.
//!
//! `VectorBuffer::Copy` reduces its source to a flat or constant vector before it
//! reaches `CopyInternal`, so the source's buffer is the one its rows live in.
//! Only the buffer kinds that keep their rows in an entry array are handed over,
//! which is what `VectorBuffer::GetData` tells apart: only a buffer that
//! overrides it has an address to hand over, and an array buffer overrides
//! `Capacity` but not `GetData`, so its rows have none at this level. Every
//! other kind goes across as an empty slice, so the Rust kernel reports the
//! shortfall instead of reading past the end of the buffer.
inline rs_std::SliceRef<const std::uint8_t> RustEntryBuffer(const Vector &vector, idx_t entry_size) {
	const auto &buffer_ref = vector.GetBufferRef();
	if (!buffer_ref) {
		return rs_std::SliceRef<const std::uint8_t>();
	}
	switch (buffer_ref->GetBufferType()) {
	case VectorBufferType::STANDARD_BUFFER:
	case VectorBufferType::STRING_BUFFER:
	case VectorBufferType::LIST_BUFFER:
		return RustBytes(FlatVector::GetData(vector), buffer_ref->Capacity() * entry_size);
	default:
		return rs_std::SliceRef<const std::uint8_t>();
	}
}

//! `true` when the two byte ranges share a byte. Compared as integers rather than
//! as pointers, because the two need not come from the same allocation.
inline bool RustSpansOverlap(rs_std::SliceRef<const std::uint8_t> lhs, rs_std::SliceRef<const std::uint8_t> rhs) {
	if (lhs.size() == 0 || rhs.size() == 0) {
		return false;
	}
	auto lhs_begin = reinterpret_cast<std::uintptr_t>(lhs.data());
	auto rhs_begin = reinterpret_cast<std::uintptr_t>(rhs.data());
	return lhs_begin < rhs_begin + rhs.size() && rhs_begin < lhs_begin + lhs.size();
}

//! `true` when the copy a `copy_vector_buffer` call would make writes every row
//! over itself: one non-empty buffer, no selection vector, and the same offset
//! on both sides. The `memcpy` this module replaced ran such a copy as a no-op.
//!
//! The target has to be non-empty for the answer to mean anything: a buffer with
//! no capacity has a null `data_ptr`, which is also what an empty slice reports
//! and what a source buffer of a kind that keeps its rows elsewhere reports. Two
//! of those agreeing is not a copy of a buffer onto itself, it is a copy with no
//! room to land in, which is the kernel's error rather than a no-op.
inline bool RustCopyIsIdentity(rs_std::SliceRef<std::uint8_t> target, rs_std::SliceRef<const std::uint8_t> source,
                               const SelectionVector &sel, idx_t source_offset, idx_t target_offset) {
	return target.size() != 0 && target.data() == source.data() && !sel.IsSet() && source_offset == target_offset;
}

//! Refuses a copy whose target and source share their bytes.
//!
//! Crubit's generated thunk calls `CheckNoMutableAliasing` on a `&mut` target
//! beside every `&` source and ends the process with `std::abort()` when the
//! spans it is given touch, so an overlapping copy never reaches the kernel even
//! though the `memcpy` it replaced would have run. `std::abort()` is not a
//! recoverable failure — it lands in the middle of a copy rather than at a
//! boundary anything can catch — so the case is decided here instead.
//!
//! `RustCopyIsIdentity` answers the one form of it DuckDB performs, a copy that
//! writes each row over itself. What is left is refused, and the test is made
//! over the whole buffer spans rather than the rows the copy touches, so it is
//! deliberately the wider of the two: a copy that moves rows between two views
//! of one buffer is refused even where the rows it moves would not have met.
//! Narrowing that to the rows involved would need the selection scanned a
//! second time to bound the source, which is the same cost as the copy itself.
//! Whole-suite instrumentation found no such copy, so the wider test costs
//! nothing today and keeps the failure a query error rather than a wrong answer.
inline void RustRejectOverlappingCopy(rs_std::SliceRef<std::uint8_t> target,
                                      rs_std::SliceRef<const std::uint8_t> source) {
	if (RustSpansOverlap(target, source)) {
		throw InternalException("cannot copy vector data over the rows it is read from");
	}
}

//! A selection vector, or an empty slice when it is not set. Rust reads an empty
//! slice as the identity selection, which is what `SelectionVector::get_index`
//! returns when `sel_vector` is null.
//!
//! `count` is how many rows the caller means to read, which can reach past what
//! the vector holds — the C API takes both from the user. The length handed over
//! is therefore the smaller of the two, so that a request the vector cannot
//! answer is the kernel's error rather than a read past the end. A selection
//! vector that is set but holds no entries at all is refused here instead: it
//! would otherwise go across as the same empty slice as an absent one, and be
//! read as the identity.
inline rs_std::SliceRef<const std::uint32_t> RustSel(const SelectionVector &sel, idx_t count) {
	if (!sel.IsSet()) {
		return rs_std::SliceRef<const std::uint32_t>();
	}
	auto entry_count = MinValue(count, sel.Capacity());
	if (entry_count == 0 && count > 0) {
		throw InternalException("a selection vector that selects %llu rows holds no entries", count);
	}
	return rs_std::SliceRef<const std::uint32_t>(sel.data(), entry_count);
}

//! The write-side form: the selection vector a kernel gathers child rows into.
//!
//! The pointer is what is tested, not the capacity, because a moved-from
//! `SelectionVector` keeps its capacity and drops its pointer.
inline rs_std::SliceRef<std::uint32_t> RustSelMut(SelectionVector &sel) {
	if (!sel.IsSet()) {
		return rs_std::SliceRef<std::uint32_t>();
	}
	return rs_std::SliceRef<std::uint32_t>(sel.data(), sel.Capacity());
}

//! A validity mask as its raw words, or an empty slice when it cannot have
//! nulls. Rust reads an empty slice as "every row is valid", which is what
//! `ValidityMask::RowIsValid` returns when the mask is null.
//!
//! The word count comes from `count`, clamped to what the mask actually holds,
//! for the same reason `RustSel` clamps the entry count.
inline rs_std::SliceRef<const std::uint64_t> RustValidity(const ValidityMask &validity, idx_t count) {
	if (!validity.CanHaveNull()) {
		return rs_std::SliceRef<const std::uint64_t>();
	}
	auto word_count = ValidityMask::EntryCount(MinValue(count, validity.Capacity()));
	return rs_std::SliceRef<const std::uint64_t>(validity.GetData(), word_count);
}

//! The three views a read-only traversal of a vector needs: its entry array, its
//! selection vector and its validity mask, all taken from a single
//! `ToUnifiedFormat` so that they describe the same rows.
//!
//! The entry array's length is the vector's own size times the entry size. That
//! is enough for every read the traversal makes — `VectorIterator` reads
//! `data[sel[i]]` for `i` below `vector.size()`, and every selection vector a
//! `ToUnifiedFormat` produces stays inside the entry array that produced it —
//! and it is not more, which is what makes an out-of-range entry an error rather
//! than a silent read past the end.
struct RustVectorView {
	//! Builds the views for `vector`, whose rows are `entry_size` bytes each.
	explicit RustVectorView(const Vector &vector, idx_t entry_size) {
		vector.ToUnifiedFormat(format);
		auto count = vector.size();
		entries = RustBytes(format.data, count * entry_size);
		selection = RustSel(*format.sel, count);
		validity = RustValidity(format.validity, count);
	}

	//! The unified format the views were taken from; it outlives them.
	UnifiedVectorFormat format;
	//! The entry array, as raw bytes.
	rs_std::SliceRef<const std::uint8_t> entries;
	//! The selection vector the rows are read through.
	rs_std::SliceRef<const std::uint32_t> selection;
	//! The validity mask the rows are read against.
	rs_std::SliceRef<const std::uint64_t> validity;
};

//! The Rust kernels of the vector module, in the shape the C++ call sites use
//! them. Each is named after the C++ function it replaced, and each is reached
//! through the module it lives in rather than a re-export, so there is one
//! spelling of it in the tree.
namespace rust {
using ahirudb::vector::array_vector::array_child_sel;
using ahirudb::vector::flat_vector::copy_vector_buffer;
using ahirudb::vector::list_vector::consecutive_child_list_info;
using ahirudb::vector::list_vector::consecutive_child_sel_vector;
using ahirudb::vector::list_vector::ConsecutiveChildListInfo;
using ahirudb::vector::list_vector::copy_list_entries;
using ahirudb::vector::list_vector::CopyRange;
using ahirudb::vector::list_vector::total_entry_count;
} // namespace rust

} // namespace duckdb
