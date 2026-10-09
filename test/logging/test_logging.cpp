#include "catch.hpp"
#include "test_helpers.hpp"
#include "duckdb.hpp"
#include "duckdb/main/database.hpp"
#include "duckdb/logging/logger.hpp"
#include "duckdb/main/extension/extension_loader.hpp"
#include "duckdb/main/extension_manager.hpp"
#include "duckdb/function/table_function.hpp"
#include "duckdb/logging/log_storage.hpp"
#include "duckdb/logging/log_manager.hpp"

#include <atomic>
#include <thread>

using namespace duckdb;

template <class SOURCE, typename FUN>
void LogSimpleCustomType(SOURCE &src, FUN f) {
	f("custom_type", src, "log-a-lot: 'simple with type'");
}

template <class SOURCE, typename FUN>
void LogFormatStringCustomType(SOURCE &src, FUN f) {
	f("custom_type", src, "log-a-lot: '%s'", "format with type");
}

#define TEST_ALL_LOG_MACROS(LOG_LEVEL, SOURCE, LOG_TYPE)                                                               \
	DUCKDB_LOG_INTERNAL(SOURCE, "default", LOG_LEVEL, "log-a-lot: 'simple'")                                           \
	DUCKDB_LOG_INTERNAL(SOURCE, "default", LOG_LEVEL, "log-a-lot: '%s'", "format")                                     \
	DUCKDB_LOG_INTERNAL(SOURCE, "default", LOG_LEVEL, string("log-a-lot: 'string type'"))                              \
	DUCKDB_LOG_INTERNAL(SOURCE, "custom_type", LOG_LEVEL, "log-a-lot: 'simple with type'")                             \
	DUCKDB_LOG_INTERNAL(SOURCE, "custom_type", LOG_LEVEL, "log-a-lot: '%s'", "format with type")                       \
	DUCKDB_LOG_INTERNAL(SOURCE, "custom_type", LOG_LEVEL, string("log-a-lot: 'string type with type'"))

// Tests all Logger function entrypoints at the specified log level with the specified enabled/disabled loggers
void test_logging(const string &minimum_level, const string &enabled_log_types, const string &disabled_log_types) {
	DuckDB db(nullptr);
	Connection con(db);

	duckdb::vector<Value> default_types = {"default", "default"};
	duckdb::vector<string> log_levels = {"TRACE", "DEBUG", "INFO", "WARNING", "ERROR", "FATAL"};
	auto minimum_level_index = std::find(log_levels.begin(), log_levels.end(), minimum_level) - log_levels.begin();

	REQUIRE_NO_FAIL(con.Query("set enable_logging=true;"));
	REQUIRE_NO_FAIL(con.Query("set logging_level='" + minimum_level + "';"));
	if (!enabled_log_types.empty()) {
		REQUIRE_NO_FAIL(con.Query("set enabled_log_types='" + enabled_log_types + "';"));
		REQUIRE_NO_FAIL(con.Query("set logging_mode='enable_selected';"));
	}
	if (!disabled_log_types.empty()) {
		REQUIRE_NO_FAIL(con.Query("set disabled_log_types='" + disabled_log_types + "';"));
		REQUIRE_NO_FAIL(con.Query("set logging_mode='disable_selected';"));
	}

	bool level_only = (enabled_log_types.empty() && disabled_log_types.empty());
	bool enabled_mode = !enabled_log_types.empty();
	bool disabled_mode = !disabled_log_types.empty();

	// Log all to global logger
	TEST_ALL_LOG_MACROS(LogLevel::LOG_TRACE, *db.instance, DatabaseInstance);
	TEST_ALL_LOG_MACROS(LogLevel::LOG_DEBUG, *db.instance, DatabaseInstance);
	TEST_ALL_LOG_MACROS(LogLevel::LOG_INFO, *db.instance, DatabaseInstance);
	TEST_ALL_LOG_MACROS(LogLevel::LOG_WARNING, *db.instance, DatabaseInstance);
	TEST_ALL_LOG_MACROS(LogLevel::LOG_ERROR, *db.instance, DatabaseInstance);
	TEST_ALL_LOG_MACROS(LogLevel::LOG_FATAL, *db.instance, DatabaseInstance);

	// Log all to client context logger
	TEST_ALL_LOG_MACROS(LogLevel::LOG_TRACE, *con.context, ClientContext);
	TEST_ALL_LOG_MACROS(LogLevel::LOG_DEBUG, *con.context, ClientContext);
	TEST_ALL_LOG_MACROS(LogLevel::LOG_INFO, *con.context, ClientContext);
	TEST_ALL_LOG_MACROS(LogLevel::LOG_WARNING, *con.context, ClientContext);
	TEST_ALL_LOG_MACROS(LogLevel::LOG_ERROR, *con.context, ClientContext);
	TEST_ALL_LOG_MACROS(LogLevel::LOG_FATAL, *con.context, ClientContext);

	// Generate expected log messages
	duckdb::vector<Value> expected_types;
	duckdb::vector<Value> expected_messages;
	duckdb::vector<Value> expected_log_levels;
	idx_t num_runs = 2;
	idx_t num_log_levels = 6;

	for (idx_t i = 0; i < num_runs; i++) {
		for (idx_t j = minimum_level_index; j < num_log_levels; j++) {
			if (level_only ||
			    (enabled_mode && enabled_log_types.find(default_types[i].ToString()) != enabled_log_types.npos) ||
			    (disabled_mode && disabled_log_types.find(default_types[i].ToString()) == enabled_log_types.npos)) {
				expected_messages.push_back("log-a-lot: 'simple'");
				expected_messages.push_back("log-a-lot: 'format'");
				expected_messages.push_back("log-a-lot: 'string type'");
				expected_types.push_back(default_types[i]);
				expected_types.push_back(default_types[i]);
				expected_types.push_back(default_types[i]);
				expected_log_levels.push_back(Value(log_levels[j]));
				expected_log_levels.push_back(Value(log_levels[j]));
				expected_log_levels.push_back(Value(log_levels[j]));
			}

			if (level_only || (enabled_mode && enabled_log_types.find("custom_type") != enabled_log_types.npos) ||
			    (disabled_mode && disabled_log_types.find("custom_type") == enabled_log_types.npos)) {
				expected_messages.push_back("log-a-lot: 'simple with type'");
				expected_messages.push_back("log-a-lot: 'format with type'");
				expected_messages.push_back("log-a-lot: 'string type with type'");
				expected_types.push_back("custom_type");
				expected_types.push_back("custom_type");
				expected_types.push_back("custom_type");
				expected_log_levels.push_back(Value(log_levels[j]));
				expected_log_levels.push_back(Value(log_levels[j]));
				expected_log_levels.push_back(Value(log_levels[j]));
			}
		}
	}

	auto res = con.Query("SELECT type, log_level, message, from duckdb_logs where starts_with(message, 'log-a-lot:')");
	REQUIRE(CHECK_COLUMN(res, 0, expected_types));
	REQUIRE(CHECK_COLUMN(res, 1, expected_log_levels));
	REQUIRE(CHECK_COLUMN(res, 2, expected_messages));
}

