#include "duckdb/common/enums/logical_operator_type.hpp"

#include "enums_rust_bridge.hpp"

namespace duckdb {

string LogicalOperatorToString(LogicalOperatorType type) {
	return EnumsRustBridge::LogicalOperatorToString(type);
}

} // namespace duckdb
