#include "duckdb/common/sorting/sort.hpp"
#include "duckdb/main/client_context.hpp"

#include "duckdb/main/settings.hpp"
#include "duckdb/common/type_visitor.hpp"
#include "duckdb/common/sorting/sort_key.hpp"
#include "sort_rust_bridge.hpp"
#include "duckdb/common/sorting/sorted_run.hpp"
#include "duckdb/common/sorting/sorted_run_merger.hpp"
#include "duckdb/common/types/batched_data_collection.hpp"
#include "duckdb/common/types/row/tuple_data_collection.hpp"
#include "duckdb/function/create_sort_key.hpp"
#include "duckdb/function/function_binder.hpp"
#include "duckdb/planner/expression/bound_constant_expression.hpp"
#include "duckdb/planner/expression/bound_reference_expression.hpp"
#include "duckdb/storage/temporary_memory_manager.hpp"

namespace duckdb {

Sort::Sort(ClientContext &client_context_p, const vector<BoundOrderByNode> &orders,
           const vector<LogicalType> &input_types, vector<idx_t> projection_map, bool is_index_sort_p)
    : client_context(client_context_p), key_layout(make_shared_ptr<TupleDataLayout>()),
      payload_layout(make_shared_ptr<TupleDataLayout>()), is_index_sort(is_index_sort_p) {
	// Convert orders to a single "create_sort_key" expression (and corresponding "decode_sort_key")
	FunctionBinder binder(client_context);
	vector<unique_ptr<Expression>> create_children;
	vector<unique_ptr<Expression>> decode_children;
	child_list_t<LogicalType> decode_child_list;
	for (idx_t col_idx = 0; col_idx < orders.size(); col_idx++) {
		const auto &order = orders[col_idx];

		// Create: for each column we have two arguments: 1. the column, 2. sort specifier
		create_children.emplace_back(order.expression->Copy());
		create_children.emplace_back(make_uniq<BoundConstantExpression>(Value(order.GetOrderModifier())));

		// Avoid having unnamed structs fields (otherwise we get a parser exception while binding)
		const auto col_name = StringUtil::Format("c%llu", col_idx);
		auto col_type = order.expression->GetReturnType();
		decode_child_list.emplace_back(col_name, col_type);
		col_type = TypeVisitor::VisitReplace(col_type, [](const LogicalType &type) {
			if (!StructType::IsStruct(type)) {
				return type;
			}
			child_list_t<LogicalType> internal_child_list;
			for (const auto &child : StructType::GetChildTypes(type)) {
				internal_child_list.emplace_back(StringUtil::Format("c%llu", internal_child_list.size()), child.second);
			}
			return LogicalType::STRUCT(std::move(internal_child_list));
		});

		// Decode: for each column we have two arguments: 1. col name + type, 2. sort specifier
		decode_children.emplace_back(make_uniq<BoundConstantExpression>(Value(col_name + " " + col_type.ToString())));
		decode_children.emplace_back(make_uniq<BoundConstantExpression>(order.GetOrderModifier()));
	}

	ErrorData error;
	create_sort_key =
	    binder.BindScalarFunction(Identifier::DefaultSchema(), "create_sort_key", std::move(create_children), error);
	if (!create_sort_key) {
		throw InternalException("Unable to bind create_sort_key in Sort::Sort");
	}

	switch (create_sort_key->GetReturnType().id()) {
	case LogicalTypeId::BIGINT:
		decode_children.insert(decode_children.begin(),
		                       make_uniq<BoundReferenceExpression>(LogicalType::BIGINT, static_cast<storage_t>(0)));
		break;
	default:
		D_ASSERT(create_sort_key->GetReturnType().id() == LogicalTypeId::BLOB);
		decode_children.insert(decode_children.begin(),
		                       make_uniq<BoundReferenceExpression>(LogicalType::BLOB, static_cast<storage_t>(0)));
	}

	decode_sort_key = binder.BindScalarFunction(DecodeSortKeyFun::GetFunction(), std::move(decode_children));
	if (!decode_sort_key) {
		throw InternalException("Unable to bind decode_sort_key in Sort::Sort");
	}

	// A bit hacky, but this way we make sure that the output does contain the unnamed structs again
	decode_sort_key->SetReturnType(LogicalType::STRUCT(std::move(decode_child_list)));

	// For convenience, we fill the projection map if it is empty
	if (projection_map.empty()) {
		projection_map.reserve(input_types.size());
		for (idx_t col_idx = 0; col_idx < input_types.size(); col_idx++) {
			projection_map.push_back(col_idx);
		}
	}

	// We need to output this many columns, reserve
	output_projection_columns.reserve(projection_map.size());

	// Build order_input_columns for Rust: input column each order reads, or INVALID_INDEX.
	vector<uint64_t> order_input_columns;
	order_input_columns.reserve(orders.size());
	for (const auto &order : orders) {
		const auto &expr = *order.expression;
		if (expr.GetExpressionClass() == ExpressionClass::BOUND_REF) {
			order_input_columns.push_back(expr.Cast<BoundReferenceExpression>().Index());
		} else {
			order_input_columns.push_back(ahirudb::sort::sort::INVALID_INDEX);
		}
	}

	vector<uint64_t> projection_columns_flat(projection_map.size() * 3, 0);
	vector<uint64_t> input_projection_map_buf(projection_map.size(), 0);

	auto layout = ahirudb::sort::sort::derive_sort_layout(
	    rs_std::SliceRef<const uint64_t>(order_input_columns.data(), order_input_columns.size()),
	    rs_std::SliceRef<const uint64_t>(reinterpret_cast<const uint64_t *>(projection_map.data()),
	                                     projection_map.size()),
	    rs_std::SliceRef<uint64_t>(projection_columns_flat.data(), projection_columns_flat.size()),
	    rs_std::SliceRef<uint64_t>(input_projection_map_buf.data(), input_projection_map_buf.size()));

	const idx_t payload_count = layout.payload_column_count;

	// Construct payload layout
	vector<LogicalType> payload_types;
	payload_types.reserve(payload_count);
	for (idx_t i = 0; i < payload_count; i++) {
		payload_types.push_back(input_types[input_projection_map_buf[i]]);
	}
	payload_layout->Initialize(payload_types, TupleDataValidityType::CAN_HAVE_NULL_VALUES);

	// Reconstruct output_projection_columns from Rust output
	output_projection_columns.clear();
	output_projection_columns.reserve(projection_map.size());
	for (idx_t i = 0; i < projection_map.size(); i++) {
		SortProjectionColumn opc;
		opc.is_payload = projection_columns_flat[i * 3] != 0;
		opc.layout_col_idx = projection_columns_flat[i * 3 + 1];
		opc.output_col_idx = projection_columns_flat[i * 3 + 2];
		output_projection_columns.push_back(opc);
	}
	input_projection_map.clear();
	input_projection_map.reserve(payload_count);
	for (idx_t i = 0; i < payload_count; i++) {
		input_projection_map.push_back(input_projection_map_buf[i]);
	}

	// Finally, initialize the key layout (now that we know whether we have a payload)
	key_layout->Initialize(orders, create_sort_key->GetReturnType(), !payload_types.empty());
}

//===--------------------------------------------------------------------===//
// Sink
//===--------------------------------------------------------------------===//
class SortLocalSinkState : public LocalSinkState {
public:
	SortLocalSinkState(const Sort &sort, ClientContext &context)
	    : maximum_run_size(0), external(false), key_executor(context, *sort.create_sort_key) {
		key.Initialize(context, {sort.create_sort_key->GetReturnType()});
		payload.Initialize(context, sort.payload_layout->GetTypes());
	}

public:
	void InitializeSortedRun(const Sort &sort, ClientContext &context) {
		D_ASSERT(!sorted_run);
		sorted_run = make_uniq<SortedRun>(context, sort, false);
	}

public:
	//! The sorted run that we're appending to
	unique_ptr<SortedRun> sorted_run;
	//! The current maximum run size (in bytes, retrieved from global state)
	idx_t maximum_run_size;
	//! Whether this is an external sort (retrieved from global state)
	bool external;