// This tests
// - all log levels
// - all combinations of log levels and having either enabled_log_types or disabled_log_types
TEST_CASE("Test logging", "[logging][.]") {
	duckdb::vector<string> log_levels = {"TRACE", "DEBUG", "INFO", "WARNING", "ERROR", "FATAL"};
	for (const auto &level : log_levels) {
		// Test in regular mode without explicitly enabled or disabled loggers
		test_logging(level, "", "");

		// Test various combinations of enabled and disabled loggers
		test_logging(level, "custom_type,default", "");
		test_logging(level, "custom_type", "");
		test_logging(level, "", "default");
		test_logging(level, "", "custom_type,default");
	}
}

struct TestLoggingData : public LocalTableFunctionState {
	explicit TestLoggingData(ExecutionContext &context_p) : context(context_p) {};
	ExecutionContext &context;
};

static duckdb::unique_ptr<LocalTableFunctionState>
TestLoggingInitLocal(ExecutionContext &context, TableFunctionInitInput &input, GlobalTableFunctionState *global_state) {
	return make_uniq<TestLoggingData>(context);
}

static duckdb::unique_ptr<FunctionData> TestLoggingBind(ClientContext &context, TableFunctionBindInput &input,
                                                        duckdb::vector<LogicalType> &return_types,
                                                        duckdb::vector<Identifier> &names) {
	names.emplace_back("value");
	return_types.emplace_back(LogicalType::INTEGER);

	return make_uniq<TableFunctionData>();
}

static void TestLoggingFunction(ClientContext &context, TableFunctionInput &data_p, DataChunk &output) {
	auto &local_state = data_p.local_state->Cast<TestLoggingData>();
	DUCKDB_LOG_INTERNAL(local_state.context, "duckdb.", LogLevel::LOG_INFO, "thread_logger");
}

// This test the thread context logger
TEST_CASE("Test thread context logger", "[logging][.]") {
	DuckDB db(nullptr);
	Connection con(db);

	duckdb::TableFunction tf("test_thread_logger", {}, TestLoggingFunction, TestLoggingBind, nullptr,
	                         TestLoggingInitLocal);
	ExtensionInfo extension_info {};
	ExtensionActiveLoad load_info {*db.instance, extension_info, "log_test_extension", ""};
	ExtensionLoader loader {load_info};
	loader.RegisterFunction(tf);

	REQUIRE_NO_FAIL(con.Query("set enable_logging=true;"));

	// Run our dummy table function to call the thread local logger
	REQUIRE_NO_FAIL(con.Query("FROM test_thread_logger()"));

	auto res = con.Query("SELECT scope, message, from duckdb_logs where starts_with(message, 'thread_logger')");
	REQUIRE(CHECK_COLUMN(res, 0, {"THREAD"}));
	REQUIRE(CHECK_COLUMN(res, 1, {"thread_logger"}));
}

