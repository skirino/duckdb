#include "duckdb/common/enums/statement_type.hpp"

#include "duckdb/catalog/catalog.hpp"

#include "enums_rust_bridge.hpp"

namespace duckdb {

string StatementTypeToString(StatementType type) {
	return EnumsRustBridge::StatementTypeToString(type);
}

string StatementReturnTypeToString(StatementReturnType type) {
	return EnumsRustBridge::StatementReturnTypeToString(type);
}

void StatementProperties::RegisterDBRead(Catalog &catalog, ClientContext &context) {
	auto catalog_identity = CatalogIdentity {catalog.GetOid(), catalog.GetCatalogVersion(context)};
	D_ASSERT(read_databases.count(catalog.GetName()) == 0 || read_databases[catalog.GetName()] == catalog_identity);
	read_databases[catalog.GetName()] = catalog_identity;
}

void StatementProperties::RegisterDBModify(Catalog &catalog, ClientContext &context,
                                           DatabaseModificationType modification) {
	auto catalog_identity = CatalogIdentity {catalog.GetOid(), catalog.GetCatalogVersion(context)};
	auto entry = modified_databases.insert(make_pair(catalog.GetName(), ModificationInfo()));
	if (entry.second) {
		entry.first->second.identity = catalog_identity;
	} else {
		D_ASSERT(entry.first->second.identity == catalog_identity);
	}
	entry.first->second.modifications |= modification;
}

} // namespace duckdb