	ExpressionExecutor key_executor;
	DataChunk key;
	DataChunk payload;
};

class SortGlobalSinkState : public GlobalSinkState {
public:
	explicit SortGlobalSinkState(ClientContext &context)
	    : num_threads(TaskScheduler::GetScheduler(context).NumberOfThreads()),
	      temporary_memory_state(TemporaryMemoryManager::Get(context).Register(context)), sorted_tuples(0),
	      external(Settings::Get<DebugForceExternalSetting>(context)), any_combined(false), total_count(0),
	      partition_size(0) {
	}

public:
	void UpdateLocalState(SortLocalSinkState &lstate) const {
		lstate.maximum_run_size = temporary_memory_state->GetReservation() / num_threads;
		lstate.external = external;
	}

	void TryIncreaseReservation(ClientContext &context, SortLocalSinkState &lstate, bool is_index_sort,
	                            const unique_lock<mutex> &guard) {
		D_ASSERT(!external);

		// If we already got less than we requested last time, have to go external
		if (temporary_memory_state->GetReservation() < temporary_memory_state->GetRemainingSize()) {
			if (!any_combined) {
				external = true;
			}
			return;
		}

		// Double until it fits
		auto required = num_threads * lstate.sorted_run->SizeInBytes();
		if (is_index_sort) {
			required *= 4; // Index creation is pretty intense, so we are very conservative here
		}
		auto request = temporary_memory_state->GetRemainingSize() * 2;
		while (request < required) {
			request *= 2;
		}

		// Send the request
		temporary_memory_state->SetRemainingSizeAndUpdateReservation(context, request);

		// If we got less than we required, we have to go external
		if (temporary_memory_state->GetReservation() < required) {
			if (!any_combined) {
				external = true;
			}
		}
	}

