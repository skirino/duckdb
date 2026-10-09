#include "duckdb/logging/log_manager.hpp"
#include "duckdb/logging/logger.hpp"

#include "duckdb/main/client_context.hpp"
#include "duckdb/main/database.hpp"
#include "duckdb/main/settings.hpp"
#include "duckdb/common/file_opener.hpp"
#include "duckdb/parallel/thread_context.hpp"

#include "log_rust_bridge.hpp"

namespace duckdb {

void Logger::WriteLog(const char *log_type, LogLevel log_level, const string &message) {
	WriteLog(log_type, log_level, message.c_str());
}
void Logger::WriteLog(const char *log_type, LogLevel log_level, const string_t &message) {
	string copied_string = message.GetString();
	WriteLog(log_type, log_level, copied_string.c_str());
}

void Logger::WriteLog(const char *log_type, LogLevel log_level, const char *message) {
	ThrowOnWarningsAsErrors(log_level, message);
	WriteLogInternal(log_type, log_level, message);
}

void Logger::ThrowOnWarningsAsErrors(LogLevel log_level, const char *message) {
	// `warnings_as_errors` turns log warnings into query errors. This has to happen before the message reaches the log
	// storage, and it cannot live in Rust because it needs the C++ settings.
	if (log_level == LogLevel::LOG_WARNING && Settings::Get<WarningsAsErrorsSetting>(manager.db_instance)) {
		throw InvalidInputException(message);
	}
}

Logger &Logger::Get(const DatabaseInstance &db) {
	return db.GetLogManager().GlobalLogger();
}

Logger &Logger::Get(const ThreadContext &thread_context) {
	return *thread_context.logger;
}

Logger &Logger::Get(const ExecutionContext &execution_context) {
	return *execution_context.thread.logger;
}

Logger &Logger::Get(const ClientContext &client_context) {
	return client_context.GetLogger();
}

Logger &Logger::Get(const FileOpener &opener) {
	return opener.GetLogger();
}

Logger &Logger::Get(const shared_ptr<Logger> &logger) {
	return *logger;
}

//===--------------------------------------------------------------------===//
// ThreadSafeLogger
//===--------------------------------------------------------------------===//
ThreadSafeLogger::ThreadSafeLogger(unique_ptr<RustThreadSafeLogger> rust_logger_p, LogManager &manager)
    : Logger(manager), rust_logger(std::move(rust_logger_p)) {
}

bool ThreadSafeLogger::ShouldLog(const char *log_type, LogLevel log_level) {
	return rust_logger->ShouldLog(log_type, log_level);
}

void ThreadSafeLogger::WriteLogInternal(const char *log_type, LogLevel log_level, const char *log_message) {
	rust_logger->WriteLog(log_type, log_level, log_message);
}

void ThreadSafeLogger::Flush() {
	rust_logger->Flush();
}

//===--------------------------------------------------------------------===//
// MutableLogger
//===--------------------------------------------------------------------===//
MutableLogger::MutableLogger(unique_ptr<RustMutableLogger> rust_logger_p, LogManager &manager)
    : Logger(manager), rust_logger(std::move(rust_logger_p)) {
}

bool MutableLogger::ShouldLog(const char *log_type, LogLevel log_level) {
	return rust_logger->ShouldLog(log_type, log_level);
}

void MutableLogger::WriteLogInternal(const char *log_type, LogLevel log_level, const char *log_message) {
	rust_logger->WriteLog(log_type, log_level, log_message);
}

void MutableLogger::Flush() {
	rust_logger->Flush();
}

void MutableLogger::UpdateConfig(const LogConfig &new_config) {
	rust_logger->UpdateConfig(ToRustConfigSnapshot(new_config));
}

} // namespace duckdb
