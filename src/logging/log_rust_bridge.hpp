//===----------------------------------------------------------------------===//
//                         DuckDB
//
// logging/log_rust_bridge.hpp
//
// Internal glue between the C++ logging classes and the Rust logging module in `ahirudb`. Not a public header: the
// crubit-generated bindings are only included from the sources in `src/logging`.
//
//===----------------------------------------------------------------------===//

#pragma once

#include "duckdb/logging/log_manager.hpp"

#include "crubit/ahirudb.h"

namespace duckdb {

string ToStdString(const ::rs::alloc::string::String &value);

//! The Rust mirror of a logging context. The `has_*` flags stand in for the C++ `optional_idx`.
ahirudb::logging::RegisteredLoggingContextLayout ToRustContext(const LoggingContext &context);
//! Same as above, but keeps the context id that was assigned when the context was registered
ahirudb::logging::RegisteredLoggingContextLayout ToRustContext(const RegisteredLoggingContext &context);
RegisteredLoggingContext GetRegisteredLoggingContext(const ahirudb::logging::RegisteredLoggingContextLayout &context);

//! A `StrRef` for a string that is not guaranteed to be valid UTF-8. A `StrRef` may only ever hold valid UTF-8, so
//! invalid sequences are replaced (by the Rust `utf8_lossy`); the replacement bytes are owned by this object, so it has
//! to stay alive for as long as the view is used.
class RustStr {
public:
	explicit RustStr(std::string_view value) : view(rs_std::StrRef::FromUtf8(value)) {
		if (!view) {
			sanitized = ToStdString(ahirudb::logging::logging::utf8_lossy(value));
			view = rs_std::StrRef::FromUtf8Unchecked(std::string_view(sanitized));
		}
	}

	operator rs_std::StrRef() const { // NOLINT(google-explicit-constructor)
		// The constructor always leaves the view engaged: either directly, or from the bytes it sanitized
		D_ASSERT(view);
		return *view;
	}

private:
	std::optional<rs_std::StrRef> view;
	//! Only holds bytes when `value` was not valid UTF-8; `view` borrows from it
	string sanitized;
};

//! A `SliceRef` of `StrRef`s that owns the bytes every view points at. A `StrRef` is only a borrowed view, and a
//! [`RustStr`] that had to sanitize its input keeps those replacement bytes in itself, so the owners have to outlive
//! the slice that is handed to Rust. The owners are individually allocated so that growing the view vector cannot move
//! them.
class RustStrSlice {
public:
	RustStrSlice() = default;
	//! One constructor for every container of names: they all do the same thing with their elements
	template <class VALUES>
	explicit RustStrSlice(const VALUES &values) {
		Add(values);
	}
	//! A null pointer means "no selection at all", which is an empty slice
	explicit RustStrSlice(const optional_ptr<unordered_set<string>> &values) {
		if (values) {
			Add(*values);
		}
	}

	rs_std::SliceRef<rs_std::StrRef const> Get() const {
		return rs_std::SliceRef<rs_std::StrRef const>(views.data(), views.size());
	}

private:
	template <class VALUES>
	void Add(const VALUES &values) {
		views.reserve(values.size());
		for (auto &value : values) {
			views.push_back(*owners.emplace_back(make_uniq<RustStr>(value)));
		}
	}