// Testing pluggable log storage
class MyLogStorage : public LogStorage {
public:
	void WriteLogEntry(timestamp_t timestamp, LogLevel level, const string &log_type, const string &log_message,
	                   const RegisteredLoggingContext &context) override {
		lock_guard<mutex> lck(storage_lock);
		log_store.insert(log_message);
	};
	void WriteLogEntries(DataChunk &chunk, const RegisteredLoggingContext &context) override {};
	void Flush(LoggingTargetTable table) override {};
	void FlushAll() override {
		lock_guard<mutex> lck(storage_lock);
		flush_count++;
	};
	bool IsEnabled(LoggingTargetTable table) override {
		return table == LoggingTargetTable::ALL_LOGS;
	}
	const string GetStorageName() override {
		return "MyLogStorage";
	}

	idx_t FlushCount() {
		lock_guard<mutex> lck(storage_lock);
		return flush_count;
	}
	bool Contains(const string &message) {
		lock_guard<mutex> lck(storage_lock);
		return log_store.find(message) != log_store.end();
	}

private:
	mutex storage_lock;
	unordered_set<string> log_store;
	idx_t flush_count = 0;
};

TEST_CASE("Test pluggable log storage", "[logging][.]") {
	DuckDB db(nullptr);
	Connection con(db);

	auto my_log_storage = make_shared_ptr<MyLogStorage>();

	duckdb::shared_ptr<LogStorage> base_ptr = my_log_storage;
	db.instance->GetLogManager().RegisterLogStorage("my_log_storage", base_ptr);

	REQUIRE_NO_FAIL(con.Query("set enable_logging=true;"));
	REQUIRE_NO_FAIL(con.Query("set logging_storage='my_log_storage';"));

	REQUIRE_NO_FAIL(con.Query("select write_log('HELLO, BRO');"));

	REQUIRE(my_log_storage->Contains("HELLO, BRO"));
}

//! A storage switch that is rejected must not touch the storage that is still active: its buffer must not be flushed.
TEST_CASE("A rejected log storage switch keeps the active storage", "[logging][.]") {
	DuckDB db(nullptr);
	Connection con(db);

	auto my_log_storage = make_shared_ptr<MyLogStorage>();
	duckdb::shared_ptr<LogStorage> base_ptr = my_log_storage;
	LogManager &log_manager = db.instance->GetLogManager();
	log_manager.RegisterLogStorage("my_log_storage", base_ptr);

	REQUIRE_NO_FAIL(con.Query("set enable_logging=true;"));
	REQUIRE_NO_FAIL(con.Query("set logging_storage='my_log_storage';"));
	REQUIRE_NO_FAIL(con.Query("select write_log('BEFORE');"));

	// 'SET logging_storage' cannot supply the path that file logging needs, so this switch is rejected
	auto flushes_before_the_switch = my_log_storage->FlushCount();
	REQUIRE_THROWS(log_manager.SetLogStorage(*db.instance, LogConfig::FILE_STORAGE_NAME));
	REQUIRE(my_log_storage->FlushCount() == flushes_before_the_switch);

	REQUIRE_NO_FAIL(con.Query("select write_log('AFTER');"));
	REQUIRE(my_log_storage->Contains("BEFORE"));
	REQUIRE(my_log_storage->Contains("AFTER"));
}

//! A storage that cannot be scanned reads back as an empty result rather than as an error. `MyLogStorage` does not
//! override `CanScan`, so it takes the `false` of the base class even though it does have log entries - which is what
//! every storage but the in-memory one does, since the base `LogStorage::CanScan` defaults to `false`.
TEST_CASE("A log storage that cannot be scanned reads back empty", "[logging][.]") {
	DuckDB db(nullptr);
	Connection con(db);

	auto my_log_storage = make_shared_ptr<MyLogStorage>();
	duckdb::shared_ptr<LogStorage> base_ptr = my_log_storage;
	LogManager &log_manager = db.instance->GetLogManager();
	log_manager.RegisterLogStorage("my_log_storage", base_ptr);

	REQUIRE_NO_FAIL(con.Query("set enable_logging=true;"));
	REQUIRE_NO_FAIL(con.Query("set logging_storage='my_log_storage';"));
	REQUIRE_NO_FAIL(con.Query("select write_log('NOT SCANNABLE');"));
	REQUIRE(my_log_storage->Contains("NOT SCANNABLE"));

	REQUIRE(!log_manager.CanScan(LoggingTargetTable::LOG_ENTRIES));
	REQUIRE(!log_manager.CanScan(LoggingTargetTable::LOG_CONTEXTS));
	REQUIRE(!log_manager.CanScan(LoggingTargetTable::ALL_LOGS));

	auto entries = con.Query("SELECT count(*) FROM duckdb_logs");
	REQUIRE(CHECK_COLUMN(entries, 0, {0}));
	// `duckdb_log_contexts` is only a table function, so it needs its parentheses (`duckdb_logs` is also a view)
	auto contexts = con.Query("SELECT count(*) FROM duckdb_log_contexts()");
	REQUIRE(CHECK_COLUMN(contexts, 0, {0}));
	auto parsed = con.Query("SELECT count(*) FROM duckdb_logs_parsed('QueryLog')");
	REQUIRE(CHECK_COLUMN(parsed, 0, {0}));

	// The zeros above come from `CanScan`, not from a scan that finds nothing: a storage that does scan sees its
	// entries
	REQUIRE_NO_FAIL(con.Query("set logging_storage='memory';"));
	REQUIRE_NO_FAIL(con.Query("select write_log('SCANNABLE');"));
	auto scannable = con.Query("SELECT count(*) > 0 FROM duckdb_logs");
	REQUIRE(CHECK_COLUMN(scannable, 0, {true}));
}

