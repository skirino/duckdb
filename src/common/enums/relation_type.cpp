#include "duckdb/common/enums/relation_type.hpp"

#include "enums_rust_bridge.hpp"

namespace duckdb {

string RelationTypeToString(RelationType type) {
	return EnumsRustBridge::RelationTypeToString(type);
}

} // namespace duckdb
