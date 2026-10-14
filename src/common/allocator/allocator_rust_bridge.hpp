//===----------------------------------------------------------------------===//
//                         DuckDB
//
// common/allocator/allocator_rust_bridge.hpp
//
// Internal glue between the C++ allocator classes and the Rust allocator module in
// `ahirudb`. Not a public header: the crubit-generated bindings are only included
// from the sources in `src/common/allocator`.
//
//===----------------------------------------------------------------------===//

#pragma once

#include "crubit/ahirudb.h"

namespace duckdb {
}