//! A storage that models what `BufferingLogStorage` does with a log entry: it stays in a buffer until the storage is
//! flushed. An entry that is buffered into a storage after that storage was flushed is never persisted, because the
//! replacement took over - which is exactly the entry a non-atomic storage switch loses.
class BufferingTestLogStorage : public LogStorage {
public:
	explicit BufferingTestLogStorage(string name_p) : storage_name(std::move(name_p)) {
	}

	void WriteLogEntry(timestamp_t timestamp, LogLevel level, const string &log_type, const string &log_message,
	                   const RegisteredLoggingContext &context) override {
		lock_guard<mutex> lck(storage_lock);
		// Widens the window between the LogManager picking this storage and the entry arriving in it, which is where a
		// storage switch that is not atomic with respect to writes loses entries
		std::this_thread::sleep_for(std::chrono::microseconds(20));
		buffered.push_back(log_message);
	}
	void WriteLogEntries(DataChunk &chunk, const RegisteredLoggingContext &context) override {};
	void Flush(LoggingTargetTable table) override {};
	void FlushAll() override {
		lock_guard<mutex> lck(storage_lock);
		flush_count++;
		for (auto &entry : buffered) {
			persisted.push_back(std::move(entry));
		}
		buffered.clear();
	}
	void Truncate() override {
		lock_guard<mutex> lck(storage_lock);
		buffered.clear();
		persisted.clear();
	}
	bool IsEnabled(LoggingTargetTable table) override {
		return false;
	}
	const string GetStorageName() override {
		return storage_name;
	}

	idx_t PersistedCount() {
		lock_guard<mutex> lck(storage_lock);
		return persisted.size();
	}
	idx_t FlushCount() {
		lock_guard<mutex> lck(storage_lock);
		return flush_count;
	}

private:
	mutex storage_lock;
	string storage_name;
	vector<string> buffered;
	vector<string> persisted;
	idx_t flush_count = 0;
};

//! Runs `switch_storage` in a loop while another thread writes log entries, and requires that every entry ends up
//! persisted in one of the two test storages
template <class SWITCH>
static void RequireStorageSwitchKeepsEntries(SWITCH switch_storage) {
	DuckDB db(nullptr);
	LogManager &log_manager = db.instance->GetLogManager();

	auto first = make_shared_ptr<BufferingTestLogStorage>("buffering_first");
	auto second = make_shared_ptr<BufferingTestLogStorage>("buffering_second");
	duckdb::shared_ptr<LogStorage> first_ptr = first;
	duckdb::shared_ptr<LogStorage> second_ptr = second;
	REQUIRE(log_manager.RegisterLogStorage("buffering_first", first_ptr));
	REQUIRE(log_manager.RegisterLogStorage("buffering_second", second_ptr));

	log_manager.SetEnableLogging(true);
	auto logger = log_manager.CreateLogger(LoggingContext(LogContextScope::DATABASE));
	REQUIRE(logger->ShouldLog(QueryLogType::NAME, LogLevel::LOG_INFO));

	// The first storage is selected before the writer starts, so that every entry it produces lands in one of the two
	// test storages instead of in the one that is about to be replaced
	switch_storage(*db.instance, "buffering_first");

	// One thread writes, the other swaps the storage out from under it. Both have to finish, and every entry has to end
	// up persisted in one of the two storages
	const idx_t entries = 200;
	const idx_t switches = 500;
	std::atomic<bool> writing {false};
	std::thread writer([&]() {
		writing = true;
		for (idx_t i = 0; i < entries; i++) {
			logger->WriteLog(QueryLogType::NAME, LogLevel::LOG_INFO, "entry");
		}
	});
	while (!writing) {
		std::this_thread::yield();
	}
	for (idx_t i = 0; i < switches; i++) {
		switch_storage(*db.instance, i % 2 == 0 ? "buffering_second" : "buffering_first");
	}
	writer.join();

	// Only the storage that is active at the end is flushed now: anything still buffered in the storage that was
	// replaced is an entry the switch dropped
	log_manager.Flush();
	REQUIRE(first->FlushCount() + second->FlushCount() > 0);
	REQUIRE(first->PersistedCount() + second->PersistedCount() == entries);
}

//! Selecting the storage that is already active is not a switch: the storage that is installed is not replaced and its
//! buffer is not flushed. This is also what pins the name that a switch is resolved against to the installed storage.
TEST_CASE("Selecting the active log storage is a no-op", "[logging][.]") {
	DuckDB db(nullptr);
	Connection con(db);

	LogManager &log_manager = db.instance->GetLogManager();
	auto storage = make_shared_ptr<BufferingTestLogStorage>("buffering_noop");
	duckdb::shared_ptr<LogStorage> base_ptr = storage;
	REQUIRE(log_manager.RegisterLogStorage("buffering_noop", base_ptr));

	REQUIRE_NO_FAIL(con.Query("set enable_logging=true;"));
	log_manager.SetLogStorage(*db.instance, "buffering_noop");
	log_manager.Flush();
	REQUIRE(log_manager.GetConfig().storage == "buffering_noop");

	// Selecting it again - in a different case - resolves to "no switch", so nothing is flushed
	auto flushes_before_the_switch = storage->FlushCount();
	log_manager.SetLogStorage(*db.instance, "BUFFERING_NOOP");
	REQUIRE(storage->FlushCount() == flushes_before_the_switch);
	REQUIRE(log_manager.GetConfig().storage == "buffering_noop");

	// Which means writes still land in the storage that was installed
	auto logger = log_manager.CreateLogger(LoggingContext(LogContextScope::DATABASE));
	logger->WriteLog(QueryLogType::NAME, LogLevel::LOG_INFO, "entry");
	log_manager.Flush();
	REQUIRE(storage->PersistedCount() == 1);
}

