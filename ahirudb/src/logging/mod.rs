//! Logging module: a Rust port of DuckDB's `src/logging` module.
//!
//! The module mirrors the C++ structure:
//! - [`logging`]: core types (levels, config, contexts)
//! - [`logger`]: the logger implementations
//! - [`log_manager`]: global logging state and configuration handling
//! - [`log_storage`]: the C++-ABI sink a `duckdb::LogStorage` is reached
//!   through
//! - [`log_type`]: the log type registry

// The names of the log levels, modes, scopes and target tables are kept identical to their C++
// counterparts.
#![allow(non_camel_case_types)]
#![allow(nonstandard_style)]
// `logging::logging` mirrors `duckdb/logging/logging.hpp`
#![allow(clippy::module_inception)]

pub mod log_manager;
pub mod log_storage;
pub mod log_type;
pub mod logger;
pub mod logging;

#[cfg(test)]
mod test_support;

pub use log_manager::{
    CreatedLogger,
    LogManager,
    StorageSwitchError,
    StorageSwitchErrorKind,
};
pub use log_storage::{
    CLogStorage,
    LoggingTargetTable,
};
pub use logger::{
    LoggerKind,
    MutableLogger,
    ThreadSafeLogger,
};
pub use logging::{
    LogConfig,
    LogConfigSnapshot,
    LogFilter,
    LogLevel,
    LogMode,
    RegisteredLoggingContextLayout,
    current_timestamp,
    log_config_snapshot,
};
