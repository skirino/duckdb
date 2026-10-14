#include "duckdb/common/enums/join_type.hpp"
#include "duckdb/common/exception.hpp"

#include "enums_rust_bridge.hpp"

namespace duckdb {

bool IsLeftOuterJoin(JoinType type) {
	return EnumsRustBridge::IsLeftOuterJoin(type);
}

bool IsRightOuterJoin(JoinType type) {
	return EnumsRustBridge::IsRightOuterJoin(type);
}

bool PropagatesBuildSide(JoinType type) {
	return EnumsRustBridge::PropagatesBuildSide(type);
}

bool HasInverseJoinType(JoinType type) {
	return EnumsRustBridge::HasInverseJoinType(type);
}

JoinType InverseJoinType(JoinType type) {
	return EnumsRustBridge::InverseJoinType(type);
}

string JoinTypeToString(JoinType type) {
	return EnumsRustBridge::JoinTypeToString(type);
}

} // namespace duckdb