//! A storage switch has to be atomic with respect to log writes: the storage that is being replaced is flushed
//! and dropped in one critical section, so an entry can never be buffered into it after it was flushed.
TEST_CASE("A SetLogStorage switch does not lose entries written while it happens", "[logging][.]") {
	RequireStorageSwitchKeepsEntries(
	    [](DatabaseInstance &db, const string &storage_name) { db.GetLogManager().SetLogStorage(db, storage_name); });
}

//! Same, through the other entry point: `SetConfig` resolves and installs the storage in the same way.
TEST_CASE("A SetConfig switch does not lose entries written while it happens", "[logging][.]") {
	RequireStorageSwitchKeepsEntries([](DatabaseInstance &db, const string &storage_name) {
		LogConfig config;
		config.enabled = true;
		config.storage = storage_name;
		db.GetLogManager().SetConfig(db, config);
	});
}

//! A log type selection reaches Rust as a slice of borrowed string views, and "no selection" is a slice with a null
//! data pointer rather than an empty one. It has to arrive as "nothing is selected", and a config that selects nothing
//! must survive the round trip through the manager and the loggers.
TEST_CASE("An empty log type selection reaches Rust as nothing selected", "[logging][.]") {
	DuckDB db(nullptr);
	Connection con(db);

	LogManager &log_manager = db.instance->GetLogManager();
	unordered_set<string> log_types = {QueryLogType::NAME};

	// A null selection clears the set, both through the dedicated setters...
	log_manager.SetEnabledLogTypes(log_types);
	log_manager.SetLogMode(LogMode::ENABLE_SELECTED);
	REQUIRE(log_manager.GetConfig().enabled_log_types.count(QueryLogType::NAME) == 1);

	log_manager.SetEnabledLogTypes(nullptr);
	REQUIRE(log_manager.GetConfig().enabled_log_types.empty());

	log_manager.SetDisabledLogTypes(log_types);
	log_manager.SetLogMode(LogMode::DISABLE_SELECTED);
	log_manager.SetDisabledLogTypes(nullptr);
	REQUIRE(log_manager.GetConfig().disabled_log_types.empty());

	// ...and through a config that selects nothing at all. That is the default, so it is also the path every fresh
	// LogManager takes.
	log_manager.SetLogMode(LogMode::LEVEL_ONLY);
	REQUIRE(log_manager.GetConfig().IsConsistent());
	REQUIRE_NO_FAIL(con.Query("set enable_logging=true;"));
	REQUIRE_NO_FAIL(con.Query("select write_log('NO SELECTION');"));
	auto res = con.Query("SELECT count(*) FROM duckdb_logs WHERE message='NO SELECTION'");
	REQUIRE(CHECK_COLUMN(res, 0, {Value::BIGINT(1)}));
}

//! `ShouldLog` is the hot path of the logging module, so the log type crosses into Rust as raw bytes instead of a
//! validated string view. The name a `LogType` registers is ASCII, so matching bytes answers the same question - but a
//! caller may pass anything at all (`duckdb_v2_context_log` takes the log type from the API user), and a name that is
//! not valid UTF-8 has to match nothing instead of matching whatever sorts next to it.
TEST_CASE("ShouldLog matches a log type name that is not valid UTF-8 against nothing", "[logging][.]") {
	DuckDB db(nullptr);
	Connection con(db);

	LogManager &log_manager = db.instance->GetLogManager();
	unordered_set<string> log_types = {QueryLogType::NAME};
	log_manager.SetEnableLogging(true);
	log_manager.SetEnabledLogTypes(log_types);
	log_manager.SetLogMode(LogMode::ENABLE_SELECTED);

	auto logger = log_manager.CreateLogger(LoggingContext(LogContextScope::DATABASE));
	auto &global_logger = log_manager.GlobalLogger();

	// 0xff never appears in valid UTF-8, so a name carrying it can never equal an ASCII log type name
	const string not_utf8 = string("QueryLog\xff", 9);
	for (auto &candidate : {logger.get(), &global_logger}) {
		REQUIRE(candidate->ShouldLog(QueryLogType::NAME, LogLevel::LOG_INFO));
		REQUIRE(!candidate->ShouldLog(not_utf8.c_str(), LogLevel::LOG_INFO));
		// A name that a lossy conversion would turn into a prefix of a registered one still does not match
		REQUIRE(!candidate->ShouldLog("QueryLo", LogLevel::LOG_INFO));
		REQUIRE(!candidate->ShouldLog("", LogLevel::LOG_INFO));
	}
}