	vector<unique_ptr<RustStr>> owners;
	vector<rs_std::StrRef> views;
};

//! Plain-scalar mirror of `LogConfig`, used to hand a config from C++ to the Rust `LogManager`. `LogConfig` itself
//! cannot cross the FFI boundary because it holds `unordered_set`s, so its parts are passed individually.
struct RustLogConfig {
	bool enabled = false;
	uint8_t mode = 0;
	uint8_t level = 0;
	string storage;
	RustStrSlice enabled_log_types;
	RustStrSlice disabled_log_types;
};

//! Converts a `LogConfig` into the parts the Rust `LogManager` expects
RustLogConfig ToRustConfig(const LogConfig &config);
//! Same as above, but as the snapshot the Rust `MutableLogger` takes on a config change: this is the only form a config
//! has when it crosses into a logger.
ahirudb::logging::LogConfigSnapshot ToRustConfigSnapshot(const LogConfig &config);
//! Converts a config snapshot from the Rust `LogManager` back into a `LogConfig`. The snapshot fields live in anonymous
//! unions (crubit cannot model structs that need drop glue as C++ aggregates), which is fine because a snapshot is only
//! ever read.
LogConfig FromRustConfig(const ahirudb::logging::LogConfigSnapshot &snapshot);
//! Converts a C++ enum into its Rust counterpart
ahirudb::logging::LogLevel ToRustLogLevel(LogLevel level);
ahirudb::logging::LoggingTargetTable ToRustTargetTable(LoggingTargetTable table);

//! Marks the [`CallIntoRust`] invocation that a log storage error may be attributed to.
class CallScope {
public:
	CallScope();
	~CallScope();

private:
	friend void RethrowPendingStorageError(const CallScope &call_scope);

	//! Identifies this call, so that a log storage error can be attributed to it
	uint64_t index;
	//! The scope that was active before this one, restored on destruction
	uint64_t previous;
};

//! Rethrows the exception that a [`LogStorageBridge`] trampoline caught during `call_scope`, if any. An error that
//! belongs to an earlier call is never rethrown: it would surface as an error of an unrelated statement.
void RethrowPendingStorageError(const CallScope &call_scope);
//! Discards the exception a trampoline caught during a call that never came back through `CallIntoRust`. `CallIntoRust`
//! does this on entry, so an unwrapped storage call cannot leave an error behind for a later statement to throw.
void ClearPendingStorageError();

//! Runs `fn` and rethrows the exception that a [`LogStorageBridge`] trampoline caught on this thread.
//!
//! The log storage is DuckDB C++ code that throws (`BufferingLogStorage::WriteLogEntry` throws when the buffer limit is
//! exceeded, flushing a disabled target throws, `LogStorage::Truncate` is not implemented for every storage). Letting
//! those exceptions unwind out of an `extern "C"` trampoline and through the Rust frames in between is undefined
//! behaviour, so the trampolines catch them and every call into the Rust logging module has to go through this helper.
template <class F>
auto CallIntoRust(F &&fn) -> decltype(fn()) {
	ClearPendingStorageError();
	CallScope call_scope;
	if constexpr (std::is_void_v<decltype(fn())>) {
		fn();
		RethrowPendingStorageError(call_scope);
	} else {
		auto result = fn();
		RethrowPendingStorageError(call_scope);
		return result;
	}
}

//! Owns the heap-allocated Rust `LogManager`. The Rust loggers keep a raw pointer to it (mirroring the C++
//! `LogManager &manager` member), so the address has to stay stable for the lifetime of the manager.
//!
//! The Rust LogManager is built with the C++ LogManager at `ctx` as its log sink, and that is the only sink it ever
//! gets: the functions it calls forward to whichever `duckdb::LogStorage` that C++ LogManager has installed, under that
//! manager's own lock.
class RustLogManager {
public:
	explicit RustLogManager(void *ctx);
	~RustLogManager();

	ahirudb::logging::LogManager &Get() {
		return *manager;
	}

private:
	ahirudb::logging::LogManager *manager;
};

//! Holds one of the Rust loggers. `RUST_LOGGER` is `ahirudb::logging::ThreadSafeLogger` (whose settings never change)
//! or `ahirudb::logging::MutableLogger` (the only logger that tracks later config changes). The two have the same three
//! methods below, so they share this holder; the C++ `Logger` subclasses stay separate because they exist for the
//! virtual overrides.
//!
//! The logger is built from the filter that `LogManager::CreateLogger` based its decision on, so that the kind of the
//! logger and the filter it applies cannot disagree.
template <class RUST_LOGGER>
class RustLogger {
public:
	RustLogger(ahirudb::logging::LogManager &manager, const RegisteredLoggingContext &context,
	           const ahirudb::logging::LogFilter &filter);

