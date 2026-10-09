#include "test_capi_v2.hpp"

#include <cstring>
#include <string>
#include <vector>

// ---------------------------------------------------------------------------
// V2 context log tests. A `duckdb_v2_context_handle` is only handed out inside a DuckDB-managed scope, so the calls
// are made from a replacement scan callback, which is the cheapest way to obtain one from a test.
//
// The callback must not use Catch assertions (a REQUIRE would throw through the C callback boundary), so what it
// observes is latched into file-scope state that the test asserts on afterwards. All file-scope names carry a Log
// prefix: this directory is a unity build, so they must be unique across test files.
// ---------------------------------------------------------------------------

namespace test_capi_v2 {

namespace {

struct LogObserved {
	//! Return codes of the calls the callback makes, so a test can assert that one of them failed
	std::vector<DUCKDB_V2_ERROR> rcs;
	std::string last_message;
	std::string last_log_type;
	DUCKDB_V2_LOG_LEVEL last_level = DUCKDB_V2_LOG_LEVEL_INFO;
};
LogObserved log_observed;

void LogReset() {
	log_observed = LogObserved();
}

//! Claims every unresolved name as `range(1)` aliased to the probe name, and logs through the context on the way
void LogClaimAndRecord(duckdb_v2_replacement_scan_info_handle info, duckdb_v2_context_handle context,
                       duckdb_v2_error_info_handle *err) {
	duckdb_v2_identifier_t parts[1] = {Convert("range")};
	duckdb_v2_qname_handle name = nullptr;
	auto rc = duckdb_v2_qname_create(parts, 1, &name, err);
	if (rc != DUCKDB_V2_ERROR_NONE) {
		return;
	}
	rc = duckdb_v2_replacement_scan_set_function_name(info, name, err);
	duckdb_v2_qname_destroy(&name);
	if (rc != DUCKDB_V2_ERROR_NONE) {
		return;
	}

	duckdb_v2_value_handle value = nullptr;
	rc = duckdb_v2_value_create_bigint_with_context(context, 1, &value, err);
	if (rc != DUCKDB_V2_ERROR_NONE) {
		return;
	}
	rc = duckdb_v2_replacement_scan_add_argument(info, value, err);
	duckdb_v2_value_destroy(&value);
	if (rc != DUCKDB_V2_ERROR_NONE) {
		return;
	}
	rc = duckdb_v2_replacement_scan_set_alias(info, Convert("log_probe"), err);
	if (rc != DUCKDB_V2_ERROR_NONE) {
		return;
	}

	// The log call gets a throwaway error slot: a failure must not fail the surrounding query
	auto log_rc = duckdb_v2_context_log(
	    context, log_observed.last_level, Convert(log_observed.last_log_type),
	    duckdb_v2_str {log_observed.last_message.data(), log_observed.last_message.size()}, nullptr);
	log_observed.rcs.push_back(log_rc);
}

void LogInstallOnConnection(duckdb_v2_connection_handle conn) {
	duckdb_v2_replacement_scan_handle scan = nullptr;
	REQUIRE(duckdb_v2_replacement_scan_create_with_connection(conn, &scan, nullptr) == DUCKDB_V2_ERROR_NONE);
	REQUIRE(duckdb_v2_replacement_scan_set_callback(scan, LogClaimAndRecord, nullptr) == DUCKDB_V2_ERROR_NONE);
	REQUIRE(duckdb_v2_replacement_scan_register(scan, nullptr) == DUCKDB_V2_ERROR_NONE);
	REQUIRE(duckdb_v2_replacement_scan_destroy(&scan) == DUCKDB_V2_ERROR_NONE);
}

//! Returns the single BIGINT that `sql` produces, so that a test can assert on a value without going through the whole
//! vector API. Value assertions are expressed as `count(*)` with the predicate pushed into the query.
int64_t LogQueryCount(duckdb_v2_connection_handle conn, const std::string &sql) {
	duckdb_v2_result_handle r = nullptr;
	REQUIRE(Query(conn, sql.c_str(), &r) == DUCKDB_V2_ERROR_NONE);

	int64_t count = -1;
	auto chunk = StepChunk(r);
	if (chunk) {
		duckdb_v2_vector_handle vec = nullptr;
		REQUIRE(duckdb_v2_data_chunk_get_vector(chunk, 0, &vec, nullptr) == DUCKDB_V2_ERROR_NONE);
		duckdb_v2_vector_view view {};
		REQUIRE(duckdb_v2_vector_get_view(vec, &view, nullptr) == DUCKDB_V2_ERROR_NONE);
		if (view.data) {
			count = static_cast<const int64_t *>(view.data)[SelAt(view.sel, 0)];
		}
		duckdb_v2_data_chunk_destroy(&chunk);
	}
	while (auto trailing = StepChunk(r)) {
		duckdb_v2_data_chunk_destroy(&trailing);
	}
	duckdb_v2_result_destroy(&r);
	return count;
}

//! The number of rows `sql` produces
int64_t LogQueryRowCount(duckdb_v2_connection_handle conn, const char *sql) {
	duckdb_v2_result_handle r = nullptr;
	REQUIRE(Query(conn, sql, &r) == DUCKDB_V2_ERROR_NONE);
	auto count = DrainRowCount(r);
	duckdb_v2_result_destroy(&r);
	return static_cast<int64_t>(count);
}

} // namespace

TEST_CASE("V2: context log entries land in duckdb_logs", "[capi_v2][logging]") {
	EnvFixture fx;
	ExecSQL(fx.conn, "CALL enable_logging('QueryLog')");
	LogInstallOnConnection(fx.conn);

	LogReset();
	log_observed.last_log_type = "QueryLog";
	log_observed.last_level = DUCKDB_V2_LOG_LEVEL_WARNING;
	log_observed.last_message = "a warning from the C API";
	REQUIRE(LogQueryRowCount(fx.conn, "SELECT * FROM log_probe") == 1);

	REQUIRE(log_observed.rcs.size() == 1);
	REQUIRE(log_observed.rcs[0] == DUCKDB_V2_ERROR_NONE);

	// The entry was written under the log type and severity the caller asked for
	REQUIRE(LogQueryCount(fx.conn, "SELECT count(*) FROM duckdb_logs WHERE type = 'QueryLog' "
	                               "AND log_level = 'WARNING' AND message = 'a warning from the C API'") == 1);
}

TEST_CASE("V2: context log writes nothing while logging is disabled", "[capi_v2][logging]") {
	EnvFixture fx;
	ExecSQL(fx.conn, "CALL disable_logging()");
	LogInstallOnConnection(fx.conn);

	LogReset();
	log_observed.last_log_type = "QueryLog";
	log_observed.last_level = DUCKDB_V2_LOG_LEVEL_ERROR;
	log_observed.last_message = "dropped";
	REQUIRE(LogQueryRowCount(fx.conn, "SELECT * FROM log_probe") == 1);

	// The call still succeeds, it just does not record anything
	REQUIRE(log_observed.rcs.size() == 1);
	REQUIRE(log_observed.rcs[0] == DUCKDB_V2_ERROR_NONE);
	REQUIRE(LogQueryCount(fx.conn, "SELECT count(*) FROM duckdb_logs") == 0);
}

//! An unknown log level is rejected before anything is logged
TEST_CASE("V2: context log rejects an unknown level", "[capi_v2][logging]") {
	EnvFixture fx;
	ExecSQL(fx.conn, "CALL enable_logging()");
	LogInstallOnConnection(fx.conn);

	LogReset();
	log_observed.last_log_type = "QueryLog";
	log_observed.last_level = static_cast<DUCKDB_V2_LOG_LEVEL>(42);
	log_observed.last_message = "nope";
	REQUIRE(LogQueryRowCount(fx.conn, "SELECT * FROM log_probe") == 1);

	REQUIRE(log_observed.rcs.size() == 1);
	REQUIRE(log_observed.rcs[0] != DUCKDB_V2_ERROR_NONE);
}

//! `warnings_as_errors` turns log warnings into errors. Routing `duckdb_v2_context_log` through `Logger` is what makes
//! a C API warning respect the setting, so a C API caller now gets an error back from a log call.
TEST_CASE("V2: context log warnings become errors under warnings_as_errors", "[capi_v2][logging]") {
	EnvFixture fx;
	ExecSQL(fx.conn, "CALL enable_logging()");
	ExecSQL(fx.conn, "SET warnings_as_errors=true");
	LogInstallOnConnection(fx.conn);

	LogReset();
	log_observed.last_log_type = "QueryLog";
	log_observed.last_level = DUCKDB_V2_LOG_LEVEL_WARNING;
	log_observed.last_message = "this warning is an error";
	REQUIRE(LogQueryRowCount(fx.conn, "SELECT * FROM log_probe") == 1);
	REQUIRE(log_observed.rcs.size() == 1);
	REQUIRE(log_observed.rcs[0] != DUCKDB_V2_ERROR_NONE);

	// Other levels are unaffected
	log_observed.last_level = DUCKDB_V2_LOG_LEVEL_INFO;
	log_observed.last_message = "just a message";
	REQUIRE(LogQueryRowCount(fx.conn, "SELECT * FROM log_probe") == 1);
	REQUIRE(log_observed.rcs.size() == 2);
	REQUIRE(log_observed.rcs[1] == DUCKDB_V2_ERROR_NONE);
	REQUIRE(LogQueryCount(fx.conn, "SELECT count(*) FROM duckdb_logs WHERE message = 'just a message'") == 1);
}

//! A log message is not required to be valid UTF-8: it is sanitized on the way into Rust so that the FFI boundary is
//! never handed invalid UTF-8.
TEST_CASE("V2: context log accepts non-UTF-8 messages", "[capi_v2][logging]") {
	EnvFixture fx;
	ExecSQL(fx.conn, "CALL enable_logging()");
	LogInstallOnConnection(fx.conn);

	// 0xff never appears in valid UTF-8
	LogReset();
	log_observed.last_log_type = "QueryLog";
	log_observed.last_level = DUCKDB_V2_LOG_LEVEL_INFO;
	log_observed.last_message = std::string("a\xff"
	                                        "b",
	                                        3);

	REQUIRE(LogQueryRowCount(fx.conn, "SELECT * FROM log_probe") == 1);
	REQUIRE(log_observed.rcs.size() == 1);
	REQUIRE(log_observed.rcs[0] == DUCKDB_V2_ERROR_NONE);

	// The message reached the log storage with the invalid byte replaced by U+FFFD, so the row is valid UTF-8. The
	// expected value is spliced into the query as a single literal: adjacent string literals are not a valid SQL
	// expression.
	const std::string expected = "a\xef\xbf\xbd"
	                             "b";
	REQUIRE(LogQueryCount(fx.conn, "SELECT count(*) FROM duckdb_logs WHERE message = '" + expected + "'") == 1);
}

} // namespace test_capi_v2
