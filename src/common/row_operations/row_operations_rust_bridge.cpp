//===----------------------------------------------------------------------===//
//                         DuckDB
//
// common/row_operations/row_operations_rust_bridge.cpp
//
// The trampolines that let the Rust row-operations module drive the C++ it
// depends on: the aggregate callbacks, the vector operations, the tuple-data
// gather and the comparison kernels. Every trampoline that can make the C++
// side throw catches the exception instead and records it for
// [`CallIntoRust`], which rethrows it at the C++ entry point.
//
//===----------------------------------------------------------------------===//

#include "row_operations_rust_bridge.hpp"

#include "duckdb/common/clustered_aggregate.hpp"
#include "duckdb/common/exception.hpp"
#include "duckdb/common/helper.hpp"
#include "duckdb/common/operator/comparison_operators.hpp"
#include "duckdb/common/types/data_chunk.hpp"
#include "duckdb/common/types/row/tuple_data_collection.hpp"
#include "duckdb/common/types/row/tuple_data_states.hpp"
#include "duckdb/common/types/selection_vector.hpp"
#include "duckdb/common/types/validity_mask.hpp"
#include "duckdb/common/vector/flat_vector.hpp"
#include "duckdb/common/vector/struct_vector.hpp"
#include "duckdb/common/vector_operations/vector_operations.hpp"
#include "duckdb/function/aggregate_state.hpp"

