//===----------------------------------------------------------------------===//
//                         DuckDB
//
// duckdb/common/row_operations/row_matcher.hpp
//
//
//===----------------------------------------------------------------------===//

#pragma once

#include "duckdb/common/enums/expression_type.hpp"
#include "duckdb/common/types.hpp"

#include <cstdint>

namespace duckdb {

class DataChunk;
class TupleDataLayout;
struct TupleDataVectorFormat;
struct SelectionVector;

struct RowMatcher {
public:
	using Predicates = vector<ExpressionType>;

	//! Initializes the RowMatcher, filling match functions using layout and equality_predicates
	//! If columns is empty, we assume column indices 0, 1, 2, etc.
	//!
	//! The match functions themselves are built in the Rust row-operations module; this class
	//! owns the handle to that state and releases it on destruction or move.
	void Initialize(const bool no_match_sel, const TupleDataLayout &layout, const Predicates &predicates,
	                vector<column_t> columns = {});

	//! Given a DataChunk on the LHS, on which we've called TupleDataCollection::ToUnifiedFormat,
	//! we match it with rows on the RHS, according to the given layout and locations.
	//! Initially, 'sel' has 'count' entries which point to what needs to be compared.
	//! After matching is done, this returns how many matching entries there are, which 'sel' is modified to point to
	idx_t Match(DataChunk &lhs, const vector<TupleDataVectorFormat> &lhs_formats, SelectionVector &sel, idx_t count,
	            Vector &rhs_row_locations, SelectionVector *no_match_sel, idx_t &no_match_count);

public:
	RowMatcher() = default;
	~RowMatcher();
	RowMatcher(RowMatcher &&other) noexcept;
	RowMatcher &operator=(RowMatcher &&other) noexcept;
	RowMatcher(const RowMatcher &) = delete;
	RowMatcher &operator=(const RowMatcher &) = delete;

private:
	//! The Rust owner of this matcher's match functions, built by `Initialize`
	uintptr_t matcher_handle = 0;
};

} // namespace duckdb