struct CorrectLogType : public LogType {
	static constexpr const char *NAME = "CorrectLogType";
	static constexpr LogLevel LEVEL = LogLevel::LOG_INFO;

	// Correctly using the unstructured type constructor for VARCHAR
	CorrectLogType() : LogType(NAME, LEVEL) {
	}

	template <typename... ARGS>
	static string ConstructLogMessage(const string &str, ARGS... params) {
		return StringUtil::Format(str, params...);
	}
};

constexpr LogLevel CorrectLogType::LEVEL;

TEST_CASE("Add LogType with VARCHAR type", "[logging][.]") {
	DuckDB db(nullptr);
	Connection con(db);

	LogManager &log_manager = db.instance->GetLogManager();

	REQUIRE_NOTHROW(log_manager.RegisterLogType(make_uniq<CorrectLogType>()));
}

//! Log type names are user input, so they can hold bytes that are not valid UTF-8. Those bytes are replaced with
//! U+FFFD on the way into Rust, because the Rust view of a string may only ever hold valid UTF-8 - the replacement has
//! to survive the whole round trip, not just the call that passes it over.
TEST_CASE("Log type names that are not valid UTF-8", "[logging][.]") {
	DuckDB db(nullptr);
	Connection con(db);

	LogManager &log_manager = db.instance->GetLogManager();

	// 0xff never appears in valid UTF-8
	unordered_set<string> log_types = {QueryLogType::NAME, string("bad\xffname", 8)};
	const string sanitized_name = "bad\xef\xbf\xbdname";

	log_manager.SetEnabledLogTypes(log_types);
	log_manager.SetLogMode(LogMode::ENABLE_SELECTED);

	auto config = log_manager.GetConfig();
	REQUIRE(config.enabled_log_types.count(QueryLogType::NAME) == 1);
	REQUIRE(config.enabled_log_types.count(sanitized_name) == 1);

	log_manager.SetEnabledLogTypes(nullptr);
	log_manager.SetDisabledLogTypes(log_types);
	log_manager.SetLogMode(LogMode::DISABLE_SELECTED);
	config = log_manager.GetConfig();
	REQUIRE(config.enabled_log_types.empty());
	REQUIRE(config.disabled_log_types.count(QueryLogType::NAME) == 1);
	REQUIRE(config.disabled_log_types.count(sanitized_name) == 1);

	// Both sets are handed over once more when the config is validated, and again when it is replaced wholesale
	REQUIRE(config.IsConsistent());
	LogConfig replacement;
	replacement.enabled = true;
	replacement.mode = LogMode::ENABLE_SELECTED;
	replacement.enabled_log_types = log_types;
	log_manager.SetConfig(*db.instance, replacement);
	config = log_manager.GetConfig();
	REQUIRE(config.enabled_log_types.count(QueryLogType::NAME) == 1);
	REQUIRE(config.enabled_log_types.count(sanitized_name) == 1);
	REQUIRE(config.IsConsistent());

	// An unregistered log type is still rejected, sanitization or not
	duckdb::vector<string> unknown_types = {string("bad\xffname", 8)};
	REQUIRE_THROWS(log_manager.SetEnableStructuredLoggers(unknown_types));

	// Only the registered name is a valid structured logger
	duckdb::vector<string> known_types = {QueryLogType::NAME};
	REQUIRE_NOTHROW(log_manager.SetEnableStructuredLoggers(known_types));
}

