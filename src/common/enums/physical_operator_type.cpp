#include "duckdb/common/enums/physical_operator_type.hpp"

#include "enums_rust_bridge.hpp"

namespace duckdb {

string PhysicalOperatorToString(PhysicalOperatorType type) {
	return EnumsRustBridge::PhysicalOperatorToString(type);
}

} // namespace duckdb