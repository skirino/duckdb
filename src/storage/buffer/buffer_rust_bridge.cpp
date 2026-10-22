#include "buffer_rust_bridge.hpp"

namespace duckdb {

void ThrowBufferError(const ::rs::alloc::string::String &error) {
	throw InternalException("%s", string(std::string_view(error.as_str())).c_str());
}

} // namespace duckdb