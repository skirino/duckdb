//===----------------------------------------------------------------------===//
//                         DuckDB
//
// vector/vector_rust_bridge.cpp
//
// The one definition the vector module's Rust bridge needs out of line: the Rust
// error strings arrive as `rs::alloc::string::String`, and reading one has to
// happen where that type is complete.
//
//===----------------------------------------------------------------------===//

#include "vector_rust_bridge.hpp"

namespace duckdb {

void ThrowVectorError(const ::rs::alloc::string::String &error) {
	throw InternalException("%s", string(std::string_view(error.as_str())).c_str());
}

} // namespace duckdb