namespace duckdb {

void ThrowRowError(const ::rs::alloc::string::String &error) {
	throw InternalException("%s", string(std::string_view(error.as_str())).c_str());
}

//===--------------------------------------------------------------------===//
// The call-scope bookkeeping
//===--------------------------------------------------------------------===//
namespace {

//! An exception a failing trampoline caught, together with the [`CallScope`] that produced it
struct PendingRowError {
	std::exception_ptr error;
	uint64_t call_scope = 0;
};

//! The per-thread state the call-scope bookkeeping needs. Mirrors the log-storage bridge: every call into the
//! Rust row-operations module is made on a single thread, so thread-local state is enough.
struct CallScopeState {
	PendingRowError pending_error;
	uint64_t next_call_scope = 0;
	//! The [`CallScope`] that is currently running, or 0 when this thread is not inside one
	uint64_t active_call_scope = 0;
};

CallScopeState &CallScopeStates() {
	static thread_local CallScopeState state;
	return state;
}

PendingRowError &PendingRowErrorSlot() {
	return CallScopeStates().pending_error;
}

//! Runs `fn` on the C++ side, reporting success instead of letting an exception escape
template <class F>
bool CatchRowException(F &&fn) {
	static_assert(std::is_void_v<decltype(fn())>, "CatchRowException only reports whether the call threw");
	// A trampoline whose call is not wrapped in `CallIntoRust` has no way to report its exception back to the
	// C++ code that caused it: the error would be recorded against the enclosing scope, or against none at all,
	// and surface at an unrelated later call
	D_ASSERT(CallScopeStates().active_call_scope != 0);
	try {
		fn();
		return true;
	} catch (...) {
		PendingRowErrorSlot() = PendingRowError {std::current_exception(), CallScopeStates().active_call_scope};
		return false;
	}
}

} // namespace

CallScope::CallScope() : index(++CallScopeStates().next_call_scope), previous(CallScopeStates().active_call_scope) {
	CallScopeStates().active_call_scope = index;
}

CallScope::~CallScope() {
	CallScopeStates().active_call_scope = previous;
}

void RethrowPendingError(const CallScope &call_scope) {
	auto &slot = PendingRowErrorSlot();
	if (!slot.error) {
		return;
	}
	if (slot.call_scope != call_scope.index) {
		// The failure belongs to a call that was not wrapped in `CallIntoRust` (hence scope 0). Re-throwing it
		// here would blame this statement for it, so it is dropped instead.
		slot = PendingRowError();
		return;
	}
	auto error = slot.error;
	slot = PendingRowError();
	std::rethrow_exception(error);
}

void ClearPendingError() {
	PendingRowErrorSlot() = PendingRowError();
}

//===--------------------------------------------------------------------===//
// The layout projection
//===--------------------------------------------------------------------===//
namespace {

vector<ahirudb::row_operations::row_aggregate::AggregateSlot>
MakeAggregateSlots(const vector<AggregateObject> &aggregates) {
	vector<ahirudb::row_operations::row_aggregate::AggregateSlot> result;
	result.reserve(aggregates.size());
	for (const auto &aggr : aggregates) {
		ahirudb::row_operations::row_aggregate::AggregateSlot slot;
		slot.aggr = const_cast<AggregateObject *>(&aggr);
		slot.payload_size = aggr.payload_size;
		slot.child_count = aggr.child_count;
		slot.has_destructor = aggr.function.HasStateDestructorCallback();
		slot.has_cluster_update = aggr.function.GetStateClusterUpdateCallback() != nullptr;
		slot.is_distinct = aggr.aggr_type == AggregateType::DISTINCT;
		slot.has_filter = aggr.filter != nullptr;
		slot.has_combine = aggr.function.HasStateCombineCallback();
		slot.has_init_local_state = aggr.function.GetCallbacks().HasInitLocalStateFinalizeCallback();
		result.push_back(slot);
	}
	return result;
}

} // namespace

vector<ahirudb::row_operations::row_aggregate::AggregateSlot>
row_ops_bridge::MakeAggregateSlots(const TupleDataLayout &layout) {
	return MakeAggregateSlots(layout.GetAggregates());
}

ahirudb::row_operations::row_aggregate::AggregateSlot row_ops_bridge::MakeAggregateSlot(const AggregateObject &aggr) {
	vector<AggregateObject> aggregates {aggr};
	auto slots = MakeAggregateSlots(aggregates);
	return slots[0];
}

//===--------------------------------------------------------------------===//
// The AggregateOps trampolines
//===--------------------------------------------------------------------===//
namespace {

void InitStateTramp(void *aggr_ptr, uintptr_t state_ptr) {
	auto &aggr = *reinterpret_cast<const AggregateObject *>(aggr_ptr);
	CatchRowException([&]() {
		AggregateStateInput state_input(aggr.function, aggr.GetFunctionData());
		aggr.function.GetStateInitCallback()(state_input, reinterpret_cast<data_ptr_t *>(&state_ptr), 1);
	});
}

void DestroyStatesTramp(void *ctx, void *aggr_ptr, void *addresses_ptr, uintptr_t count) {
	auto &state = *reinterpret_cast<RowOperationsState *>(ctx);
	auto &aggr = *reinterpret_cast<const AggregateObject *>(aggr_ptr);
	auto &addresses = *reinterpret_cast<Vector *>(addresses_ptr);
	CatchRowException([&]() {
		AggregateInputData aggr_input_data(aggr, state.allocator);
		aggr.function.GetStateDestructorCallback()(addresses, aggr_input_data, count);
	});
}

void UpdateTramp(void *ctx, void *aggr_ptr, void *inputs, uintptr_t input_count, void *addresses_ptr, uintptr_t count) {
	auto &state = *reinterpret_cast<RowOperationsState *>(ctx);
	auto &aggr = *reinterpret_cast<const AggregateObject *>(aggr_ptr);
	auto &addresses = *reinterpret_cast<Vector *>(addresses_ptr);
	CatchRowException([&]() {
		AggregateInputData aggr_input_data(aggr, state.allocator);
		aggr.function.GetStateUpdateCallback()(reinterpret_cast<Vector *>(inputs), aggr_input_data, aggr.child_count,
		                                       addresses, count);
	});
}

void ClusterUpdateTramp(void *ctx, void *aggr_ptr, void *inputs, uintptr_t input_count, const void *clustered,
                        uintptr_t count) {
	auto &state = *reinterpret_cast<RowOperationsState *>(ctx);
	auto &aggr = *reinterpret_cast<const AggregateObject *>(aggr_ptr);
	CatchRowException([&]() {
		AggregateInputData aggr_input_data(aggr, state.allocator);
		aggr_input_data.clustered = reinterpret_cast<const ClusteredAggr *>(clustered);
		aggr.function.GetStateClusterUpdateCallback()(reinterpret_cast<Vector *>(inputs), aggr_input_data,
		                                              aggr.child_count, *aggr_input_data.clustered, count);
	});
}

void CombineTramp(void *ctx, void *aggr_ptr, void *sources_ptr, void *targets_ptr, uintptr_t count) {
	auto &state = *reinterpret_cast<RowOperationsState *>(ctx);
	auto &aggr = *reinterpret_cast<const AggregateObject *>(aggr_ptr);
	auto &sources = *reinterpret_cast<Vector *>(sources_ptr);
	auto &targets = *reinterpret_cast<Vector *>(targets_ptr);
	CatchRowException([&]() {
		AggregateInputData aggr_input_data(aggr, state.allocator, AggregateCombineType::ALLOW_DESTRUCTIVE);
		aggr.function.GetStateCombineCallback()(sources, targets, aggr_input_data, count);
	});
}

void FinalizeTramp(void *ctx, void *aggr_ptr, void *addresses_ptr, void *local_state, void *target_ptr, uintptr_t count,
                   uintptr_t offset) {
	auto &state = *reinterpret_cast<RowOperationsState *>(ctx);
	auto &aggr = *reinterpret_cast<const AggregateObject *>(aggr_ptr);
	auto &addresses = *reinterpret_cast<Vector *>(addresses_ptr);
	auto &target = *reinterpret_cast<Vector *>(target_ptr);
	CatchRowException([&]() {
		AggregateFinalizeInputData finalize_input_data(aggr, state.allocator,
		                                               reinterpret_cast<FunctionLocalState *>(local_state));
		aggr.function.GetStateFinalizeCallback()(addresses, finalize_input_data, target, count, offset);
	});
}

void AddInPlaceTramp(void *vector_ptr, int64_t delta) {
	CatchRowException([&]() { VectorOperations::AddInPlace(*reinterpret_cast<Vector *>(vector_ptr), delta); });
}

void VectorCopyTramp(void *source_ptr, void *target_ptr, uintptr_t count) {
	CatchRowException([&]() {
		VectorOperations::Copy(*reinterpret_cast<Vector *>(source_ptr), *reinterpret_cast<Vector *>(target_ptr), count,
		                       0, 0);
	});
}

void SetVectorSizeTramp(void *vector_ptr, uintptr_t size) {
	CatchRowException([&]() { FlatVector::SetSize(*reinterpret_cast<Vector *>(vector_ptr), size); });
}

uintptr_t VectorSizeTramp(void *vector_ptr) {
	uintptr_t result = 0;
	CatchRowException([&]() { result = reinterpret_cast<Vector *>(vector_ptr)->size(); });
	return result;
}

void *GetOrCreateAddressesTramp(void *ctx) {
	auto &state = *reinterpret_cast<RowOperationsState *>(ctx);
	Vector *result = nullptr;
	CatchRowException([&]() {
		if (!state.addresses) {
			state.addresses = make_uniq<Vector>(LogicalType::POINTER);
		}
		result = state.addresses.get();
	});
	// A null return is how the trampoline reports the caught exception to Rust; `CallIntoRust` rethrows it
	return result;
}

uintptr_t ApplyFilterTramp(void *filter_data_ptr, void *payload_ptr) {
	auto &filter_data = *reinterpret_cast<AggregateFilterData *>(filter_data_ptr);
	auto &payload = *reinterpret_cast<DataChunk *>(payload_ptr);
	uintptr_t result = 0;
	CatchRowException([&]() { result = filter_data.ApplyFilter(payload); });
	return result;
}

void *MakeFilteredAddressesTramp(void *addresses_ptr, void *filter_data_ptr, uintptr_t count) {
	auto &addresses = *reinterpret_cast<Vector *>(addresses_ptr);
	auto &filter_data = *reinterpret_cast<AggregateFilterData *>(filter_data_ptr);
	Vector *result = nullptr;
	CatchRowException([&]() {
		Vector filtered_addresses(addresses, filter_data.true_sel, count);
		filtered_addresses.Flatten();
		// Allocated on the heap: the Rust side frees it through `FreeVectorTramp` when the update is done
		result = new Vector(std::move(filtered_addresses));
	});
	return result;
}

void *FilteredPayloadTramp(void *filter_data_ptr) {
	auto &filter_data = *reinterpret_cast<AggregateFilterData *>(filter_data_ptr);
	return &filter_data.filtered_payload;
}

void FreeVectorTramp(void *vector_ptr) {
	delete reinterpret_cast<Vector *>(vector_ptr);
}

void *GetFilterDataTramp(void *filter_set_ptr, uintptr_t aggr_idx) {
	auto &filter_set = *reinterpret_cast<AggregateFilterDataSet *>(filter_set_ptr);
	return &filter_set.GetFilterData(aggr_idx);
}

void ClusteredAdvanceTramp(void *clustered_ptr, uintptr_t payload_size) {
	CatchRowException([&]() { reinterpret_cast<ClusteredAggr *>(clustered_ptr)->AdvanceStates(payload_size); });
}

void *ChunkChildVectorTramp(void *chunk_ptr, uintptr_t idx) {
	return &reinterpret_cast<DataChunk *>(chunk_ptr)->data[idx];
}

uintptr_t ChunkSizeTramp(void *chunk_ptr) {
	uintptr_t result = 0;
	CatchRowException([&]() { result = reinterpret_cast<DataChunk *>(chunk_ptr)->size(); });
	return result;
}

uintptr_t LocalStateCountTramp(void *ctx) {
	return reinterpret_cast<RowOperationsState *>(ctx)->local_states.size();
}

void ResizeLocalStatesTramp(void *ctx, uintptr_t n) {
	auto &state = *reinterpret_cast<RowOperationsState *>(ctx);
	CatchRowException([&]() { state.local_states.resize(n); });
}

void InitLocalStateTramp(void *ctx, void *aggr_ptr, uintptr_t idx) {
	auto &state = *reinterpret_cast<RowOperationsState *>(ctx);
	auto &aggr = *reinterpret_cast<const AggregateObject *>(aggr_ptr);
	CatchRowException([&]() {
		auto &callbacks = aggr.function.GetCallbacks();
		AggregateInputData aggr_input_data(aggr, state.allocator);
		state.local_states[idx] =
		    callbacks.GetInitLocalStateFinalizeCallback()(aggr_input_data.function, aggr_input_data.bind_data);
	});
}

void *LocalStatePtrTramp(void *ctx, uintptr_t idx) {
	return reinterpret_cast<RowOperationsState *>(ctx)->local_states[idx].get();
}

} // namespace

const ahirudb::row_operations::row_aggregate::AggregateOps &row_ops_bridge::AggregateOpsTable() {
	static const auto table = ahirudb::row_operations::row_aggregate::AggregateOps::new_(
	    InitStateTramp, DestroyStatesTramp, UpdateTramp, ClusterUpdateTramp, CombineTramp, FinalizeTramp,
	    AddInPlaceTramp, VectorCopyTramp, SetVectorSizeTramp, VectorSizeTramp, GetOrCreateAddressesTramp,
	    ApplyFilterTramp, MakeFilteredAddressesTramp, FilteredPayloadTramp, FreeVectorTramp, GetFilterDataTramp,
	    ClusteredAdvanceTramp, ChunkChildVectorTramp, ChunkSizeTramp, LocalStateCountTramp, ResizeLocalStatesTramp,
	    InitLocalStateTramp, LocalStatePtrTramp);
	return table;
}

//===--------------------------------------------------------------------===//
// The MatcherOps trampolines
//===--------------------------------------------------------------------===//
namespace {

using ValidityBytes = TupleDataLayout::ValidityBytes;

uintptr_t LayoutColumnCountTramp(const void *layout_ptr) {
	if (!layout_ptr) {
		return 0;
	}
	return reinterpret_cast<const TupleDataLayout *>(layout_ptr)->ColumnCount();
}

bool LayoutCanHaveNullTramp(const void *layout_ptr) {
	if (!layout_ptr) {
		return false;
	}
	return reinterpret_cast<const TupleDataLayout *>(layout_ptr)->CanHaveNull();
}

uintptr_t LayoutOffsetTramp(const void *layout_ptr, uintptr_t col_idx) {
	if (!layout_ptr) {
		return 0;
	}
	return reinterpret_cast<const TupleDataLayout *>(layout_ptr)->GetOffsets()[col_idx];
}

const void *LayoutTypeTramp(const void *layout_ptr, uintptr_t col_idx) {
	if (!layout_ptr) {
		return nullptr;
	}
	auto &types = reinterpret_cast<const TupleDataLayout *>(layout_ptr)->GetTypes();
	if (col_idx >= types.size()) {
		// Out-of-range columns are a caller bug the C++ asserted on; the null handle makes the Rust
		// node construction fail instead of reading past the type vector
		return nullptr;
	}
	return &types[col_idx];
}

const void *LayoutStructTramp(const void *layout_ptr, uintptr_t col_idx) {
	if (!layout_ptr) {
		return nullptr;
	}
	auto &layout = *reinterpret_cast<const TupleDataLayout *>(layout_ptr);
	const TupleDataLayout *result = nullptr;
	CatchRowException([&]() { result = &layout.GetStructLayout(col_idx); });
	return result;
}

uint8_t TypePhysicalTramp(const void *ty_ptr) {
	if (!ty_ptr) {
		return 0;
	}
	return static_cast<uint8_t>(reinterpret_cast<const LogicalType *>(ty_ptr)->InternalType());
}

bool TypeIsUnionTramp(const void *ty_ptr) {
	if (!ty_ptr) {
		return false;
	}
	return reinterpret_cast<const LogicalType *>(ty_ptr)->id() == LogicalTypeId::UNION;
}

uintptr_t TypeStructChildCountTramp(const void *ty_ptr) {
	if (!ty_ptr) {
		return 0;
	}
	return StructType::GetChildCount(*reinterpret_cast<const LogicalType *>(ty_ptr));
}

const void *TypeStructChildTramp(const void *ty_ptr, uintptr_t idx) {
	if (!ty_ptr) {
		return nullptr;
	}
	auto &children = StructType::GetChildTypes(*reinterpret_cast<const LogicalType *>(ty_ptr));
	if (idx >= children.size()) {
		return nullptr;
	}
	return &children[idx].second;
}

const void *LhsFormatAtTramp(const void *formats_ptr, uintptr_t col_idx) {
	if (!formats_ptr) {
		return nullptr;
	}
	auto &formats = *reinterpret_cast<const vector<TupleDataVectorFormat> *>(formats_ptr);
	if (col_idx >= formats.size()) {
		return nullptr;
	}
	return &formats[col_idx];
}

const void *LhsFormatChildTramp(const void *format_ptr, uintptr_t col_idx) {
	if (!format_ptr) {
		return nullptr;
	}
	auto &children = reinterpret_cast<const TupleDataVectorFormat *>(format_ptr)->children;
	if (col_idx >= children.size()) {
		return nullptr;
	}
	return &children[col_idx];
}

bool LhsCannotHaveNullTramp(const void *format_ptr) {
	if (!format_ptr) {
		return true;
	}
	return reinterpret_cast<const TupleDataVectorFormat *>(format_ptr)->unified.validity.CannotHaveNull();
}

bool LhsRowIsNullTramp(const void *format_ptr, uintptr_t idx) {
	if (!format_ptr) {
		return false;
	}
	return !reinterpret_cast<const TupleDataVectorFormat *>(format_ptr)->unified.validity.RowIsValidUnsafe(idx);
}

//! `TemplatedMatchLoop`'s per-row comparison, monomorphized over the physical type and the predicate
template <class T, class OP>
bool CompareRowT(const TupleDataVectorFormat &format, idx_t idx, uintptr_t rhs_ptr, bool rhs_null, bool lhs_all_valid) {
	using COMPARISON_OP = ComparisonOperationWrapper<OP>;

	// LHS
	const auto &lhs_sel = *format.unified.sel;
	const auto lhs_data = UnifiedVectorFormat::GetData<T>(format.unified);
	const auto &lhs_validity = format.unified.validity;

	const auto lhs_idx = lhs_sel.get_index(idx);
	return COMPARISON_OP::template Operation<T>(lhs_data[lhs_idx], Load<T>(reinterpret_cast<const_data_ptr_t>(rhs_ptr)),
	                                            lhs_all_valid ? false : !lhs_validity.RowIsValidUnsafe(lhs_idx),
	                                            rhs_null);
}

template <class OP>
bool CompareRowOp(const TupleDataVectorFormat &format, idx_t idx, uintptr_t rhs_ptr, bool rhs_null, bool lhs_all_valid,
                  PhysicalType physical) {
	switch (physical) {
	case PhysicalType::BOOL:
		return CompareRowT<bool, OP>(format, idx, rhs_ptr, rhs_null, lhs_all_valid);
	case PhysicalType::INT8:
		return CompareRowT<int8_t, OP>(format, idx, rhs_ptr, rhs_null, lhs_all_valid);
	case PhysicalType::INT16:
		return CompareRowT<int16_t, OP>(format, idx, rhs_ptr, rhs_null, lhs_all_valid);
	case PhysicalType::INT32:
		return CompareRowT<int32_t, OP>(format, idx, rhs_ptr, rhs_null, lhs_all_valid);
	case PhysicalType::INT64:
		return CompareRowT<int64_t, OP>(format, idx, rhs_ptr, rhs_null, lhs_all_valid);
	case PhysicalType::INT128:
		return CompareRowT<hugeint_t, OP>(format, idx, rhs_ptr, rhs_null, lhs_all_valid);
	case PhysicalType::UINT8:
		return CompareRowT<uint8_t, OP>(format, idx, rhs_ptr, rhs_null, lhs_all_valid);
	case PhysicalType::UINT16:
		return CompareRowT<uint16_t, OP>(format, idx, rhs_ptr, rhs_null, lhs_all_valid);
	case PhysicalType::UINT32:
		return CompareRowT<uint32_t, OP>(format, idx, rhs_ptr, rhs_null, lhs_all_valid);
	case PhysicalType::UINT64:
		return CompareRowT<uint64_t, OP>(format, idx, rhs_ptr, rhs_null, lhs_all_valid);
	case PhysicalType::UINT128:
		return CompareRowT<uhugeint_t, OP>(format, idx, rhs_ptr, rhs_null, lhs_all_valid);
	case PhysicalType::FLOAT:
		return CompareRowT<float, OP>(format, idx, rhs_ptr, rhs_null, lhs_all_valid);
	case PhysicalType::DOUBLE:
		return CompareRowT<double, OP>(format, idx, rhs_ptr, rhs_null, lhs_all_valid);
	case PhysicalType::INTERVAL:
		return CompareRowT<interval_t, OP>(format, idx, rhs_ptr, rhs_null, lhs_all_valid);
	case PhysicalType::VARCHAR:
		return CompareRowT<string_t, OP>(format, idx, rhs_ptr, rhs_null, lhs_all_valid);
	default:
		// Unreachable: the Rust node construction only builds primitive nodes for the types above
		return false;
	}
}

bool CompareRowTramp(const void *lhs_format_ptr, uintptr_t idx, uint8_t predicate, uint8_t physical, uintptr_t rhs_ptr,
                     bool rhs_null, bool lhs_all_valid) {
	if (!lhs_format_ptr) {
		return false;
	}
	const auto &format = *reinterpret_cast<const TupleDataVectorFormat *>(lhs_format_ptr);
	bool result = false;
	CatchRowException([&]() {
		switch (predicate) {
		case ExpressionType::COMPARE_EQUAL:
			result = CompareRowOp<Equals>(format, idx, rhs_ptr, rhs_null, lhs_all_valid,
			                              static_cast<PhysicalType>(physical));
			break;
		case ExpressionType::COMPARE_NOTEQUAL:
			result = CompareRowOp<NotEquals>(format, idx, rhs_ptr, rhs_null, lhs_all_valid,
			                                 static_cast<PhysicalType>(physical));
			break;
		case ExpressionType::COMPARE_DISTINCT_FROM:
			result = CompareRowOp<DistinctFrom>(format, idx, rhs_ptr, rhs_null, lhs_all_valid,
			                                    static_cast<PhysicalType>(physical));
			break;
		case ExpressionType::COMPARE_NOT_DISTINCT_FROM:
			result = CompareRowOp<NotDistinctFrom>(format, idx, rhs_ptr, rhs_null, lhs_all_valid,
			                                       static_cast<PhysicalType>(physical));
			break;
		case ExpressionType::COMPARE_GREATERTHAN:
			result = CompareRowOp<GreaterThan>(format, idx, rhs_ptr, rhs_null, lhs_all_valid,
			                                   static_cast<PhysicalType>(physical));
			break;
		case ExpressionType::COMPARE_GREATERTHANOREQUALTO:
			result = CompareRowOp<GreaterThanEquals>(format, idx, rhs_ptr, rhs_null, lhs_all_valid,
			                                         static_cast<PhysicalType>(physical));
			break;
		case ExpressionType::COMPARE_LESSTHAN:
			result = CompareRowOp<LessThan>(format, idx, rhs_ptr, rhs_null, lhs_all_valid,
			                                static_cast<PhysicalType>(physical));
			break;
		case ExpressionType::COMPARE_LESSTHANOREQUALTO:
			result = CompareRowOp<LessThanEquals>(format, idx, rhs_ptr, rhs_null, lhs_all_valid,
			                                      static_cast<PhysicalType>(physical));
			break;
		default:
			// Unreachable: the Rust node construction only builds nodes for the comparison predicates
			result = false;
			break;
		}
	});
	return result;
}

bool RhsRowIsNullTramp(const void *layout_ptr, uintptr_t location, uintptr_t col_idx, uintptr_t column_count) {
	if (!layout_ptr) {
		return false;
	}
	auto &layout = *reinterpret_cast<const TupleDataLayout *>(layout_ptr);
	idx_t entry_idx;
	idx_t idx_in_entry;
	ValidityBytes::GetEntryIndex(col_idx, entry_idx, idx_in_entry);
	return !ValidityBytes::RowIsValid(
	    ValidityBytes(reinterpret_cast<data_ptr_t>(location), column_count).GetValidityEntryUnsafe(entry_idx),
	    idx_in_entry);
}

uintptr_t RhsLocationAtTramp(void *row_locations_ptr, uintptr_t idx) {
	if (!row_locations_ptr) {
		return 0;
	}
	return FlatVector::GetData<data_ptr_t>(*reinterpret_cast<Vector *>(row_locations_ptr))[idx];
}

bool StructNullKeepTramp(uint8_t predicate, bool lhs_null, bool rhs_null) {
	switch (predicate) {
	case ExpressionType::COMPARE_EQUAL:
		return ComparisonOperationWrapper<Equals>::COMPARE_NULL &&
		       ComparisonOperationWrapper<Equals>::template Operation<uint32_t>(0, 0, lhs_null, rhs_null);
	case ExpressionType::COMPARE_NOTEQUAL:
		return ComparisonOperationWrapper<NotEquals>::COMPARE_NULL &&
		       ComparisonOperationWrapper<NotEquals>::template Operation<uint32_t>(0, 0, lhs_null, rhs_null);
	case ExpressionType::COMPARE_DISTINCT_FROM:
		return ComparisonOperationWrapper<DistinctFrom>::COMPARE_NULL &&
		       ComparisonOperationWrapper<DistinctFrom>::template Operation<uint32_t>(0, 0, lhs_null, rhs_null);
	case ExpressionType::COMPARE_NOT_DISTINCT_FROM:
		return ComparisonOperationWrapper<NotDistinctFrom>::COMPARE_NULL &&
		       ComparisonOperationWrapper<NotDistinctFrom>::template Operation<uint32_t>(0, 0, lhs_null, rhs_null);
	case ExpressionType::COMPARE_GREATERTHAN:
		return ComparisonOperationWrapper<GreaterThan>::COMPARE_NULL &&
		       ComparisonOperationWrapper<GreaterThan>::template Operation<uint32_t>(0, 0, lhs_null, rhs_null);
	case ExpressionType::COMPARE_GREATERTHANOREQUALTO:
		return ComparisonOperationWrapper<GreaterThanEquals>::COMPARE_NULL &&
		       ComparisonOperationWrapper<GreaterThanEquals>::template Operation<uint32_t>(0, 0, lhs_null, rhs_null);
	case ExpressionType::COMPARE_LESSTHAN:
		return ComparisonOperationWrapper<LessThan>::COMPARE_NULL &&
		       ComparisonOperationWrapper<LessThan>::template Operation<uint32_t>(0, 0, lhs_null, rhs_null);
	case ExpressionType::COMPARE_LESSTHANOREQUALTO:
		return ComparisonOperationWrapper<LessThanEquals>::COMPARE_NULL &&
		       ComparisonOperationWrapper<LessThanEquals>::template Operation<uint32_t>(0, 0, lhs_null, rhs_null);
	default:
		return false;
	}
}

void *BuildStructRowLocationsTramp(void *row_locations_ptr, const uint32_t *sel, uintptr_t match_count,
                                   uintptr_t offset) {
	auto result = new Vector(LogicalType::POINTER);
	CatchRowException([&]() {
		const auto rhs_locations = FlatVector::GetData<data_ptr_t>(*reinterpret_cast<Vector *>(row_locations_ptr));
		auto rhs_struct_locations = FlatVector::ScatterWriter<data_ptr_t>(*result);
		for (uintptr_t i = 0; i < match_count; i++) {
			const auto idx = sel[i];
			rhs_struct_locations[idx] = rhs_locations[idx] + offset;
		}
	});
	// A null return is how the trampoline reports the caught exception to Rust; `CallIntoRust` rethrows it
	return result;
}

const void *StructEntriesTramp(void *lhs_vector_ptr) {
	if (!lhs_vector_ptr) {
		return nullptr;
	}
	return &StructVector::GetEntries(*reinterpret_cast<Vector *>(lhs_vector_ptr));
}

void *StructEntryTramp(const void *entries_ptr, uintptr_t idx) {
	if (!entries_ptr) {
		return nullptr;
	}
	auto &entries = *reinterpret_cast<const vector<Vector> *>(entries_ptr);
	if (idx >= entries.size()) {
		return nullptr;
	}
	return &const_cast<Vector &>(entries[idx]);
}

//! `SelectComparison`'s operand mapping, mirroring the C++ template specializations
uintptr_t SelectComparisonTramp(uint8_t predicate, const Vector &left, const Vector &right, const SelectionVector &sel,
                                idx_t count, SelectionVector *true_sel, SelectionVector *false_sel) {
	switch (predicate) {
	case ExpressionType::COMPARE_EQUAL:
		return VectorOperations::Equals(left, right, &sel, count, true_sel, false_sel);
	case ExpressionType::COMPARE_NOTEQUAL:
		return VectorOperations::NotEquals(left, right, &sel, count, true_sel, false_sel);
	case ExpressionType::COMPARE_DISTINCT_FROM:
		return VectorOperations::DistinctFrom(left, right, &sel, count, true_sel, false_sel);
	case ExpressionType::COMPARE_NOT_DISTINCT_FROM:
		return VectorOperations::NotDistinctFrom(left, right, &sel, count, true_sel, false_sel);
	case ExpressionType::COMPARE_GREATERTHAN:
		return VectorOperations::DistinctGreaterThan(left, right, &sel, count, true_sel, false_sel);
	case ExpressionType::COMPARE_GREATERTHANOREQUALTO:
		return VectorOperations::DistinctGreaterThanEquals(left, right, &sel, count, true_sel, false_sel);
	case ExpressionType::COMPARE_LESSTHAN:
		return VectorOperations::DistinctLessThan(left, right, &sel, count, true_sel, false_sel);
	case ExpressionType::COMPARE_LESSTHANOREQUALTO:
		return VectorOperations::DistinctLessThanEquals(left, right, &sel, count, true_sel, false_sel);
	default:
		throw NotImplementedException("Unsupported list comparison operand for RowMatcher::GetMatchFunction");
	}
}

uintptr_t GenericNestedMatchTramp(uint8_t predicate, const void *rhs_layout_ptr, uintptr_t col_idx,
                                  void *lhs_vector_ptr, void *rhs_row_locations_ptr, uint32_t *sel, uintptr_t count,
                                  uint32_t *no_match_sel, uintptr_t no_match_sel_capacity, uintptr_t *no_match_count) {
	if (!lhs_vector_ptr || !rhs_row_locations_ptr || !rhs_layout_ptr) {
		return 0;
	}
	auto &lhs_vector = *reinterpret_cast<Vector *>(lhs_vector_ptr);
	auto &rhs_row_locations = *reinterpret_cast<Vector *>(rhs_row_locations_ptr);
	auto &rhs_layout = *reinterpret_cast<const TupleDataLayout *>(rhs_layout_ptr);
	uintptr_t match_count = 0;
	CatchRowException([&]() {
		const auto &type = rhs_layout.GetTypes()[col_idx];

		// Gather a dense Vector containing the column values being matched
		Vector key(type);
		const auto gather_function = TupleDataCollection::GetGatherFunction(type);
		SelectionVector sel_view(sel, count);
		gather_function.Gather(rhs_row_locations, rhs_layout, col_idx, sel_view, count, key,
		                       *FlatVector::IncrementalSelectionVector(), nullptr);
		key.Verify();

		// Densify the input column
		Vector sliced(lhs_vector, sel_view, count);

		if (no_match_sel) {
			SelectionVector no_match_sel_offset(no_match_sel, no_match_sel_capacity);
			match_count =
			    SelectComparisonTramp(predicate, sliced, key, sel_view, count, &sel_view, &no_match_sel_offset);
			*no_match_count += count - match_count;
		} else {
			match_count = SelectComparisonTramp(predicate, sliced, key, sel_view, count, &sel_view, nullptr);
		}
	});
	return match_count;
}

void *ChunkChildVectorMatcherTramp(void *chunk_ptr, uintptr_t col_idx) {
	if (!chunk_ptr) {
		return nullptr;
	}
	return &reinterpret_cast<DataChunk *>(chunk_ptr)->data[col_idx];
}

} // namespace

const ahirudb::row_operations::row_matcher::MatcherOps &row_ops_bridge::MatcherOpsTable() {
	static const auto table = ahirudb::row_operations::row_matcher::MatcherOps::new_(
	    LayoutColumnCountTramp, LayoutCanHaveNullTramp, LayoutOffsetTramp, LayoutTypeTramp, LayoutStructTramp,
	    TypePhysicalTramp, TypeIsUnionTramp, TypeStructChildCountTramp, TypeStructChildTramp, LhsFormatAtTramp,
	    LhsFormatChildTramp, LhsCannotHaveNullTramp, LhsRowIsNullTramp, CompareRowTramp, RhsRowIsNullTramp,
	    RhsLocationAtTramp, StructNullKeepTramp, BuildStructRowLocationsTramp, FreeVectorTramp, StructEntriesTramp,
	    StructEntryTramp, GenericNestedMatchTramp, ChunkChildVectorMatcherTramp);
	return table;
}

} // namespace duckdb