	bool ShouldLog(const char *log_type, LogLevel log_level) {
		// The log type crosses as bytes, not as a `StrRef`: `should_log` only matches it against the registered log
		// type names, so it does not have to be valid UTF-8, and this is the hot path of the logging module
		return logger.should_log(std::string_view(log_type), ToRustLogLevel(log_level));
	}

	void WriteLog(const char *log_type, LogLevel log_level, const char *message) {
		CallIntoRust(
		    [&]() { logger.write_log_internal(RustStr(log_type), ToRustLogLevel(log_level), RustStr(message)); });
	}

	void Flush() {
		CallIntoRust([&]() { logger.flush(); });
	}

protected:
	RUST_LOGGER logger;
};

//! Holds the Rust `ThreadSafeLogger`
class RustThreadSafeLogger : public RustLogger<ahirudb::logging::ThreadSafeLogger> {
public:
	using RustLogger::RustLogger;
};

//! Holds the Rust `MutableLogger`, the only logger whose settings change after it was created
class RustMutableLogger : public RustLogger<ahirudb::logging::MutableLogger> {
public:
	using RustLogger::RustLogger;

	//! A config change arrives as a whole snapshot: this is the cold path, unlike the creation above
	void UpdateConfig(const ahirudb::logging::LogConfigSnapshot &config) {
		logger.set_config(config);
	}
};

//! Thread-safe logger. Both this and the [`MutableLogger`] below are declared here rather than in the public
//! `duckdb/logging/logger.hpp`: each holds a `unique_ptr` of an opaque Rust logger type, which would force the
//! crubit-generated headers into a header that every translation unit includes, and neither of them is constructed
//! anywhere but `LogManager::CreateLogger`.
class ThreadSafeLogger : public Logger {
public:
	explicit ThreadSafeLogger(unique_ptr<RustThreadSafeLogger> rust_logger_p, LogManager &manager);

	// Main Logger API
	bool ShouldLog(const char *log_type, LogLevel log_level) override;
	void WriteLogInternal(const char *log_type, LogLevel log_level, const char *message) override;

	void Flush() override;

private:
	unique_ptr<RustThreadSafeLogger> rust_logger;
};

//! Thread-safe Logger with mutable log settings
class MutableLogger : public Logger {
public:
	explicit MutableLogger(unique_ptr<RustMutableLogger> rust_logger_p, LogManager &manager);

	// Main Logger API
	bool ShouldLog(const char *log_type, LogLevel log_level) override;
	void WriteLogInternal(const char *log_type, LogLevel log_level, const char *message) override;

	void Flush() override;
	void UpdateConfig(const LogConfig &new_config) override;

private:
	unique_ptr<RustMutableLogger> rust_logger;
};

//! The C-ABI function table that lets the Rust `LogManager` drive the C++ `LogStorage` owned by a `LogManager`. The
//! signatures mirror the ones `ahirudb::logging::CLogStorage::new_` expects. Every function that can make the storage
//! throw returns `false` and records the exception for [`CallIntoRust`].
class LogStorageBridge {
public:
	static bool WriteLogEntry(void *ctx, int64_t timestamp, uint8_t level, const int8_t *log_type,
	                          uintptr_t log_type_len, const int8_t *log_message, uintptr_t log_message_len,
	                          const ahirudb::logging::RegisteredLoggingContextLayout *context);
	static bool FlushAll(void *ctx);
	static bool Truncate(void *ctx);
	static bool CanScan(void *ctx, uint8_t table);

	//! Builds the sink for the LogManager at `ctx`
	static ahirudb::logging::CLogStorage Create(void *ctx);
};

} // namespace duckdb
