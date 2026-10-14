#include "duckdb/common/exception/catalog_exception.hpp"
#include "duckdb/common/query_location.hpp"
#include "duckdb/common/to_string.hpp"
#include "duckdb/common/string_util.hpp"
#include "duckdb/catalog/entry_lookup_info.hpp"
#include "duckdb/planner/tableref/bound_at_clause.hpp"
#include "exception_rust_bridge.hpp"

namespace duckdb {

CatalogException::CatalogException(const string &msg) : Exception(ExceptionType::CATALOG, msg) {
}

CatalogException::CatalogException(const unordered_map<string, string> &extra_info, const string &msg)
    : Exception(extra_info, ExceptionType::CATALOG, msg) {
}

CatalogException CatalogException::MissingEntry(const EntryLookupInfo &lookup_info, const string &suggestion) {
	string version_info;
	if (auto at_clause = lookup_info.GetAtClause()) {
		version_info +=
		    " at " + StringUtil::Lower(at_clause->Unit().GetIdentifierName()) + " " + at_clause->GetValue().ToString();
	}

	string message;
	unordered_map<string, string> extra_info;
	ExceptionRustBridge::MissingEntry(CatalogTypeToString(lookup_info.GetCatalogType()), lookup_info.GetEntryName(),
	                                  suggestion, version_info, lookup_info.GetErrorContext().query_location, message,
	                                  extra_info);
	return CatalogException(extra_info, message);
}

CatalogException CatalogException::MissingEntry(CatalogType type, const Identifier &name, const string &suggestion,
                                                QueryErrorContext context) {
	EntryLookupInfo lookup_info(type, QualifiedName(name), context);
	return MissingEntry(lookup_info, suggestion);
}

CatalogException CatalogException::MissingEntry(const string &type, const Identifier &name,
                                                const vector<Identifier> &suggestions, QueryErrorContext context) {
	auto suggestion_names = IdentifiersToStrings(suggestions);
	string message;
	unordered_map<string, string> extra_info;
	ExceptionRustBridge::MissingEntry(
	    type, name.GetIdentifierName(), suggestion_names,
	    StringUtil::CandidatesErrorMessage(suggestion_names, name.GetIdentifierName(), "Did you mean"),
	    context.query_location, message, extra_info);
	return CatalogException(extra_info, message);
}

CatalogException CatalogException::EntryAlreadyExists(CatalogType type, const Identifier &name,
                                                      QueryErrorContext context) {
	string message;
	unordered_map<string, string> extra_info;
	ExceptionRustBridge::EntryAlreadyExists(CatalogTypeToString(type), name.GetIdentifierName(), message, extra_info);
	return CatalogException(extra_info, message);
}

} // namespace duckdb