//! A config change reaches the global `MutableLogger` through `UpdateConfig`, and nothing holds the C++
//! `LogManager::lock` while it does - so two connections running two `SET`s at the same time are inside that update at
//! the same time. It is also the path where a logger is built from the filter that `create_logger` decided on, so this
//! covers both halves of the hand-over at once: a reader has to see one self-consistent config, and every entry written
//! has to survive.
TEST_CASE("Concurrent config changes reach the global logger", "[logging][.]") {
	DuckDB db(nullptr);
	Connection con(db);
	REQUIRE_NO_FAIL(con.Query("set enable_logging=true;"));
	REQUIRE_NO_FAIL(con.Query("set logging_level='ERROR';"));

	LogManager &log_manager = db.instance->GetLogManager();
	// A logger whose filter is fixed for the rest of the test, so that every entry a writer produces is written: only
	// the global logger's settings are supposed to change under the writers
	auto logger = log_manager.CreateLogger(LoggingContext(LogContextScope::DATABASE));
	REQUIRE(logger->ShouldLog(QueryLogType::NAME, LogLevel::LOG_ERROR));

	constexpr idx_t THREADS = 4;
	constexpr idx_t WRITES_PER_THREAD = 500;
	constexpr idx_t CHANGES_PER_THREAD = 2000;
	// Assertions inside a spawned thread would escape the thread function and terminate the process, so the failures
	// are counted and checked on this thread instead
	std::atomic<idx_t> inconsistent_filters {0};

	// Writers: the write path, plus a logger created while the settings are churning
	duckdb::vector<std::thread> writers;
	for (idx_t i = 0; i < THREADS; i++) {
		writers.emplace_back([&]() {
			for (idx_t w = 0; w < WRITES_PER_THREAD; w++) {
				logger->WriteLog(QueryLogType::NAME, LogLevel::LOG_ERROR, "concurrent");
				// Logging stays enabled and the level stays at or below `ERROR`, so whatever config this logger was
				// built from, it has to let a `LOG_ERROR` entry through
				auto created = log_manager.CreateLogger(LoggingContext(LogContextScope::DATABASE));
				if (!created->ShouldLog(QueryLogType::NAME, LogLevel::LOG_ERROR)) {
					inconsistent_filters++;
				}
			}
		});
	}
	// Setters: every one of these pushes a config into the global logger, and none of them holds a lock while it does
	duckdb::vector<std::thread> setters;
	for (idx_t i = 0; i < THREADS; i++) {
		setters.emplace_back([&, i]() {
			auto level = (i % 2) ? LogLevel::LOG_ERROR : LogLevel::LOG_TRACE;
			for (idx_t s = 0; s < CHANGES_PER_THREAD; s++) {
				log_manager.SetEnableLogging(true);
				log_manager.SetLogLevel(level);
				// A reader racing the update sees one self-consistent config, if a stale one
				log_manager.GlobalLogger().ShouldLog(QueryLogType::NAME, LogLevel::LOG_ERROR);
			}
		});
	}
	for (auto &writer : writers) {
		writer.join();
	}
	for (auto &setter : setters) {
		setter.join();
	}
	REQUIRE(inconsistent_filters == 0);

	// Whatever the last `SET` was, the global logger ends up with the config it was handed
	log_manager.SetEnableLogging(true);
	log_manager.SetLogLevel(LogLevel::LOG_TRACE);
	REQUIRE(log_manager.GlobalLogger().ShouldLog(QueryLogType::NAME, LogLevel::LOG_ERROR));
	REQUIRE(log_manager.GlobalLogger().ShouldLog(QueryLogType::NAME, LogLevel::LOG_TRACE));

	log_manager.Flush();
	auto res = con.Query("SELECT count(*) FROM duckdb_logs WHERE message='concurrent'");
	REQUIRE(CHECK_COLUMN(res, 0, {Value::BIGINT(THREADS * WRITES_PER_THREAD)}));
}

//! A config change is applied in Rust and published to the global logger, and the two halves have to be one step. If
//! they can be interleaved, two concurrent changes can be *applied* in one order and *published* in the other, which
//! leaves the global logger holding a config that is not the current one - and it keeps holding it until the next
//! change. HEAD held one lock across both halves, and `GetConfig` took that same lock, so neither half was observable
//! on its own.
TEST_CASE("A config change reaches the global logger as one step", "[logging][.]") {
	DuckDB db(nullptr);
	Connection con(db);
	REQUIRE_NO_FAIL(con.Query("set enable_logging=true;"));

	LogManager &log_manager = db.instance->GetLogManager();
	auto &global_logger = log_manager.GlobalLogger();

	// Only the level changes, and logging stays enabled at `LEVEL_ONLY`, so what the global logger answers is a pure
	// function of the level it was published: at `LOG_ERROR` it lets an error through and nothing below it, at
	// `LOG_TRACE` it lets everything through
	constexpr idx_t CHANGES = 20000;
	constexpr idx_t MAX_SAMPLES = 20000;
	std::atomic<bool> changing {false};
	std::thread setter([&]() {
		changing = true;
		for (idx_t i = 0; i < CHANGES; i++) {
			log_manager.SetLogLevel(i % 2 == 0 ? LogLevel::LOG_ERROR : LogLevel::LOG_TRACE);
		}
	});
	while (!changing) {
		std::this_thread::yield();
	}

	idx_t samples = 0;
	idx_t mismatches = 0;
	while (changing && samples < MAX_SAMPLES) {
		// The two reads of the config pin the sample to one config: if they disagree, a change landed while the sample
		// was being taken and the sample says nothing about any single config. If they agree, the global logger has to
		// answer for exactly that config
		auto level_before = log_manager.GetConfig().level;
		auto trace_allowed = global_logger.ShouldLog(QueryLogType::NAME, LogLevel::LOG_TRACE);
		auto error_allowed = global_logger.ShouldLog(QueryLogType::NAME, LogLevel::LOG_ERROR);
		auto level_after = log_manager.GetConfig().level;
		if (level_before != level_after) {
			continue;
		}
		samples++;
		if (!error_allowed || (level_before == LogLevel::LOG_ERROR && trace_allowed)) {
			mismatches++;
		}
	}
	setter.join();

	REQUIRE(samples > 0);
	REQUIRE(mismatches == 0);

	// Whatever the last change was, the global logger ends up with it
	log_manager.SetLogLevel(LogLevel::LOG_ERROR);
	REQUIRE(global_logger.ShouldLog(QueryLogType::NAME, LogLevel::LOG_ERROR));
	REQUIRE(!global_logger.ShouldLog(QueryLogType::NAME, LogLevel::LOG_DEBUG));
}

