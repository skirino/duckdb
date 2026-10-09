//===----------------------------------------------------------------------===//
//                         DuckDB
//
// logging/log_rust_bridge.cpp
//
// Internal glue between the C++ logging classes and the Rust logging module in `ahirudb`.
//
//===----------------------------------------------------------------------===//

#include "log_rust_bridge.hpp"

#include "duckdb/common/exception.hpp"

namespace duckdb {

//===--------------------------------------------------------------------===//
// Conversions
//===--------------------------------------------------------------------===//
RustLogConfig ToRustConfig(const LogConfig &config) {
	RustLogConfig result;
	result.enabled = config.enabled;
	result.mode = static_cast<uint8_t>(config.mode);
	result.level = static_cast<uint8_t>(config.level);
	result.storage = config.storage;
	result.enabled_log_types = RustStrSlice(config.enabled_log_types);
	result.disabled_log_types = RustStrSlice(config.disabled_log_types);
	return result;
}

ahirudb::logging::LogConfigSnapshot ToRustConfigSnapshot(const LogConfig &config) {
	auto rust_config = ToRustConfig(config);
	return ahirudb::logging::log_config_snapshot(rust_config.enabled, rust_config.mode, rust_config.level,
	                                             RustStr(rust_config.storage), rust_config.enabled_log_types.Get(),
	                                             rust_config.disabled_log_types.Get());
}

LogConfig FromRustConfig(const ahirudb::logging::LogConfigSnapshot &snapshot) {
	LogConfig result;
	result.enabled = snapshot.enabled;
	result.mode = static_cast<LogMode>(snapshot.mode);
	result.level = static_cast<LogLevel>(snapshot.level);
	result.storage = ToStdString(snapshot.storage);
	for (auto &log_type : snapshot.enabled_log_types) {
		result.enabled_log_types.insert(ToStdString(log_type));
	}
	for (auto &log_type : snapshot.disabled_log_types) {
		result.disabled_log_types.insert(ToStdString(log_type));
	}
	return result;
}

string ToStdString(const ::rs::alloc::string::String &value) {
	return string(std::string_view(value.as_str()));
}

static ahirudb::logging::RegisteredLoggingContextLayout MakeRustContext(const LoggingContext &context,
                                                                        idx_t context_id) {
	ahirudb::logging::RegisteredLoggingContextLayout result;
	result.context_id = context_id;
	result.scope = static_cast<uint8_t>(context.scope);
	// An `optional_idx` that is not set becomes the `has_* = false` / `value = 0` pair the Rust mirror uses
	auto set = [](const optional_idx &value, bool &has_flag, idx_t &target) {
		has_flag = value.IsValid();
		target = has_flag ? value.GetIndex() : 0;
	};
	set(context.thread_id, result.has_thread_id, result.thread_id);
	set(context.connection_id, result.has_connection_id, result.connection_id);
	set(context.transaction_id, result.has_transaction_id, result.transaction_id);
	set(context.query_id, result.has_query_id, result.query_id);
	return result;
}

ahirudb::logging::RegisteredLoggingContextLayout ToRustContext(const LoggingContext &context) {
	return MakeRustContext(context, 0);
}

ahirudb::logging::RegisteredLoggingContextLayout ToRustContext(const RegisteredLoggingContext &context) {
	return MakeRustContext(context.context, context.context_id);
}

RegisteredLoggingContext GetRegisteredLoggingContext(const ahirudb::logging::RegisteredLoggingContextLayout &context) {
	RegisteredLoggingContext result {context.context_id, LoggingContext(static_cast<LogContextScope>(context.scope))};
	if (context.has_thread_id) {
		result.context.thread_id = optional_idx(context.thread_id);
	}
	if (context.has_connection_id) {
		result.context.connection_id = optional_idx(context.connection_id);
	}
	if (context.has_transaction_id) {
		result.context.transaction_id = optional_idx(context.transaction_id);
	}
	if (context.has_query_id) {
		result.context.query_id = optional_idx(context.query_id);
	}
	return result;
}

ahirudb::logging::LogLevel ToRustLogLevel(LogLevel level) {
	return static_cast<ahirudb::logging::LogLevel>(static_cast<uint8_t>(level));
}

ahirudb::logging::LoggingTargetTable ToRustTargetTable(LoggingTargetTable table) {
	// Not a `static_cast`: crubit models a fieldless enum as a tagged union, and the payload is private
	switch (table) {
	case LoggingTargetTable::ALL_LOGS:
		return ahirudb::logging::LoggingTargetTable::MakeALL_LOGS();
	case LoggingTargetTable::LOG_ENTRIES:
		return ahirudb::logging::LoggingTargetTable::MakeLOG_ENTRIES();
	case LoggingTargetTable::LOG_CONTEXTS:
		return ahirudb::logging::LoggingTargetTable::MakeLOG_CONTEXTS();
	default:
		throw InternalException("Unknown logging target table");
	}
}

//===--------------------------------------------------------------------===//
// LogConfig
//===--------------------------------------------------------------------===//
LogConfig::LogConfig()
    : enabled(false), mode(LogMode::LEVEL_ONLY), level(DEFAULT_LOG_LEVEL), storage(DEFAULT_LOG_STORAGE) {
}

//! Whether the two selections match the mode. Only the test in `test/logging/test_logging.cpp` calls this, and it needs
//! no DuckDB core type, so it stays here rather than becoming a binding into the Rust module.
bool LogConfig::IsConsistent() const {
	if (mode == LogMode::LEVEL_ONLY) {
		return enabled_log_types.empty() && disabled_log_types.empty();
	}
	if (mode == LogMode::DISABLE_SELECTED) {
		return enabled_log_types.empty() && !disabled_log_types.empty();
	}
	if (mode == LogMode::ENABLE_SELECTED) {
		return !enabled_log_types.empty() && disabled_log_types.empty();
	}
	return false;
}

//===--------------------------------------------------------------------===//
// RustLogManager
//===--------------------------------------------------------------------===//
RustLogManager::RustLogManager(void *ctx)
    : manager(ahirudb::logging::log_manager::new_log_manager(LogStorageBridge::Create(ctx))) {
}

RustLogManager::~RustLogManager() {
	ahirudb::logging::log_manager::delete_log_manager(manager);
}

//===--------------------------------------------------------------------===//
// Rust loggers
//===--------------------------------------------------------------------===//
template <class RUST_LOGGER>
RustLogger<RUST_LOGGER>::RustLogger(ahirudb::logging::LogManager &manager, const RegisteredLoggingContext &context,
                                    const ahirudb::logging::LogFilter &filter)
    : logger(RUST_LOGGER::new_(filter, ToRustContext(context), manager)) {
}

// Only the two loggers above are instantiated here
template class RustLogger<ahirudb::logging::ThreadSafeLogger>;
template class RustLogger<ahirudb::logging::MutableLogger>;

//===--------------------------------------------------------------------===//
// LogStorageBridge: the C-ABI trampolines that let the Rust LogManager drive the C++ LogStorage
//===--------------------------------------------------------------------===//
namespace {

//! An exception a failing trampoline caught, together with the [`CallScope`] that produced it
struct PendingStorageError {
	std::exception_ptr error;
	uint64_t call_scope = 0;
};

//! The per-thread state the call-scope bookkeeping needs. Every call into the Rust logging module is made on a single
//! thread and the storage calls are serialized by the Rust LogManager, so thread-local state is enough - and one
//! thread-local struct rather than three, so that a written log entry pays one guard-variable check for all of it
//! instead of one per piece (and so `CallScope` does not have to reach three of them to save and restore its state)
struct CallScopeState {
	PendingStorageError pending_error;
	uint64_t next_call_scope = 0;
	//! The [`CallScope`] that is currently running, or 0 when this thread is not inside one
	uint64_t active_call_scope = 0;
};

CallScopeState &CallScopeStates() {
	static thread_local CallScopeState state;
	return state;
}

PendingStorageError &PendingStorageErrorSlot() {
	return CallScopeStates().pending_error;
}

//! Runs `fn` on the C++ log storage, reporting success instead of letting an exception escape
template <class F>
bool CatchStorageException(F &&fn) {
	// A trampoline whose storage call has a result of its own cannot use this helper: the value would be dropped and
	// only the success flag survive. `CanScan` is such a case
	static_assert(std::is_void_v<decltype(fn())>, "CatchStorageException only reports whether the call threw");
	// A storage call that is not wrapped in `CallIntoRust` has no way to report its exception back to the C++ code that
	// caused it: the error would be recorded against the enclosing scope, or against none at all, and surface at an
	// unrelated later call
	D_ASSERT(CallScopeStates().active_call_scope != 0);
	try {
		fn();
		return true;
	} catch (...) {
		PendingStorageErrorSlot() = PendingStorageError {std::current_exception(), CallScopeStates().active_call_scope};
		return false;
	}
}

string ToStdString(const int8_t *data, uintptr_t length) {
	return string(reinterpret_cast<const char *>(data), length);
}

} // namespace

CallScope::CallScope() : index(++CallScopeStates().next_call_scope), previous(CallScopeStates().active_call_scope) {
	CallScopeStates().active_call_scope = index;
}

CallScope::~CallScope() {
	CallScopeStates().active_call_scope = previous;
}

void RethrowPendingStorageError(const CallScope &call_scope) {
	auto &slot = PendingStorageErrorSlot();
	if (!slot.error) {
		return;
	}
	if (slot.call_scope != call_scope.index) {
		// The failure belongs to a call that was not wrapped in `CallIntoRust` (hence scope 0). Re-throwing it here
		// would blame this statement for it, so it is dropped instead.
		slot = PendingStorageError();
		return;
	}
	auto error = slot.error;
	slot = PendingStorageError();
	std::rethrow_exception(error);
}

void ClearPendingStorageError() {
	PendingStorageErrorSlot() = PendingStorageError();
}

bool LogStorageBridge::WriteLogEntry(void *ctx, int64_t timestamp, uint8_t level, const int8_t *log_type,
                                     uintptr_t log_type_len, const int8_t *log_message, uintptr_t log_message_len,
                                     const ahirudb::logging::RegisteredLoggingContextLayout *context) {
	auto &log_manager = *reinterpret_cast<LogManager *>(ctx);
	return CatchStorageException([&]() {
		log_manager.WriteLogEntry(timestamp_t(timestamp), static_cast<LogLevel>(level),
		                          ToStdString(log_type, log_type_len), ToStdString(log_message, log_message_len),
		                          GetRegisteredLoggingContext(*context));
	});
}

bool LogStorageBridge::FlushAll(void *ctx) {
	auto &log_manager = *reinterpret_cast<LogManager *>(ctx);
	// Under the C++ lock that also guards `WriteLogEntry`, so that a storage switch cannot replace the storage in
	// between picking it and flushing it
	return CatchStorageException(
	    [&]() { log_manager.WithActiveLogStorage([](LogStorage &storage) { storage.FlushAll(); }); });
}

bool LogStorageBridge::Truncate(void *ctx) {
	auto &log_manager = *reinterpret_cast<LogManager *>(ctx);
	return CatchStorageException(
	    [&]() { log_manager.WithActiveLogStorage([](LogStorage &storage) { storage.Truncate(); }); });
}

bool LogStorageBridge::CanScan(void *ctx, uint8_t table) {
	auto &log_manager = *reinterpret_cast<LogManager *>(ctx);
	// `CanScan` has a result of its own, so it cannot be routed through `CatchStorageException` unchanged. A storage
	// that throws surfaces as an ordinary C++ exception out of `CallIntoRust`, in which case the answer is `false`
	// because nothing was reached; the storage's own answer only survives when the call succeeded
	bool can_scan = false;
	if (!CatchStorageException([&]() {
		    can_scan = log_manager.WithActiveLogStorage(
		        [&](LogStorage &storage) { return storage.CanScan(static_cast<LoggingTargetTable>(table)); });
	    })) {
		return false;
	}
	return can_scan;
}

ahirudb::logging::CLogStorage LogStorageBridge::Create(void *ctx) {
	return ahirudb::logging::CLogStorage::new_(ctx, WriteLogEntry, FlushAll, Truncate, CanScan);
}

} // namespace duckdb
