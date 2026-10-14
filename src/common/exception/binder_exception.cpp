#include "duckdb/common/exception/binder_exception.hpp"
#include "duckdb/parser/parsed_expression.hpp"
#include "duckdb/common/string_util.hpp"
#include "duckdb/function/function.hpp"
#include "exception_rust_bridge.hpp"

namespace duckdb {

BinderException::BinderException(const string &msg) : Exception(ExceptionType::BINDER, msg) {
}

BinderException::BinderException(const unordered_map<string, string> &extra_info, const string &msg)
    : Exception(extra_info, ExceptionType::BINDER, msg) {
}

BinderException BinderException::ColumnNotFound(const Identifier &name, const vector<Identifier> &similar_bindings,
                                                QueryErrorContext context) {
	auto similar = IdentifiersToStrings(similar_bindings);
	string message;
	unordered_map<string, string> extra_info;
	ExceptionRustBridge::ColumnNotFound(name.GetIdentifierName(), similar,
	                                    StringUtil::CandidatesMessage(similar, "Candidate bindings"),
	                                    context.query_location, message, extra_info);
	return BinderException(extra_info, message);
}

BinderException BinderException::NoMatchingFunction(const Identifier &catalog_name, const Identifier &schema_name,
                                                    const Identifier &name, const vector<LogicalType> &arguments,
                                                    const vector<pair<Identifier, LogicalType>> &named_arguments,
                                                    const vector<string> &candidates) {
	string message;
	unordered_map<string, string> extra_info;
	ExceptionRustBridge::NoMatchingFunction(
	    catalog_name.GetIdentifierName(), schema_name.GetIdentifierName(), name.GetIdentifierName(),
	    Function::CallToString(catalog_name, schema_name, name, arguments, named_arguments), candidates, message,
	    extra_info);
	return BinderException(extra_info, message);
}

BinderException BinderException::Unsupported(ParsedExpression &expr, const string &message) {
	auto extra_info = Exception::InitializeExtraInfo("UNSUPPORTED", expr.GetQueryLocation());
	return BinderException(extra_info, message);
}
} // namespace duckdb
