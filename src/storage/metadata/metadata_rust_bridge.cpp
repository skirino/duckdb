//===----------------------------------------------------------------------===//
//                         DuckDB
//
// storage/metadata/metadata_rust_bridge.cpp
//
//===----------------------------------------------------------------------===//

#include "metadata_rust_bridge.hpp"

namespace duckdb {

void ThrowMetadataError(const ::rs::alloc::string::String &error) {
	throw InternalException("%s", string(std::string_view(error.as_str())).c_str());
}

} // namespace duckdb
