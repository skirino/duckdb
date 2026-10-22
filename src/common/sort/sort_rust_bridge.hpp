#pragma once

#include "duckdb/common/sorting/sort.hpp"
#include "duckdb/common/sorting/sort_key.hpp"

#include "crubit/ahirudb.h"

#include <cstdint>

namespace duckdb {

namespace sort_bridge {

using ahirudb::sort::sort_key::SortKeyProperties;

} // namespace sort_bridge

} // namespace duckdb