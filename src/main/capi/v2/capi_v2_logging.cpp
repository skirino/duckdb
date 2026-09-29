#include "duckdb/main/capi_v2/capi_v2_internal.hpp"

#include "crubit/ahirudb.h"

namespace duckdb {
namespace capiv2 {

static unsigned char ConvertLogLevel(DUCKDB_V2_LOG_LEVEL level) {
	switch (level) {
	case DUCKDB_V2_LOG_LEVEL_TRACE:
		return 10; // LOG_TRACE
	case DUCKDB_V2_LOG_LEVEL_DEBUG:
		return 20; // LOG_DEBUG
	case DUCKDB_V2_LOG_LEVEL_INFO:
		return 30; // LOG_INFO
	case DUCKDB_V2_LOG_LEVEL_WARNING:
		return 40; // LOG_WARNING
	case DUCKDB_V2_LOG_LEVEL_ERROR:
		return 50; // LOG_ERROR
	case DUCKDB_V2_LOG_LEVEL_FATAL:
		return 60; // LOG_FATAL
	default:
		throw InvalidInputException("unknown log level passed to duckdb_v2_context_log");
	}
}

} // namespace capiv2
} // namespace duckdb

//----------------------------------------------------------------------------------------------------------------------
// Public API
//----------------------------------------------------------------------------------------------------------------------

using namespace duckdb::capiv2;

DUCKDB_V2_ERROR duckdb_v2_context_log(duckdb_v2_context_handle ctx, DUCKDB_V2_LOG_LEVEL level, duckdb_v2_str log_type,
                                      duckdb_v2_str message, duckdb_v2_error_info_handle *err) {
	DUCKDB_CHECK_ARG(ctx);
	return WithErrorHandler(err, [&]() {
		const auto log_level = ConvertLogLevel(level);
		const duckdb::string type(Convert(log_type));
		const auto *type_name = type.empty() ? "" : type.c_str();

		const duckdb::string msg(Convert(message));
		// Call the Rust logging function
		ahirudb::logging::ahirudb_logging_write_log(
		    reinterpret_cast<const std::int8_t*>(type_name),
		    log_level,
		    reinterpret_cast<const std::int8_t*>(msg.c_str())
		);
	});
}