	void AddSortedRun(SortLocalSinkState &lstate) {
		const lock_guard<mutex> guard(lock);
		sorted_runs.push_back(std::move(lstate.sorted_run));
		sorted_tuples += sorted_runs.back()->Count();
	}

public:
	//! Total number of threads
	const idx_t num_threads;
	//! Temporary memory state for managing this sort's memory usage
	unique_ptr<TemporaryMemoryState> temporary_memory_state;
	//! Runs that have been sorted locally before being appended to this global state
	vector<unique_ptr<SortedRun>> sorted_runs;
	//! Sorted tuple count (for progress)
	atomic<idx_t> sorted_tuples;

	//! Whether this is an external sort
	bool external;
	//! Whether any thread has called Combine yet
	bool any_combined;

	//! Total count (for the source phase)
	idx_t total_count;
	//! Partition size (for the source phase)
	idx_t partition_size;
};

unique_ptr<LocalSinkState> Sort::GetLocalSinkState(ExecutionContext &context) const {
	return make_uniq<SortLocalSinkState>(*this, context.client);
}

unique_ptr<GlobalSinkState> Sort::GetGlobalSinkState(ClientContext &context) const {
	return make_uniq<SortGlobalSinkState>(context);
}

//! Returns true if the Sink call is done (either because run size is small or because run was finalized)
static bool TryFinishSink(SortGlobalSinkState &gstate, SortLocalSinkState &lstate, unique_lock<mutex> &guard) {
	// Check if we exceed the limit
	const auto sorted_run_size = lstate.sorted_run->SizeInBytes();
	if (sorted_run_size < lstate.maximum_run_size) {
		return true; // Sink is done
	}

	// Run size exceeds the limit. If external, the limit will never be updated, so we need to sort
	if (lstate.external) {
		// Finalize, i.e., sort, the run lock-free
		if (guard.owns_lock()) {
			guard.unlock();
		}
		lstate.sorted_run->Finalize(true);

		// Append to global state (grabs lock)
		gstate.AddSortedRun(lstate);
		return true; // Sink is done
	}

	return false; // Sink is not done yet
}

SinkResultType Sort::Sink(ExecutionContext &context, DataChunk &chunk, OperatorSinkInput &input) const {
	auto &gstate = input.global_state.Cast<SortGlobalSinkState>();
	auto &lstate = input.local_state.Cast<SortLocalSinkState>();

	if (!lstate.sorted_run) {
		lstate.InitializeSortedRun(*this, context.client);
		gstate.UpdateLocalState(lstate);
	}

	// Sink data into sorted run
	lstate.key.Reset();
	lstate.payload.Reset();
	lstate.key_executor.Execute(chunk, lstate.key);
	lstate.payload.ReferenceColumns(chunk, input_projection_map);
	lstate.sorted_run->Sink(lstate.key, lstate.payload);

	// Try to finish this call to Sink
	unique_lock<mutex> guard;
	if (TryFinishSink(gstate, lstate, guard)) {
		return SinkResultType::NEED_MORE_INPUT;
	}

	// Grab the lock, update the local state, and see if we can finish now
	guard = unique_lock<mutex>(gstate.lock);
	gstate.UpdateLocalState(lstate);
	if (TryFinishSink(gstate, lstate, guard)) {
		return SinkResultType::NEED_MORE_INPUT;
	}

	// Still no, this thread must try to increase the limit
	gstate.TryIncreaseReservation(context.client, lstate, is_index_sort, guard);
	gstate.UpdateLocalState(lstate);
	guard.unlock(); // Can unlock now, local state is definitely up-to-date

	// This can return false if we somehow still don't have enough memory
	// We'll likely run into an OOM exception
	TryFinishSink(gstate, lstate, guard);

	return SinkResultType::NEED_MORE_INPUT;
}

SinkCombineResultType Sort::Combine(ExecutionContext &context, OperatorSinkCombineInput &input) const {
	auto &gstate = input.global_state.Cast<SortGlobalSinkState>();
	auto &lstate = input.local_state.Cast<SortLocalSinkState>();

	if (!lstate.sorted_run) {
		return SinkCombineResultType::FINISHED;
	}

	// Set any_combined under lock
	unique_lock<mutex> guard {gstate.lock};
	gstate.any_combined = true;
	guard.unlock();

	// Do the final local sort (lock-free)
	lstate.sorted_run->Finalize(gstate.external);

	// Append to global state (grabs lock)
	gstate.AddSortedRun(lstate);

	return SinkCombineResultType::FINISHED;
}

SinkFinalizeType Sort::Finalize(ClientContext &context, OperatorSinkFinalizeInput &input) const {
	auto &gstate = input.global_state.Cast<SortGlobalSinkState>();
	if (gstate.sorted_runs.empty()) {
		return SinkFinalizeType::NO_OUTPUT_POSSIBLE;
	}

	idx_t maximum_run_count = 0;
	for (const auto &sorted_run : gstate.sorted_runs) {
		gstate.total_count += sorted_run->Count();
		maximum_run_count = MaxValue(maximum_run_count, sorted_run->Count());
	}
	if (context.config.verify_parallelism) {
		gstate.partition_size = STANDARD_VECTOR_SIZE;
	} else {
		gstate.partition_size = MinValue<idx_t>(gstate.total_count, DEFAULT_ROW_GROUP_SIZE);
	}

	return SinkFinalizeType::READY;
}

ProgressData Sort::GetSinkProgress(ClientContext &context, GlobalSinkState &gstate_p,
                                   const ProgressData source_progress) const {
	auto &gstate = gstate_p.Cast<SortGlobalSinkState>();
	// Estimate that half of the Sink effort is sorting
	ProgressData res;
	const auto sorted_tuples = static_cast<double>(gstate.sorted_tuples);
	res.done = source_progress.done / 2 + sorted_tuples / 2;
	res.total = source_progress.total;
	res.invalid = source_progress.invalid;
	return res;
}

//===--------------------------------------------------------------------===//
// Source
//===--------------------------------------------------------------------===//
class SortGlobalSourceState : public GlobalSourceState {
public:
	SortGlobalSourceState(const Sort &sort, ClientContext &context, SortGlobalSinkState &sink_p)
	    : sink(sink_p), merger(sort, std::move(sink.sorted_runs), sink.partition_size, sink.external, false),
	      merger_global_state(merger.total_count == 0 ? nullptr : merger.GetGlobalSourceState(context)), registered(0) {
	}

public:
	idx_t MaxThreads() override {
		return merger_global_state ? merger_global_state->MaxThreads() : 1;
	}

