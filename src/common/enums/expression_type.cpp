#include "duckdb/common/enums/expression_type.hpp"
#include "duckdb/common/exception.hpp"

#include "enums_rust_bridge.hpp"

namespace duckdb {

string ExpressionTypeToString(ExpressionType type) {
	return EnumsRustBridge::ExpressionTypeToString(type);
}

string ExpressionClassToString(ExpressionClass type) {
	return EnumsRustBridge::ExpressionClassToString(type);
}

string ExpressionTypeToOperator(ExpressionType type) {
	return EnumsRustBridge::ExpressionTypeToOperator(type);
}

ExpressionType NegateComparisonExpression(ExpressionType type) {
	return EnumsRustBridge::NegateComparisonExpression(type);
}

ExpressionType FlipComparisonExpression(ExpressionType type) {
	return EnumsRustBridge::FlipComparisonExpression(type);
}

ExpressionType OperatorToExpressionType(const string &op) {
	return EnumsRustBridge::OperatorToExpressionType(op);
}

} // namespace duckdb