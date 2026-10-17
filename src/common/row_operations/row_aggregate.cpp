#include "row_operations_rust_bridge.hpp"

#include "duckdb/common/vector/flat_vector.hpp"

namespace duckdb {

void RowOperations::InitializeStates(TupleDataLayout &layout, Vector &addresses, const SelectionVector &sel,
                                     idx_t count) {
	auto pointers = FlatVector::GetData<data_ptr_t>(addresses);
	auto offsets = layout.GetOffsets();
	auto aggregates = row_ops_bridge::MakeAggregateSlots(layout);
	row_ops_bridge::CallIntoRust([&]() {
		ahirudb::row_operations::row_aggregate::initialize_states(
		    row_ops_bridge::AggregateOpsTable(),
		    rs_std::SliceRef<const uint64_t>(reinterpret_cast<const uint64_t *>(pointers), addresses.size()),
		    rs_std::SliceRef<const uintptr_t>(reinterpret_cast<const uintptr_t *>(offsets.data()), offsets.size()),
		    layout.ColumnCount(),
		    rs_std::SliceRef<const ahirudb::row_operations::row_aggregate::AggregateSlot>(aggregates.data(),
		                                                                                  aggregates.size()),
		    row_ops_bridge::RustSelEntries(sel, count), count);
	});
}

void RowOperations::DestroyStates(RowOperationsState &state, TupleDataLayout &layout, Vector &addresses) {
	auto aggregates = row_ops_bridge::MakeAggregateSlots(layout);
	row_ops_bridge::CallIntoRust([&]() {
		ahirudb::row_operations::row_aggregate::destroy_states(
		    row_ops_bridge::AggregateOpsTable(), &state, layout.GetAggrOffset(),
		    rs_std::SliceRef<const ahirudb::row_operations::row_aggregate::AggregateSlot>(aggregates.data(),
		                                                                                  aggregates.size()),
		    &addresses);
	});
}

void RowOperations::UpdateStates(RowOperationsState &state, AggregateObject &aggr, Vector &addresses,
                                 DataChunk &payload, idx_t arg_idx, optional_ptr<const ClusteredAggr> clustered) {
	auto slot = row_ops_bridge::MakeAggregateSlot(aggr);
	row_ops_bridge::CallIntoRust([&]() {
		ahirudb::row_operations::row_aggregate::update_states(row_ops_bridge::AggregateOpsTable(), &state, slot,
		                                                      &addresses, &payload, arg_idx, clustered.get());
	});
}

void RowOperations::UpdateFilteredStates(RowOperationsState &state, AggregateFilterData &filter_data,
                                         AggregateObject &aggr, Vector &addresses, DataChunk &payload, idx_t arg_idx) {
	auto slot = row_ops_bridge::MakeAggregateSlot(aggr);
	row_ops_bridge::CallIntoRust([&]() {
		ahirudb::row_operations::row_aggregate::update_filtered_states(
		    row_ops_bridge::AggregateOpsTable(), &state, &filter_data, slot, &addresses, &payload, arg_idx);
	});
}

void RowOperations::UpdateStatesClustered(RowOperationsState &state, vector<AggregateObject> &aggregates,
                                          AggregateFilterDataSet *filter_set, const unsafe_vector<idx_t> *filter,
                                          Vector &addresses, DataChunk &payload, ClusteredAggr &clustered,
                                          bool skip_addresses) {
	auto slots = row_ops_bridge::MakeAggregateSlots(aggregates);
	row_ops_bridge::CallIntoRust([&]() {
		ahirudb::row_operations::row_aggregate::update_states_clustered(
		    row_ops_bridge::AggregateOpsTable(), &state,
		    rs_std::SliceRef<const ahirudb::row_operations::row_aggregate::AggregateSlot>(slots.data(), slots.size()),
		    filter_set,
		    rs_std::SliceRef<const uint64_t>(reinterpret_cast<const uint64_t *>(filter ? filter->data() : nullptr),
		                                     filter ? filter->size() : 0),
		    filter != nullptr, &addresses, &payload, &clustered, skip_addresses);
	});
}

void RowOperations::CombineStates(RowOperationsState &state, TupleDataLayout &layout, Vector &sources,
                                  Vector &targets) {
	auto aggregates = row_ops_bridge::MakeAggregateSlots(layout);
	row_ops_bridge::CallIntoRust([&]() {
		ahirudb::row_operations::row_aggregate::combine_states(
		    row_ops_bridge::AggregateOpsTable(), &state, layout.GetAggrOffset(),
		    rs_std::SliceRef<const ahirudb::row_operations::row_aggregate::AggregateSlot>(aggregates.data(),
		                                                                                  aggregates.size()),
		    &sources, &targets);
	});
}

void RowOperations::FinalizeStates(RowOperationsState &state, TupleDataLayout &layout, Vector &addresses,
                                   DataChunk &result, idx_t aggr_idx) {
	auto aggregates = row_ops_bridge::MakeAggregateSlots(layout);
	row_ops_bridge::CallIntoRust([&]() {
		ahirudb::row_operations::row_aggregate::finalize_states(
		    row_ops_bridge::AggregateOpsTable(), &state, layout.GetAggrOffset(),
		    rs_std::SliceRef<const ahirudb::row_operations::row_aggregate::AggregateSlot>(aggregates.data(),
		                                                                                  aggregates.size()),
		    &addresses, &result, aggr_idx);
	});
}

} // namespace duckdb
