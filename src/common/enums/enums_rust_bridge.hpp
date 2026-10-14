//===----------------------------------------------------------------------===//
//                         DuckDB
//
// common/enums/enums_rust_bridge.hpp
//
// Internal glue between the C++ enum helpers and the Rust enums module in
// `ahirudb`. Not a public header: the crubit-generated bindings are only
// included from the sources in `src/common/enums`.
//
//===----------------------------------------------------------------------===//

#pragma once

#include "duckdb/common/common.hpp"
#include "duckdb/common/enums/catalog_type.hpp"
#include "duckdb/common/enums/compression_type.hpp"
#include "duckdb/common/enums/date_part_specifier.hpp"
#include "duckdb/common/enums/expression_type.hpp"
#include "duckdb/common/enums/http_status_code.hpp"
#include "duckdb/common/enums/join_type.hpp"
#include "duckdb/common/enums/logical_operator_type.hpp"
#include "duckdb/common/enums/physical_operator_type.hpp"
#include "duckdb/common/enums/relation_type.hpp"
#include "duckdb/common/enums/statement_type.hpp"
#include "duckdb/common/enums/optimizer_type.hpp"
#include "duckdb/common/exception.hpp"
#include "duckdb/common/string_util.hpp"

namespace duckdb {

struct EnumsRustBridge {
	static string CatalogTypeToString(CatalogType type);
	static CatalogType CatalogTypeFromString(const string &type);

	static string CompressionTypeToString(CompressionType type);

	static bool IsBigintDatepart(DatePartSpecifier part_code);
	static bool TryGetDatePartSpecifier(const string &specifier, DatePartSpecifier &result);
	static DatePartSpecifier GetDatePartSpecifier(const string &specifier);

	static string ExpressionTypeToString(ExpressionType type);
	static string ExpressionClassToString(ExpressionClass type);
	static string ExpressionTypeToOperator(ExpressionType type);
	static ExpressionType OperatorToExpressionType(const string &op);
	static ExpressionType NegateComparisonExpression(ExpressionType type);
	static ExpressionType FlipComparisonExpression(ExpressionType type);

	static int32_t HttpStatusCodeFromInt(int32_t status_code);
	static string HttpStatusCodeMessage(HTTPStatusCode status);

	static bool IsLeftOuterJoin(JoinType type);
	static bool IsRightOuterJoin(JoinType type);
	static bool PropagatesBuildSide(JoinType type);
	static bool HasInverseJoinType(JoinType type);
	static JoinType InverseJoinType(JoinType type);
	static string JoinTypeToString(JoinType type);

	static string LogicalOperatorToString(LogicalOperatorType type);

	static idx_t OptimizerCount();
	static string OptimizerName(idx_t idx);
	static uint32_t OptimizerDiscriminant(idx_t idx);
	static string OptimizerNameForDiscriminant(uint32_t type);
	static int64_t OptimizerIndexOfName(const string &str);

	static string PhysicalOperatorToString(PhysicalOperatorType type);

	static string RelationTypeToString(RelationType type);

	static string StatementTypeToString(StatementType type);
	static string StatementReturnTypeToString(StatementReturnType type);
};

} // namespace duckdb
