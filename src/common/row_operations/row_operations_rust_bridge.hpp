//===----------------------------------------------------------------------===//
//                         DuckDB
//
// common/row_operations/row_operations_rust_bridge.hpp
//
// Internal glue between the C++ row operations and the Rust row_operations
// module in `ahirudb`. Not a public header: the crubit-generated bindings are
// only included from the sources in `src/common/row_operations`.
//
//===----------------------------------------------------------------------===//

#pragma once

#include "duckdb/common/exception.hpp"
#include "duckdb/common/row_operations/row_operations.hpp"
#include "duckdb/common/types/row/tuple_data_layout.hpp"

#include "crubit/ahirudb.h"

#include <cstdint>

namespace duckdb {

namespace row_ops_bridge {

//! Turns the error message of a Rust row-operations entry point into an
//! exception. Defined out of line, where `rs::alloc::string::String` is complete.
[[noreturn]] void ThrowRowError(const ::rs::alloc::string::String &error);

//! Marks the [`CallIntoRust`] invocation that a row-operations trampoline error
//! may be attributed to. Mirrors the log-storage bookkeeping.
class CallScope {
public:
	CallScope();
	~CallScope();

private:
	friend void RethrowPendingError(const CallScope &call_scope);

	//! Identifies this call, so that a trampoline error can be attributed to it
	uint64_t index;
	//! The scope that was active before this one, restored on destruction
	uint64_t previous;
};

//! Rethrows the exception that a trampoline caught during `call_scope`, if any.
//! An error that belongs to an earlier call is never rethrown.
void RethrowPendingError(const CallScope &call_scope);
//! Discards the exception a trampoline caught during a call that never came back
//! through `CallIntoRust`.
void ClearPendingError();

//! Runs `fn` and rethrows the exception that a trampoline caught on this thread.
//!
//! The aggregate callbacks and the vector operations are DuckDB C++ code that
//! throws. Letting such an exception unwind out of an `extern "C"` trampoline and
//! through the Rust frames in between is undefined behaviour, so the trampolines
//! catch them and every call into the Rust row-operations module goes through
//! this helper.
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
T ThrowOnRowError(rs_std::Result<T, ::rs::alloc::string::String> &&result) {
	if (!result.has_value()) {
		ThrowRowError(result.err());
	}
	return std::move(result).value();
}

//! A selection vector's entries as the Rust loops read them: the entries
//! themselves, or an empty slice for the identity selection, which is what
//! `SelectionVector::get_index` returns when no selection is set.
//!
//! `count` is how many rows the caller means to read; the length handed over is
//! therefore the smaller of the two, so that a request the selection cannot
//! answer is the kernel's error rather than a read past the end of it.
inline rs_std::SliceRef<const uint32_t> RustSelEntries(const SelectionVector &sel, idx_t count) {
	if (!sel.IsSet()) {
		return rs_std::SliceRef<const uint32_t>();
	}
	auto entry_count = MinValue(count, sel.Capacity());
	if (entry_count == 0 && count > 0) {
		throw InternalException("a selection vector that selects %llu rows holds no entries", count);
	}
	return rs_std::SliceRef<const uint32_t>(sel.data(), entry_count);
}

//! The same, for the entry points that return nothing.
inline void ThrowOnRowError(rs_std::Result<rs_std::unit_t, ::rs::alloc::string::String> &&result) {
	if (!result.has_value()) {
		ThrowRowError(result.err());
	}
}

//! The function table behind the aggregate-state lifecycle, filled once.
const ahirudb::row_operations::row_aggregate::AggregateOps &AggregateOpsTable();

//! The function table behind `RowMatcher`, filled once.
const ahirudb::row_operations::row_matcher::MatcherOps &MatcherOpsTable();

//! Projects one `AggregateObject` into the plain data the Rust lifecycle loops walk.
ahirudb::row_operations::row_aggregate::AggregateSlot MakeAggregateSlot(const AggregateObject &aggr);

//! Projects a `TupleDataLayout`'s aggregate slots into the plain data the Rust
//! lifecycle loops walk.
vector<ahirudb::row_operations::row_aggregate::AggregateSlot> MakeAggregateSlots(const TupleDataLayout &layout);

//! The same, for the `vector<AggregateObject>` the clustered-update path walks.
vector<ahirudb::row_operations::row_aggregate::AggregateSlot>
MakeAggregateSlots(const vector<AggregateObject> &aggregates);

} // namespace row_ops_bridge

} // namespace duckdb
