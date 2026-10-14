#include "enums_rust_bridge.hpp"

#include "crubit/ahirudb.h"

#include "duckdb/common/exception/conversion_exception.hpp"

namespace duckdb {

static rs_std::SliceRef<const uint8_t> AsSlice(const string &value) {
	return rs_std::SliceRef<const uint8_t>(reinterpret_cast<const uint8_t *>(value.data()), value.size());
}

string EnumsRustBridge::CatalogTypeToString(CatalogType type) {
	auto ref = ahirudb::enums::bridge::catalog_type_to_string(static_cast<uint8_t>(type));
	return string(ref.data(), ref.size());
}

CatalogType EnumsRustBridge::CatalogTypeFromString(const string &type) {
	auto result = ahirudb::enums::bridge::catalog_type_from_string(AsSlice(type));
	if (result < 0) {
		throw InternalException("Unrecognized CatalogType '%s'", type);
	}
	return static_cast<CatalogType>(result);
}

string EnumsRustBridge::CompressionTypeToString(CompressionType type) {
	auto ref = ahirudb::enums::bridge::compression_type_to_string(static_cast<uint8_t>(type));
	string value(ref.data(), ref.size());
	if (value.empty()) {
		throw InternalException("Unrecognized compression type!");
	}
	return value;
}

bool EnumsRustBridge::IsBigintDatepart(DatePartSpecifier part_code) {
	return ahirudb::enums::bridge::is_bigint_datepart(static_cast<uint8_t>(part_code));
}

bool EnumsRustBridge::TryGetDatePartSpecifier(const string &specifier, DatePartSpecifier &result) {
	auto rust_result = ahirudb::enums::bridge::try_date_part_specifier(AsSlice(specifier));
	if (rust_result < 0) {
		return false;
	}
	result = static_cast<DatePartSpecifier>(rust_result);
	return true;
}

DatePartSpecifier EnumsRustBridge::GetDatePartSpecifier(const string &specifier) {
	DatePartSpecifier result;
	if (!TryGetDatePartSpecifier(specifier, result)) {
		throw ConversionException("extract specifier \"%s\" not recognized", specifier);
	}
	return result;
}

string EnumsRustBridge::ExpressionTypeToString(ExpressionType type) {
	auto ref = ahirudb::enums::bridge::expression_type_to_string(static_cast<uint8_t>(type));
	return string(ref.data(), ref.size());
}

string EnumsRustBridge::ExpressionClassToString(ExpressionClass type) {
	auto ref = ahirudb::enums::bridge::expression_class_to_string(static_cast<uint8_t>(type));
	return string(ref.data(), ref.size());
}

string EnumsRustBridge::ExpressionTypeToOperator(ExpressionType type) {
	auto ref = ahirudb::enums::bridge::expression_type_to_operator(static_cast<uint8_t>(type));
	return string(ref.data(), ref.size());
}

ExpressionType EnumsRustBridge::OperatorToExpressionType(const string &op) {
	return static_cast<ExpressionType>(ahirudb::enums::bridge::operator_to_expression_type(AsSlice(op)));
}

ExpressionType EnumsRustBridge::NegateComparisonExpression(ExpressionType type) {
	auto result = ahirudb::enums::bridge::negate_comparison_expression(static_cast<uint8_t>(type));
	if (result < 0) {
		throw InternalException("Unsupported comparison type in negation");
	}
	return static_cast<ExpressionType>(result);
}

ExpressionType EnumsRustBridge::FlipComparisonExpression(ExpressionType type) {
	auto result = ahirudb::enums::bridge::flip_comparison_expression(static_cast<uint8_t>(type));
	if (result < 0) {
		throw InternalException("Unsupported comparison type in flip");
	}
	return static_cast<ExpressionType>(result);
}

int32_t EnumsRustBridge::HttpStatusCodeFromInt(int32_t status_code) {
	return ahirudb::enums::bridge::http_status_code_from_int(status_code);
}

string EnumsRustBridge::HttpStatusCodeMessage(HTTPStatusCode status) {
	auto ref = ahirudb::enums::bridge::http_status_message(static_cast<uint16_t>(status));
	return string(ref.data(), ref.size());
}

bool EnumsRustBridge::IsLeftOuterJoin(JoinType type) {
	return ahirudb::enums::bridge::is_left_outer_join(static_cast<uint8_t>(type));
}

bool EnumsRustBridge::IsRightOuterJoin(JoinType type) {
	return ahirudb::enums::bridge::is_right_outer_join(static_cast<uint8_t>(type));
}

bool EnumsRustBridge::PropagatesBuildSide(JoinType type) {
	return ahirudb::enums::bridge::propagates_build_side(static_cast<uint8_t>(type));
}

bool EnumsRustBridge::HasInverseJoinType(JoinType type) {
	return ahirudb::enums::bridge::has_inverse_join_type(static_cast<uint8_t>(type));
}

JoinType EnumsRustBridge::InverseJoinType(JoinType type) {
	D_ASSERT(HasInverseJoinType(type));
	auto result = ahirudb::enums::bridge::inverse_join_type(static_cast<uint8_t>(type));
	if (result < 0) {
		throw NotImplementedException("InverseJoinType for JoinType::%s", JoinTypeToString(type));
	}
	return static_cast<JoinType>(result);
}

string EnumsRustBridge::JoinTypeToString(JoinType type) {
	auto ref = ahirudb::enums::bridge::join_type_to_string(static_cast<uint8_t>(type));
	string value(ref.data(), ref.size());
	if (value.empty()) {
		throw NotImplementedException("Enum value: unrecognized enum value \"%d\" for enum \"JoinType\"",
		                              static_cast<int>(type));
	}
	return value;
}

string EnumsRustBridge::LogicalOperatorToString(LogicalOperatorType type) {
	auto ref = ahirudb::enums::bridge::logical_operator_to_string(static_cast<uint8_t>(type));
	return string(ref.data(), ref.size());
}

idx_t EnumsRustBridge::OptimizerCount() {
	return ahirudb::enums::bridge::optimizer_count();
}

string EnumsRustBridge::OptimizerName(idx_t idx) {
	auto ref = ahirudb::enums::bridge::optimizer_name(idx);
	return string(ref.data(), ref.size());
}

uint32_t EnumsRustBridge::OptimizerDiscriminant(idx_t idx) {
	return ahirudb::enums::bridge::optimizer_discriminant(idx);
}

string EnumsRustBridge::OptimizerNameForDiscriminant(uint32_t type) {
	auto ref = ahirudb::enums::bridge::optimizer_name_for_discriminant(type);
	return string(ref.data(), ref.size());
}

int64_t EnumsRustBridge::OptimizerIndexOfName(const string &str) {
	return ahirudb::enums::bridge::optimizer_index_of_name(AsSlice(str));
}

string EnumsRustBridge::PhysicalOperatorToString(PhysicalOperatorType type) {
	auto ref = ahirudb::enums::bridge::physical_operator_to_string(static_cast<uint8_t>(type));
	return string(ref.data(), ref.size());
}

string EnumsRustBridge::RelationTypeToString(RelationType type) {
	auto ref = ahirudb::enums::bridge::relation_type_to_string(static_cast<uint8_t>(type));
	return string(ref.data(), ref.size());
}

string EnumsRustBridge::StatementTypeToString(StatementType type) {
	auto ref = ahirudb::enums::bridge::statement_type_to_string(static_cast<uint8_t>(type));
	return string(ref.data(), ref.size());
}

string EnumsRustBridge::StatementReturnTypeToString(StatementReturnType type) {
	auto ref = ahirudb::enums::bridge::statement_return_type_to_string(static_cast<uint8_t>(type));
	return string(ref.data(), ref.size());
}

} // namespace duckdb
