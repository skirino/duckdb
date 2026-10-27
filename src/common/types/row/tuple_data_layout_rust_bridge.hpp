//===----------------------------------------------------------------------===//
//                         DuckDB
//
// common/types/row/tuple_data_layout_rust_bridge.hpp
//
// Internal glue between the C++ TupleDataLayout and the Rust tuple_data_layout
// module in `ahirudb`.
//
//===----------------------------------------------------------------------===//

#pragma once

#include "duckdb/common/types/row/tuple_data_layout.hpp"

#include "crubit/ahirudb.h"

#include <cstdint>
#include <vector>

namespace duckdb {

namespace rust {
using ahirudb::tuple_data::layout::ColumnDesc;
using ahirudb::tuple_data::layout::LayoutResult;
using ahirudb::tuple_data::layout::compute_layout;
} // namespace rust

} // namespace duckdb