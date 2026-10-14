#include "duckdb/common/enums/http_status_code.hpp"
#include "duckdb/main/http/http_util.hpp"

#include "enums_rust_bridge.hpp"

namespace duckdb {

HTTPStatusCode HTTPUtil::ToStatusCode(int32_t status_code) {
	return static_cast<HTTPStatusCode>(EnumsRustBridge::HttpStatusCodeFromInt(status_code));
}

string HTTPUtil::GetStatusMessage(HTTPStatusCode status) {
	return EnumsRustBridge::HttpStatusCodeMessage(status);
}

} // namespace duckdb