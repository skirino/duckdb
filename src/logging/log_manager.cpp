#include "duckdb/logging/log_type.hpp"
#include "duckdb/logging/log_manager.hpp"
#include "duckdb/logging/log_storage.hpp"
#include "duckdb/main/database.hpp"
#include "duckdb/main/client_context.hpp"
#include "duckdb/common/local_file_system.hpp"

#include "log_rust_bridge.hpp"

namespace duckdb {

//! Translates a Rust storage switch error into the matching DuckDB exception
static void ThrowStorageSwitchError(const ahirudb::logging::StorageSwitchError &error) {
	auto message = ToStdString(error.message);
	if (error.kind == ahirudb::logging::StorageSwitchErrorKind::MakeNotRegistered()) {
		throw InvalidInputException("%s", message);
	}
	throw InvalidConfigurationException(message);
}

unique_ptr<Logger> LogManager::CreateLogger(LoggingContext context) {
	return CreateLoggerInternal(std::move(context), /*mutable_settings=*/false);
}

unique_ptr<Logger> LogManager::CreateLogger(LoggingContext context, MutableLogSettings) {
	return CreateLoggerInternal(std::move(context), /*mutable_settings=*/true);
}

unique_ptr<Logger> LogManager::CreateLoggerInternal(LoggingContext context, bool mutable_settings) {
	auto created =
	    CallIntoRust([&]() { return rust_manager->Get().create_logger(ToRustContext(context), mutable_settings); });
	if (!created.has_value()) {
		throw InternalException(ToStdString(created.error()));
	}

	auto kind = created.value().kind;
	if (kind == ahirudb::logging::LoggerKind::MakeNop()) {
		// The Rust LogManager skipped the filter for this kind: a `NopLogger` never filters anything, and logging is
		// disabled by default, so building one per query and per task would be pure waste
		return make_uniq<NopLogger>(*this);
	}
	// The filter the kind was decided on, so that a concurrent `enable_logging` cannot leave the logger filtering with
	// a config that does not match what it was created for. It is passed by reference: crubit gives a struct it models
	// with a destructor no move constructor, so taking it by value would copy every log type name in it again
	D_ASSERT(created.value().filter.has_value());
	auto &filter = *created.value().filter;
	auto registered_context = GetRegisteredLoggingContext(created.value().context);
	if (kind == ahirudb::logging::LoggerKind::MakeMutable()) {
		return make_uniq<MutableLogger>(make_uniq<RustMutableLogger>(rust_manager->Get(), registered_context, filter),
		                                *this);
	}
	if (kind == ahirudb::logging::LoggerKind::MakeThreadSafe()) {
		return make_uniq<ThreadSafeLogger>(
		    make_uniq<RustThreadSafeLogger>(rust_manager->Get(), registered_context, filter), *this);
	}
	throw InternalException("Unknown logger kind requested from the Rust LogManager");
}

bool duckdb::LogManager::RegisterLogStorage(const string &name, shared_ptr<LogStorage> &storage) {
	if (!rust_manager->Get().register_log_storage(RustStr(name))) {
		return false;
	}
	lock_guard<mutex> lck(lock);
	registered_log_storages.insert({name, std::move(storage)});
	return true;
}

Logger &duckdb::LogManager::GlobalLogger() {
	return *global_logger;
}

shared_ptr<Logger> duckdb::LogManager::GlobalLoggerReference() {
	return global_logger;
}

void duckdb::LogManager::Flush() {
	CallIntoRust([&]() { rust_manager->Get().flush(); });
}

shared_ptr<LogStorage> duckdb::LogManager::GetLogStorage() {
	lock_guard<mutex> lck(lock);
	return log_storage;
}

bool duckdb::LogManager::CanScan(LoggingTargetTable table) {
	return CallIntoRust([&]() { return rust_manager->Get().can_scan(ToRustTargetTable(table)); });
}

duckdb::LogManager::LogManager(DatabaseInstance &db, LogConfig config_p)
    : rust_manager(make_uniq<RustLogManager>(this)), db_instance(db) {
	// The storage is created before the config is handed to Rust, so that the active storage and the name the config
	// records for it can never disagree. `CreateLogStorage` throws for a storage that does not exist, leaving the
	// LogManager unconstructed rather than half-installed
	log_storage = CreateLogStorage(db, config_p.storage);
	{
		lock_guard<mutex> lck(lock);
		log_storage_name = StringUtil::Lower(config_p.storage);
	}
	// Nothing can race this yet, but `SetConfigInternal` publishes a config the same way every other change does
	lock_guard<mutex> config_lck(config_lock);
	SetConfigInternal(config_p);
}

//! `RustLogManager` is only forward declared in the header, so the destructor has to be defined here
duckdb::LogManager::~LogManager() = default;

void duckdb::LogManager::Initialize() {
	LoggingContext context(LogContextScope::DATABASE);
	global_logger = CreateLogger(context, MutableLogSettings());

	RegisterDefaultLogTypes();
}

LogManager &duckdb::LogManager::Get(ClientContext &context) {
	return context.db->GetLogManager();
}

void duckdb::LogManager::WriteLogEntry(timestamp_t timestamp, LogLevel log_level, const string &log_type,
                                       const string &log_message, const RegisteredLoggingContext &context) {
	// `warnings_as_errors` is applied by `Logger::WriteLog` before it hands the message to the Rust LogManager:
	// throwing from here would unwind through Rust frames.
	// The lock is held for the whole write, so that a storage switch cannot flush and replace the storage in between
	// picking it and writing to it, which would drop the entry after it was flushed
	lock_guard<mutex> lck(lock);
	log_storage->WriteLogEntry(timestamp, log_level, log_type, log_message, context);
}

void duckdb::LogManager::SetConfig(DatabaseInstance &db, const LogConfig &config_p) {
	// The whole config change is one step, from applying it in Rust to publishing it to the global logger
	lock_guard<mutex> config_lck(config_lock);
	// We need extra handling for switching storage: the Rust LogManager validates the storage name and reports the
	// storage that has to be created. The storage that is being replaced is flushed and swapped out in one critical
	// section, so that a log entry can never be buffered into it after it was flushed
	auto active_storage_name = GetActiveLogStorageName();
	auto result = CallIntoRust([&]() {
		return rust_manager->Get().resolve_log_storage(RustStr(active_storage_name), RustStr(config_p.storage),
		                                               CanLogToFile(db));
	});
	if (!result.has_value()) {
		ThrowStorageSwitchError(result.error());
	}
	// Creating the storage can throw, which leaves both the config and the active storage untouched
	auto &target = result.value();
	if (target.has_value()) {
		auto target_name = ToStdString(*target);
		FlushAndSetActiveLogStorage(CreateLogStorage(db, target_name), target_name);
	}
	SetConfigInternal(config_p);
}

void duckdb::LogManager::SetEnableLogging(bool enable) {
	lock_guard<mutex> lck(config_lock);
	auto snapshot = CallIntoRust([&]() { return rust_manager->Get().set_enable_logging(enable); });
	UpdateGlobalLogger(FromRustConfig(snapshot));
}

void duckdb::LogManager::SetLogMode(LogMode mode) {
	lock_guard<mutex> lck(config_lock);
	auto snapshot = CallIntoRust([&]() { return rust_manager->Get().set_log_mode(static_cast<uint8_t>(mode)); });
	UpdateGlobalLogger(FromRustConfig(snapshot));
}

void duckdb::LogManager::SetLogLevel(LogLevel level) {
	lock_guard<mutex> lck(config_lock);
	auto snapshot = CallIntoRust([&]() { return rust_manager->Get().set_log_level(static_cast<uint8_t>(level)); });
	UpdateGlobalLogger(FromRustConfig(snapshot));
}

void duckdb::LogManager::SetEnabledLogTypes(optional_ptr<unordered_set<string>> enabled_log_types) {
	lock_guard<mutex> lck(config_lock);
	auto snapshot = CallIntoRust(
	    [&]() { return rust_manager->Get().set_enabled_log_types(RustStrSlice(enabled_log_types).Get()); });
	UpdateGlobalLogger(FromRustConfig(snapshot));
}

void duckdb::LogManager::SetDisabledLogTypes(optional_ptr<unordered_set<string>> disabled_log_types) {
	lock_guard<mutex> lck(config_lock);
	auto snapshot = CallIntoRust(
	    [&]() { return rust_manager->Get().set_disabled_log_types(RustStrSlice(disabled_log_types).Get()); });
	UpdateGlobalLogger(FromRustConfig(snapshot));
}

void duckdb::LogManager::SetLogStorage(DatabaseInstance &db, const string &storage_name) {
	// Held across the whole switch, for the same reason `SetConfig` holds it: the name the Rust LogManager records
	// after installing the storage is part of the config change
	lock_guard<mutex> lck(config_lock);
	// The Rust LogManager resolves the storage name: it rejects the storages that cannot be selected without extra
	// configuration. It deliberately does not flush - the flush has to be atomic with installing the new storage, which
	// is what `FlushAndSetActiveLogStorage` does under the lock that also guards `WriteLogEntry`
	auto active_storage_name = GetActiveLogStorageName();
	auto result = CallIntoRust([&]() {
		return rust_manager->Get().set_log_storage_name(RustStr(active_storage_name), RustStr(storage_name),
		                                                CanLogToFile(db));
	});
	if (!result.has_value()) {
		ThrowStorageSwitchError(result.error());
	}
	auto &target = result.value();
	if (!target.has_value()) {
		return;
	}
	auto target_name = ToStdString(*target);
	FlushAndSetActiveLogStorage(CreateLogStorage(db, target_name), target_name);
	// Installing the storage also records its name in the config the Rust LogManager holds, which is what `GetConfig`
	// reports
	CallIntoRust([&]() { rust_manager->Get().record_log_storage_name(RustStr(target_name)); });
}

shared_ptr<LogStorage> LogManager::CreateLogStorage(DatabaseInstance &db, const string &storage_name) {
	auto storage_name_to_lower = StringUtil::Lower(storage_name);
	if (storage_name_to_lower == LogConfig::IN_MEMORY_STORAGE_NAME) {
		return make_shared_ptr<InMemoryLogStorage>(db);
	}
	if (storage_name_to_lower == LogConfig::STDOUT_STORAGE_NAME) {
		return make_shared_ptr<StdOutLogStorage>(db);
	}
	if (storage_name_to_lower == LogConfig::FILE_STORAGE_NAME) {
		return make_shared_ptr<FileLogStorage>(db);
	}
	lock_guard<mutex> lck(lock);
	auto lookup = registered_log_storages.find(storage_name_to_lower);
	if (lookup == registered_log_storages.end()) {
		throw InvalidInputException("Log storage '%s' is not yet registered", storage_name);
	}
	return lookup->second;
}

bool duckdb::LogManager::CanLogToFile(DatabaseInstance &db) {
	auto &fs = FileSystem::GetFileSystem(db);
	return !fs.SubSystemIsDisabled(LocalFileSystem().GetName());
}

string duckdb::LogManager::GetActiveLogStorageName() {
	lock_guard<mutex> lck(lock);
	return log_storage_name;
}

void duckdb::LogManager::FlushAndSetActiveLogStorage(const shared_ptr<LogStorage> &storage,
                                                     const string &storage_name) {
	// Flushing and replacing happen under the same lock as `WriteLogEntry`, so no entry can be buffered into the
	// storage that is being replaced after it was flushed. A flush that throws leaves both the storage and the config
	// alone
	lock_guard<mutex> lck(lock);
	if (log_storage_name == storage_name) {
		// Another switch to the same storage won the race to install it. That storage is already the active one, so
		// there is nothing to flush and nothing to replace: flushing it would drop whatever a concurrent write has
		// buffered into it since, and replacing it would drop that buffer with the object
		return;
	}
	log_storage->FlushAll();
	log_storage = storage;
	log_storage_name = storage_name;
}

void duckdb::LogManager::SetConfigInternal(const LogConfig &config_p) {
	// The caller holds `config_lock`, so that applying the config and publishing it to the global logger is one step
	// The whole config is handed to Rust in one step, so that a concurrent `GetConfig` can never observe a half-applied
	// change. The log storage needs nothing here: the Rust LogManager was built with this manager as its sink, and the
	// storage itself was installed above, under the lock that also guards `WriteLogEntry`.
	auto rust_config = ToRustConfig(config_p);
	CallIntoRust([&]() {
		rust_manager->Get().set_config(rust_config.enabled, rust_config.mode, rust_config.level,
		                               RustStr(rust_config.storage), rust_config.enabled_log_types.Get(),
		                               rust_config.disabled_log_types.Get());
	});
	UpdateGlobalLogger(config_p);
}

void duckdb::LogManager::UpdateLogStorageConfig(DatabaseInstance &db, case_insensitive_map_t<Value> &config_value) {
	// The lock is held for the whole call, so that a concurrent storage switch cannot reconfigure the storage that is
	// being replaced
	lock_guard<mutex> lck(lock);
	log_storage->UpdateConfig(db, config_value);
}

void duckdb::LogManager::SetEnableStructuredLoggers(vector<string> &enabled_logger_types) {
	lock_guard<mutex> lck(config_lock);
	RustStrSlice log_types(enabled_logger_types);
	auto result = CallIntoRust([&]() { return rust_manager->Get().set_enable_structured_loggers(log_types.Get()); });
	if (!result.has_value()) {
		throw InvalidInputException("%s", ToStdString(result.error()));
	}
	UpdateGlobalLogger(FromRustConfig(result.value()));
}

void duckdb::LogManager::TruncateLogStorage() {
	CallIntoRust([&]() { rust_manager->Get().truncate_log_storage(); });
}

duckdb::LogConfig duckdb::LogManager::GetConfig() {
	// Held across the read as well, so that a reader cannot observe the config in the window between a `Set*` method
	// applying it and publishing it to the global logger. HEAD took the lock that spans both halves here too
	lock_guard<mutex> lck(config_lock);
	return FromRustConfig(rust_manager->Get().get_config());
}

void duckdb::LogManager::UpdateGlobalLogger(const LogConfig &config) {
	if (!global_logger) {
		return;
	}
	global_logger->UpdateConfig(config);
}

duckdb::optional_ptr<const duckdb::LogType> duckdb::LogManager::LookupLogType(const string &type) {
	auto canonical_name = rust_manager->Get().lookup_log_type(RustStr(type));
	if (!canonical_name.has_value()) {
		return nullptr;
	}
	lock_guard<mutex> lck(lock);
	auto lookup = registered_log_types.find(ToStdString(*canonical_name));
	if (lookup == registered_log_types.end()) {
		return nullptr;
	}
	return lookup->second.get();
}

void duckdb::LogManager::RegisterLogType(unique_ptr<LogType> type) {
	auto result = rust_manager->Get().register_log_type(RustStr(type->name), static_cast<uint8_t>(type->level));
	if (!result.has_value()) {
		throw InvalidInputException("%s", ToStdString(result.error()));
	}
	lock_guard<mutex> lck(lock);
	registered_log_types[type->name] = std::move(type);
}

void duckdb::LogManager::RegisterDefaultLogTypes() {
	RegisterLogType(make_uniq<DefaultLogType>());
	RegisterLogType(make_uniq<FileSystemLogType>());
	RegisterLogType(make_uniq<HTTPLogType>());
	RegisterLogType(make_uniq<QueryLogType>());
	RegisterLogType(make_uniq<PhysicalOperatorLogType>());
	RegisterLogType(make_uniq<MetricsLogType>());
	RegisterLogType(make_uniq<AdaptiveFilterLogType>());
	RegisterLogType(make_uniq<ParquetPrefetchLogType>());
	RegisterLogType(make_uniq<AsyncTaskScheduleLogType>());
	RegisterLogType(make_uniq<ExternalResourceLogType>());
}

} // namespace duckdb
