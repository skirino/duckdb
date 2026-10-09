//===----------------------------------------------------------------------===//
//                         DuckDB
//
// duckdb/logging/log_manager.hpp
//
//
//===----------------------------------------------------------------------===//

#pragma once

#include "duckdb/logging/logger.hpp"
#include "duckdb/logging/log_storage.hpp"
#include "duckdb/common/types/timestamp.hpp"
#include "duckdb/common/case_insensitive_map.hpp"

namespace duckdb {
class LogType;
class LogStorageBridge;
class RustLogManager;

//! Tag argument for the `CreateLogger` overload that returns a mutable logger. The parameter used to be a bare `bool`
//! in a slot where callers passed `thread_safe`, so keeping a `bool` there would silently hand them a different logger.
struct MutableLogSettings {};

//! Holds global logging state
//! - Handles configuration changes
//! - Creates Loggers with cached configuration
//! - Main sink for logs (either by logging directly into this, or by syncing a pre-cached set of log entries)
//! - Holds the log storage
//!
//! The state itself lives in the Rust `ahirudb::logging::LogManager`; this class is the C++ facade that owns the
//! DuckDB-side log storage and forwards to it.
class LogManager {
	friend class Logger;
	friend class LogStorageBridge;

public:
	// Note: two step initialization because Logger needs shared pointer to log manager TODO: can we clean up?
	explicit LogManager(DatabaseInstance &db, LogConfig config = LogConfig());
	//! Defined out of line: `RustLogManager` is only forward declared here
	~LogManager();
	void Initialize();

	DUCKDB_API static LogManager &Get(ClientContext &context);
	//! Creates a logger that filters with the config it read here, and tracks later config changes only if
	//! `MutableLogSettings` is passed
	unique_ptr<Logger> CreateLogger(LoggingContext context);
	unique_ptr<Logger> CreateLogger(LoggingContext context, MutableLogSettings);

	DUCKDB_API bool RegisterLogStorage(const string &name, shared_ptr<LogStorage> &storage);

	//! The global logger can be used when
	DUCKDB_API Logger &GlobalLogger();
	DUCKDB_API shared_ptr<Logger> GlobalLoggerReference();

	//! Flush everything
	DUCKDB_API void Flush();

	//! Get a shared_ptr to the log storage (For example, to scan it)
	DUCKDB_API shared_ptr<LogStorage> GetLogStorage();
	DUCKDB_API bool CanScan(LoggingTargetTable table);

	DUCKDB_API void SetConfig(DatabaseInstance &db, const LogConfig &config);
	DUCKDB_API void SetEnableLogging(bool enable);
	DUCKDB_API void SetLogMode(LogMode mode);
	DUCKDB_API void SetLogLevel(LogLevel level);
	DUCKDB_API void SetEnabledLogTypes(optional_ptr<unordered_set<string>> enabled_log_types);
	DUCKDB_API void SetDisabledLogTypes(optional_ptr<unordered_set<string>> disabled_log_types);
	DUCKDB_API void SetLogStorage(DatabaseInstance &db, const string &storage_name);

	DUCKDB_API void UpdateLogStorageConfig(DatabaseInstance &db, case_insensitive_map_t<Value> &config_value);

	DUCKDB_API void SetEnableStructuredLoggers(vector<string> &enabled_logger_types);

	DUCKDB_API void TruncateLogStorage();

	DUCKDB_API LogConfig GetConfig();

	DUCKDB_API void RegisterLogType(unique_ptr<LogType> type);
	DUCKDB_API optional_ptr<const LogType> LookupLogType(const string &type);
	DUCKDB_API void RegisterDefaultLogTypes();

protected:
	//! Shared body of the two `CreateLogger` overloads
	unique_ptr<Logger> CreateLoggerInternal(LoggingContext context, bool mutable_settings);

	// This is to be called by the Loggers only, it does not verify log_level and log_type
	void WriteLogEntry(timestamp_t, LogLevel log_level, const string &log_type, const string &log_message,
	                   const RegisteredLoggingContext &context);