	void Destroy() {
		if (!merger_global_state) {
			return;
		}
		const lock_guard<mutex> guard {merger_global_state->lock};
		merger.sorted_runs.clear();
		sink.temporary_memory_state.reset();
	}

public:
	//! The global sink state
	SortGlobalSinkState &sink;
	//! Sorted run merger and associated global state
	SortedRunMerger merger;
	unique_ptr<GlobalSourceState> merger_global_state;
	//! How many local states have been registered
	atomic<idx_t> registered;

	//! Materialized column data (optional)
	unique_ptr<BatchedDataCollection> column_data;
};

class SortLocalSourceState : public LocalSourceState {
public:
	SortLocalSourceState(const Sort &sort, ExecutionContext &context, SortGlobalSourceState &gstate)
	    : merger_local_state(gstate.merger.total_count == 0
	                             ? nullptr
	                             : gstate.merger.GetLocalSourceState(context, *gstate.merger_global_state)) {
		gstate.registered++;
	}

public:
	unique_ptr<LocalSourceState> merger_local_state;
};

unique_ptr<LocalSourceState> Sort::GetLocalSourceState(ExecutionContext &context, GlobalSourceState &gstate) const {
	return make_uniq<SortLocalSourceState>(*this, context, gstate.Cast<SortGlobalSourceState>());
}

unique_ptr<GlobalSourceState> Sort::GetGlobalSourceState(ClientContext &context, GlobalSinkState &sink) const {
	return make_uniq<SortGlobalSourceState>(*this, context, sink.Cast<SortGlobalSinkState>());
}

SourceResultType Sort::GetData(ExecutionContext &context, DataChunk &chunk, OperatorSourceInput &input) const {
	auto &gstate = input.global_state.Cast<SortGlobalSourceState>();
	if (gstate.merger.total_count == 0) {
		return SourceResultType::FINISHED;
	}
	auto &lstate = input.local_state.Cast<SortLocalSourceState>();
	OperatorSourceInput merger_input {*gstate.merger_global_state, *lstate.merger_local_state, input.interrupt_state};
	const auto res = gstate.merger.GetData(context, chunk, merger_input);
	if (res == SourceResultType::FINISHED) {
		lstate.merger_local_state.reset();
		if (--gstate.registered == 0) {
			gstate.Destroy();
		}
	}
	return res;
}

ProgressData Sort::GetProgress(ClientContext &context, GlobalSourceState &gstate_p) const {
	auto &gstate = gstate_p.Cast<SortGlobalSourceState>();
	if (gstate.merger.total_count == 0) {
		return ProgressData {};
	}
	return gstate.merger.GetProgress(context, *gstate.merger_global_state);
}

OperatorPartitionData Sort::GetPartitionData(ExecutionContext &context, DataChunk &chunk, GlobalSourceState &gstate_p,
                                             LocalSourceState &lstate_p,
                                             const OperatorPartitionInfo &partition_info) const {
	auto &gstate = gstate_p.Cast<SortGlobalSourceState>();
	auto &lstate = lstate_p.Cast<SortLocalSourceState>();
	return gstate.merger.GetPartitionData(context, chunk, *gstate.merger_global_state, *lstate.merger_local_state,
	                                      partition_info);
}

//===--------------------------------------------------------------------===//
// Non-Standard Interface
//===--------------------------------------------------------------------===//
SourceResultType Sort::MaterializeColumnData(ExecutionContext &context, OperatorSourceInput &input) const {
	auto &gstate = input.global_state.Cast<SortGlobalSourceState>();

	// Derive output types
	vector<LogicalType> types;
	types.resize(output_projection_columns.size());
	for (auto &opc : output_projection_columns) {
		const auto &type = opc.is_payload
		                       ? payload_layout->GetTypes()[opc.layout_col_idx]
		                       : StructType::GetChildType(decode_sort_key->GetReturnType(), opc.layout_col_idx);
		types[opc.output_col_idx] = type;
	}

	// Initialize scan chunk
	DataChunk chunk;
	chunk.Initialize(context.client, types);

	// Initialize local output collection
	auto local_column_data =
	    make_uniq<BatchedDataCollection>(context.client, types, ColumnDataAllocatorType::BUFFER_MANAGER_ALLOCATOR);

	while (true) {
		// Check for interrupts since this could be a long-running task
		context.client.InterruptCheck();
		// Scan a chunk
		chunk.Reset();
		GetData(context, chunk, input);
		if (chunk.size() == 0) {
			break;
		}
		// Append to the output collection
		const auto batch_index =
		    GetPartitionData(context, chunk, input.global_state, input.local_state, OperatorPartitionInfo())
		        .batch_index;
		local_column_data->Append(chunk, batch_index);
	}

	// Merge into global output collection
	{
		const lock_guard<mutex> guard {gstate.lock};
		if (!gstate.column_data) {
			gstate.column_data = std::move(local_column_data);
		} else {
			gstate.column_data->Merge(*local_column_data);
		}
	}

	// Destroy local state before returning
	input.local_state.Cast<SortLocalSourceState>().merger_local_state.reset();

	// Return type indicates whether materialization is done
	const auto progress_data = GetProgress(context.client, input.global_state);
	if (progress_data.done == progress_data.total) {
		// Destroy global state before returning
		gstate.Destroy();
		return SourceResultType::FINISHED;
	}
	return SourceResultType::HAVE_MORE_OUTPUT;
}

unique_ptr<ColumnDataCollection> Sort::GetColumnData(OperatorSourceInput &input) const {
	auto &gstate = input.global_state.Cast<SortGlobalSourceState>();
	const lock_guard<mutex> guard {gstate.lock};
	return gstate.column_data->FetchCollection();
}

SourceResultType Sort::MaterializeSortedRun(ExecutionContext &context, OperatorSourceInput &input) const {
	auto &gstate = input.global_state.Cast<SortGlobalSourceState>();
	if (gstate.merger.total_count == 0) {
		return SourceResultType::FINISHED;
	}
	auto &lstate = input.local_state.Cast<SortLocalSourceState>();
	OperatorSourceInput merger_input {*gstate.merger_global_state, *lstate.merger_local_state, input.interrupt_state};
	return gstate.merger.MaterializeSortedRun(context, merger_input);
}

unique_ptr<SortedRun> Sort::GetSortedRun(GlobalSourceState &global_state) {
	auto &gstate = global_state.Cast<SortGlobalSourceState>();
	if (gstate.merger.total_count == 0) {
		return nullptr;
	}
	return gstate.merger.GetSortedRun(*gstate.merger_global_state);
}

} // namespace duckdb