//! Two switches that resolve against the same active storage name can both decide that they have to install a storage.
//! `CreateLogStorage` builds a *fresh* storage for each of the three built-ins, so the second install flushes and drops
//! the storage the first one installed - with whatever a concurrent write had buffered into it. HEAD held one lock
//! across the whole switch; reinstalling the storage that is already active is a no-op.
TEST_CASE("Concurrent switches to the same storage do not drop entries", "[logging][.]") {
	DuckDB db(nullptr);
	Connection con(db);
	LogManager &log_manager = db.instance->GetLogManager();

	// A registered storage to switch away from: `CreateLogStorage` hands out the registered object for those, so only
	// a built-in is built fresh per switch - and 'memory', the one built-in `SET logging_storage` can reach, is active
	// to begin with
	auto seed = make_shared_ptr<BufferingTestLogStorage>("concurrent_switch_seed");
	duckdb::shared_ptr<LogStorage> seed_ptr = seed;
	REQUIRE(log_manager.RegisterLogStorage("concurrent_switch_seed", seed_ptr));
	log_manager.SetEnableLogging(true);
	log_manager.SetLogStorage(*db.instance, "concurrent_switch_seed");

	auto logger = log_manager.CreateLogger(LoggingContext(LogContextScope::DATABASE));
	REQUIRE(logger->ShouldLog(QueryLogType::NAME, LogLevel::LOG_INFO));

	LogConfig in_memory;
	in_memory.enabled = true;
	in_memory.storage = LogConfig::IN_MEMORY_STORAGE_NAME;

	std::atomic<bool> writing {false};
	std::atomic<bool> stop {false};
	std::atomic<idx_t> written {0};
	std::thread writer([&]() {
		writing = true;
		while (!stop) {
			logger->WriteLog(QueryLogType::NAME, LogLevel::LOG_INFO, "switched");
			written++;
		}
	});
	while (!writing) {
		std::this_thread::yield();
	}

	// Both entry points, over and over: the first switch installs the in-memory storage and every later one is a switch
	// to the storage that is already active
	constexpr idx_t THREADS = 4;
	constexpr idx_t SWITCHES_PER_THREAD = 500;
	duckdb::vector<std::thread> switchers;
	for (idx_t i = 0; i < THREADS; i++) {
		switchers.emplace_back([&, i]() {
			for (idx_t s = 0; s < SWITCHES_PER_THREAD; s++) {
				if ((s + i) % 2 == 0) {
					log_manager.SetLogStorage(*db.instance, LogConfig::IN_MEMORY_STORAGE_NAME);
				} else {
					log_manager.SetConfig(*db.instance, in_memory);
				}
			}
		});
	}
	for (auto &switcher : switchers) {
		switcher.join();
	}
	stop = true;
	writer.join();

	// Every entry has to be somewhere: the ones the seed storage buffered were persisted when the switch away from it
	// flushed it, and the ones the in-memory storage buffered are read back by `duckdb_logs`
	log_manager.Flush();
	auto in_memory_entries = con.Query("SELECT count(*) FROM duckdb_logs WHERE message='switched'");
	auto in_memory_count = idx_t(in_memory_entries->GetValue(0, 0).GetValue<int64_t>());
	REQUIRE(seed->PersistedCount() + in_memory_count == written.load());
}

//! A logger that is not mutable keeps the filter it was created with, so a later config change never reaches it. That
//! is what a `FileHandle` relies on: it captures a logger when it opens the file and logs through it afterwards, so
//! every entry it writes is filtered by the config that was current at open time. HEAD snapshotted the same way.
//!
//! The config's `enabled` flag is part of that filter too, but a logger is only ever created from an enabled config -
//! `CreateLogger` hands out a `NopLogger` otherwise - so it can never be the flag that silences one of these. Stopping
//! them is what the global logger is for: it is the one logger that takes later config changes.
TEST_CASE("A thread safe logger keeps the config it was created with", "[logging][.]") {
	DuckDB db(nullptr);
	Connection con(db);
	LogManager &log_manager = db.instance->GetLogManager();
	log_manager.SetLogLevel(LogLevel::LOG_TRACE);
	log_manager.SetEnableLogging(true);

	auto logger = log_manager.CreateLogger(LoggingContext(LogContextScope::DATABASE));
	REQUIRE(logger->ShouldLog(QueryLogType::NAME, LogLevel::LOG_ERROR));

	log_manager.SetEnableLogging(false);
	// The global logger takes the change: it is the one that was created to follow the config
	REQUIRE(!log_manager.GlobalLogger().ShouldLog(QueryLogType::NAME, LogLevel::LOG_ERROR));
	REQUIRE(!log_manager.GlobalLogger().ShouldLog(QueryLogType::NAME, LogLevel::LOG_FATAL));

	// ... and the logger above does not, at any level
	REQUIRE(logger->ShouldLog(QueryLogType::NAME, LogLevel::LOG_ERROR));
	log_manager.SetLogLevel(LogLevel::LOG_FATAL);
	REQUIRE(logger->ShouldLog(QueryLogType::NAME, LogLevel::LOG_ERROR));

	// A logger created while logging is disabled does not log anything, whatever the level is
	auto disabled_logger = log_manager.CreateLogger(LoggingContext(LogContextScope::DATABASE));
	REQUIRE(!disabled_logger->ShouldLog(QueryLogType::NAME, LogLevel::LOG_FATAL));
}