	//! Creates the concrete storage for `storage_name`, after the Rust LogManager validated the name
	shared_ptr<LogStorage> CreateLogStorage(DatabaseInstance &db, const string &storage_name);
	//! The (lowercased) name of the storage that is active right now. This is the name a storage switch is resolved
	//! against: it is authoritative, unlike the name the config records, which is updated after the storage is
	//! installed
	string GetActiveLogStorageName();
	//! Flushes the active storage and replaces it by `storage_name`. Both happen under the same lock as
	//! `WriteLogEntry`, so a log entry can never be buffered into the storage that is being replaced after it was
	//! flushed. Reinstalling the storage that is already active is a no-op: two switches to the same name can resolve
	//! against the same active name, and only the first of them has anything to flush
	void FlushAndSetActiveLogStorage(const shared_ptr<LogStorage> &storage, const string &storage_name);
	//! Runs `op` on the active storage, under the lock that also guards `WriteLogEntry`. This is how the Rust
	//! `LogManager` reaches the storage for everything except a write (`flush`, `truncate`, `can_scan`), so that the
	//! storage cannot be swapped out in between picking it and using it. That lock is the only thing serializing those
	//! paths: the Rust LogManager has none of its own, because this one is already taken on the way in
	template <class OP>
	decltype(auto) WithActiveLogStorage(OP &&op) {
		lock_guard<mutex> lck(lock);
		return op(*log_storage);
	}
	//! Hands a whole config to the Rust LogManager in one step. The log storage is not part of it: the Rust LogManager
	//! was built with this manager as its sink. The caller holds `config_lock`, so that applying the config and
	//! publishing it to the global logger cannot be interleaved with another change
	void SetConfigInternal(const LogConfig &config);
	//! Installs `config` in the global logger. The `Set*` methods pass the config the Rust LogManager just produced,
	//! rather than re-reading it, so that two concurrent changes cannot leave the global logger with a stale field;
	//! `SetConfig` passes the caller's config, which is the one Rust was just given in full. Every entry point that
	//! mutates the config holds `config_lock` across both halves, so a change cannot be *applied* in one order and
	//! *published* in the other
	void UpdateGlobalLogger(const LogConfig &config);
	//! Whether file logging is possible at all, which only C++ can answer because it needs the DatabaseInstance
	bool CanLogToFile(DatabaseInstance &db);

protected:
	/// Declared first so that it is destroyed last: the loggers and the log storage hold pointers into it
	unique_ptr<RustLogManager> rust_manager;

	/// Guards the DuckDB-side state below. It is a leaf lock: it is never held while calling into Rust, so it cannot
	/// participate in a lock cycle with the Rust LogManager's own config lock. It does cover a storage call -
	/// `WriteLogEntry` holds it for the whole write and a storage switch holds it across the flush and the swap - which
	/// is what makes the switch atomic with respect to concurrent writes, and what serializes every path from Rust to
	/// the storage (the Rust LogManager takes no lock of its own for those).
	mutex lock;

	/// Serializes a config change from the moment it is applied to the moment it is published to the global logger.
	/// The Rust config mutex is released as soon as a mutator returns its snapshot, so without this the global logger
	/// could keep a config that a later change has already replaced. It is a leaf lock too: none of the entry points
	/// that take it takes `lock` at the same time, except `SetConfig`'s storage switch, which happens before
	/// `SetConfigInternal` - so the only order that exists is `config_lock` before `lock`.
	mutex config_lock;

	shared_ptr<Logger> global_logger;
	shared_ptr<LogStorage> log_storage;
	//! The name `log_storage` was selected with, kept next to it and under the same lock. It is what a storage switch
	//! is resolved against, so it can never disagree with the storage that is actually installed
	string log_storage_name;
	DatabaseInstance &db_instance;

	// Any additional LogStorages registered (by extensions for example)
	case_insensitive_map_t<shared_ptr<LogStorage>> registered_log_storages;
	case_insensitive_map_t<unique_ptr<LogType>> registered_log_types;
};

} // namespace duckdb
