#include "duckdb/common/enums/catalog_type.hpp"

#include "enums_rust_bridge.hpp"

namespace duckdb {

string CatalogTypeToString(CatalogType type) {
	return EnumsRustBridge::CatalogTypeToString(type);
}

CatalogType CatalogTypeFromString(const string &type) {
	return EnumsRustBridge::CatalogTypeFromString(type);
}

} // namespace duckdb