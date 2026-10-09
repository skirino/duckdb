//===----------------------------------------------------------------------===//
//                         DuckDB
//
// duckdb/logging/logger.hpp
//
//
//===----------------------------------------------------------------------===//

#pragma once

#include "duckdb/logging/logging.hpp"
#include "duckdb/logging/log_type.hpp"
#include "duckdb/common/types.hpp"
#include "duckdb/common/string_util.hpp"

namespace duckdb {
class TableDescription;
class DatabaseInstance;
class LogManager;
class ThreadContext;
class FileOpener;
class LogStorage;
class ExecutionContext;
struct FileHandle;

//! Internal
#define DUCKDB_LOG_INTERNAL(SOURCE, TYPE, LEVEL, ...)                                                                  \
	{                                                                                                                  \
		auto &logger_ref_ = Logger::Get(SOURCE);                                                                       \
		if (logger_ref_.ShouldLog(TYPE, LEVEL)) {                                                                      \
			logger_ref_.WriteLog(TYPE, LEVEL, __VA_ARGS__);                                                            \
		}                                                                                                              \
	}

//! Default Loggers
#define DUCKDB_LOG_TRACE(SOURCE, ...)                                                                                  \
	DUCKDB_LOG_INTERNAL(SOURCE, DefaultLogType::NAME, LogLevel::LOG_TRACE, __VA_ARGS__)
#define DUCKDB_LOG_DEBUG(SOURCE, ...)                                                                                  \
	DUCKDB_LOG_INTERNAL(SOURCE, DefaultLogType::NAME, LogLevel::LOG_DEBUG, __VA_ARGS__)
#define DUCKDB_LOG_INFO(SOURCE, ...) DUCKDB_LOG_INTERNAL(SOURCE, DefaultLogType::NAME, LogLevel::LOG_INFO, __VA_ARGS__)
#define DUCKDB_LOG_WARNING(SOURCE, ...)                                                                                \
	DUCKDB_LOG_INTERNAL(SOURCE, DefaultLogType::NAME, LogLevel::LOG_WARNING, __VA_ARGS__)
#define DUCKDB_LOG_ERROR(SOURCE, ...)                                                                                  \
	DUCKDB_LOG_INTERNAL(SOURCE, DefaultLogType::NAME, LogLevel::LOG_ERROR, __VA_ARGS__)
#define DUCKDB_LOG_FATAL(SOURCE, ...)                                                                                  \
	DUCKDB_LOG_INTERNAL(SOURCE, DefaultLogType::NAME, LogLevel::LOG_FATAL, __VA_ARGS__)

//! LogType based loggers
#define DUCKDB_LOG(SOURCE, LOG_TYPE_CLASS, ...)                                                                        \
	DUCKDB_LOG_INTERNAL(SOURCE, LOG_TYPE_CLASS::NAME, LOG_TYPE_CLASS::LEVEL,                                           \
	                    LOG_TYPE_CLASS::ConstructLogMessage(__VA_ARGS__))

//! Main logging interface
class Logger {
public:
	DUCKDB_API explicit Logger(LogManager &manager) : manager(manager) {
	}

	DUCKDB_API virtual ~Logger() = default;

	// Main Logging interface. In most cases the macros above should be used instead of calling these directly
	DUCKDB_API virtual bool ShouldLog(const char *log_type, LogLevel log_level) = 0;
	DUCKDB_API void WriteLog(const char *log_type, LogLevel log_level, const char *message);

	// Some more string types for easy logging
	DUCKDB_API void WriteLog(const char *log_type, LogLevel log_level, const string &message);
	DUCKDB_API void WriteLog(const char *log_type, LogLevel log_level, const string_t &message);

	// Syntactic sugar for formatted strings
	template <typename... ARGS>
	void WriteLog(const char *log_type, LogLevel log_level, const char *format_string, ARGS... params) {
		auto formatted_string = StringUtil::Format(format_string, params...);
		WriteLog(log_type, log_level, formatted_string.c_str());
	}

	DUCKDB_API virtual void Flush() = 0;

	// Get the Logger to write log messages to. In decreasing order of preference(!) so the ThreadContext getter is the
	// most preferred way of fetching the logger and the DatabaseInstance getter the least preferred. This has to do
	// both with logging performance and level of detail of logging context that is provided.
	DUCKDB_API static Logger &Get(const ThreadContext &thread_context);
	DUCKDB_API static Logger &Get(const ExecutionContext &execution_context);
	DUCKDB_API static Logger &Get(const ClientContext &client_context);
	DUCKDB_API static Logger &Get(const FileOpener &opener);
	DUCKDB_API static Logger &Get(const DatabaseInstance &db);
	DUCKDB_API static Logger &Get(const shared_ptr<Logger> &logger);

	template <class T>
	static void Flush(T &log_context_source) {
		Get(log_context_source).Flush();
	}

	DUCKDB_API virtual void UpdateConfig(const LogConfig &new_config) {
		throw InternalException("Cannot update the config of this logger!");
	}

protected:
	virtual void WriteLogInternal(const char *log_type, LogLevel log_level, const char *message) = 0;

	//! Throws if `warnings_as_errors` turns this log message into a query error. Called by `WriteLog` before handing
	//! the message to the log storage.
	DUCKDB_API void ThrowOnWarningsAsErrors(LogLevel log_level, const char *message);

protected:
	LogManager &manager;
};

// For when logging is disabled: NOPs everything
class NopLogger : public Logger {
public:
	explicit NopLogger(LogManager &manager) : Logger(manager) {
	}
	bool ShouldLog(const char *log_type, LogLevel log_level) override {
		return false;
	}
	void WriteLogInternal(const char *log_type, LogLevel log_level, const char *message) override {};
	void Flush() override {
	}
};

//! The other two loggers are declared in `src/logging/log_rust_bridge.hpp`: they hold an opaque Rust logger, so they
//! cannot be declared without a type that only that header can define. `LogManager::CreateLogger` is the only way to
//! get one.

} // namespace duckdb
