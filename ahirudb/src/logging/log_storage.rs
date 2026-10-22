//! Log storage interface, mirroring
//! `src/include/duckdb/logging/log_storage.hpp`.
//!
//! The concrete log storages manipulate DuckDB `DataChunk`s,
//! `ColumnDataCollection`s and `CSVWriter`s, none of which exist in Rust yet,
//! so the data plane stays in C++ and reaches the Rust [`LogManager`] through
//! [`CLogStorage`].
//!
//! # Errors
//! Nothing here reports failures back to the caller. The C++ storages throw,
//! and a `Result` in Rust could not carry such an exception across the boundary
//! anyway, so the bridge catches it and hands it to the C++ code that entered
//! Rust (see `CallIntoRust`): the error surfaces as an ordinary C++ exception
//! at the call site that caused it.
//!
//! `LogStorage::WriteLogEntries` (pushing a pre-built `DataChunk`) and
//! `LogStorage::UpdateConfig` (which consumes a
//! `case_insensitive_map_t<Value>`) are not modelled here: the former is not
//! implemented anywhere in C++ either (`BufferingLogStorage::WriteLogEntries`
//! throws), and the latter is driven from C++ because it needs DuckDB `Value`s.
//! `LogStorage::Flush(LoggingTargetTable)` is not modelled either: only
//! `LogManager::Flush` reaches a storage, and it flushes everything.
//!
//! `LogStorage::GetStorageName` is not modelled either: C++ records the name
//! the storage was *selected* with in
//! [`LogManager::record_log_storage_name`](super::log_manager::LogManager::record_log_storage_name) when it installs the
//! storage, and the only users of the name (the storage config error messages)
//! are produced in C++.
//!
//! `LogStorage::IsEnabled` has no user outside `log_storage.cpp` either:
//! `BufferingLogStorage` answers it from its own `IsEnabledInternal`, and the
//! `duckdb_logs` binding decides which tables to scan from the `CanScan` below
//! instead.

use std::ffi::{c_char, c_void};

use crate::logging::logging::{LogLevel, RegisteredLoggingContextLayout};

/// Logging storage can store entries normalized or denormalized. This enum
/// describes what a single table/file/etc contains.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoggingTargetTable {
    /// Denormalized: log entries consisting of both the full log entry and the
    /// context
    ALL_LOGS = 0,
    /// Normalized: contains only the log entries and a context_id
    LOG_ENTRIES = 1,
    /// Normalized: contains only the log contexts
    LOG_CONTEXTS = 2,
}

type WriteLogEntryFn = extern "C" fn(
    ctx: *mut c_void,
    timestamp: i64,
    level: u8,
    log_type: *const c_char,
    log_type_len: usize,
    log_message: *const c_char,
    log_message_len: usize,
    context: *const RegisteredLoggingContextLayout,
) -> bool;
type FlushAllFn = extern "C" fn(ctx: *mut c_void) -> bool;
type TruncateFn = extern "C" fn(ctx: *mut c_void) -> bool;
type CanScanFn = extern "C" fn(ctx: *mut c_void, table: u8) -> bool;

/// A `duckdb::LogStorage` reached through C++-supplied function pointers.
///
/// `ctx` is the address of the C++ `duckdb::LogManager`, which owns the storage
/// and forwards every call to whichever storage it currently has installed.
/// That is also why a `LogManager` builds this table once and never replaces
/// it: swapping it could not change where a write lands, only which lock it was
/// read under. `Copy`, because it is a bare function table - unlike an `Arc<dyn
/// LogStorage>`, which would add a refcount to every single log entry.
///
/// A `false` return from any of these means the storage threw.
#[derive(Clone, Copy)]
pub struct CLogStorage {
    ctx: *mut c_void,
    write_log_entry: WriteLogEntryFn,
    flush_all: FlushAllFn,
    truncate: TruncateFn,
    can_scan: CanScanFn,
}

// SAFETY: `ctx` points at the C++ LogManager, which outlives the Rust
// LogManager that owns this storage. Every call through it reaches the storage
// under the C++ `LogManager::lock`, so concurrent calls are serialized on that
// side.
unsafe impl Send for CLogStorage {}
unsafe impl Sync for CLogStorage {}

impl CLogStorage {
    pub fn new(
        ctx: *mut c_void,
        write_log_entry: WriteLogEntryFn,
        flush_all: FlushAllFn,
        truncate: TruncateFn,
        can_scan: CanScanFn,
    ) -> Self {
        Self {
            ctx,
            write_log_entry,
            flush_all,
            truncate,
            can_scan,
        }
    }

    pub fn write_log_entry(
        &self,
        timestamp: i64,
        level: LogLevel,
        log_type: &str,
        log_message: &str,
        context: &RegisteredLoggingContextLayout,
    ) {
        (self.write_log_entry)(
            self.ctx,
            timestamp,
            level as u8,
            log_type.as_ptr().cast::<c_char>(),
            log_type.len(),
            log_message.as_ptr().cast::<c_char>(),
            log_message.len(),
            context as *const RegisteredLoggingContextLayout,
        );
    }

    pub fn flush_all(&self) {
        (self.flush_all)(self.ctx);
    }

    pub fn truncate(&self) {
        (self.truncate)(self.ctx);
    }

    pub fn can_scan(&self, table: LoggingTargetTable) -> bool {
        (self.can_scan)(self.ctx, table as u8)
    }
}
