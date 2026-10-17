#include "row_operations_rust_bridge.hpp"

#include "duckdb/common/types/data_chunk.hpp"
#include "duckdb/common/types/row/tuple_data_states.hpp"

namespace duckdb {

RowMatcher::~RowMatcher() {
	// Freeing handle 0 is a no-op, so a matcher that was never initialized is safe to destroy
	ahirudb::row_operations::row_matcher::matcher_free(matcher_handle);
}

RowMatcher::RowMatcher(RowMatcher &&other) noexcept : matcher_handle(other.matcher_handle) {
	other.matcher_handle = 0;
}

RowMatcher &RowMatcher::operator=(RowMatcher &&other) noexcept {
	if (this != &other) {
		ahirudb::row_operations::row_matcher::matcher_free(matcher_handle);
		matcher_handle = other.matcher_handle;
		other.matcher_handle = 0;
	}
	return *this;
}

void RowMatcher::Initialize(const bool no_match_sel, const TupleDataLayout &layout, const Predicates &predicates,
                            vector<column_t> columns) {
	if (matcher_handle) {
		// Re-initializing replaces the match functions; the Rust state is rebuilt
		ahirudb::row_operations::row_matcher::matcher_free(matcher_handle);
		matcher_handle = 0;
	}
	vector<uint8_t> predicate_bytes;
	predicate_bytes.reserve(predicates.size());
	for (const auto &predicate : predicates) {
		predicate_bytes.push_back(static_cast<uint8_t>(predicate));
	}
	vector<uint64_t> column_indices;
	column_indices.reserve(columns.size());
	for (const auto &column : columns) {
		column_indices.push_back(static_cast<uint64_t>(column));
	}
	matcher_handle = row_ops_bridge::ThrowOnRowError(row_ops_bridge::CallIntoRust([&]() {
		return ahirudb::row_operations::row_matcher::matcher_initialize(
		    *row_ops_bridge::MatcherOpsTable(), no_match_sel, &layout,
		    rs_std::SliceRef<const uint8_t>(predicate_bytes.data(), predicate_bytes.size()),
		    rs_std::SliceRef<const uint64_t>(column_indices.data(), column_indices.size()));
	}));
}

idx_t RowMatcher::Match(DataChunk &lhs, const vector<TupleDataVectorFormat> &lhs_formats, SelectionVector &sel,
                        idx_t count, Vector &rhs_row_locations, SelectionVector *no_match_sel, idx_t &no_match_count) {
	auto result = row_ops_bridge::CallIntoRust([&]() {
		return ahirudb::row_operations::row_matcher::matcher_match(
		    matcher_handle, &lhs, &lhs_formats, rs_std::SliceRef<uint32_t>(sel.data(), sel.Capacity()), count,
		    &rhs_row_locations,
		    rs_std::SliceRef<uint32_t>(no_match_sel ? no_match_sel->data() : nullptr,
		                               no_match_sel ? no_match_sel->Capacity() : 0),
		    no_match_count);
	});
	auto out = row_ops_bridge::ThrowOnRowError(std::move(result));
	no_match_count = out.no_match_count;
	return out.match_count;
}

} // namespace duckdb
