#![allow(non_camel_case_types)]
#![allow(nonstandard_style)]

use std::collections::HashSet;

pub mod log_type;
pub mod logger;

// Note: `LOG_` prefix is to avoid problems with DEBUG and ERROR macros
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LogLevel {
    LOG_TRACE = 10,
    LOG_DEBUG = 20,
    LOG_INFO = 30,
    LOG_WARNING = 40,
    LOG_ERROR = 50,
    LOG_FATAL = 60,
}

impl Default for LogLevel {
    fn default() -> Self {
        LogLevel::LOG_INFO
    }
}

impl From<u8> for LogLevel {
    fn from(value: u8) -> Self {
        match value {
            10 => LogLevel::LOG_TRACE,
            20 => LogLevel::LOG_DEBUG,
            30 => LogLevel::LOG_INFO,
            40 => LogLevel::LOG_WARNING,
            50 => LogLevel::LOG_ERROR,
            60 => LogLevel::LOG_FATAL,
            _ => LogLevel::LOG_INFO,
        }
    }
}

impl From<LogLevel> for u8 {
    fn from(level: LogLevel) -> Self {
        level as u8
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogContextScope {
    DATABASE = 10,
    CONNECTION = 20,
    THREAD = 30,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogMode {
    LEVEL_ONLY = 0,
    DISABLE_SELECTED = 1,
    ENABLE_SELECTED = 2,
}

impl From<u8> for LogMode {
    fn from(value: u8) -> Self {
        match value {
            0 => LogMode::LEVEL_ONLY,
            1 => LogMode::DISABLE_SELECTED,
            2 => LogMode::ENABLE_SELECTED,
            _ => LogMode::LEVEL_ONLY,
        }
    }
}

impl From<LogMode> for u8 {
    fn from(mode: LogMode) -> Self {
        mode as u8
    }
}

#[derive(Debug, Clone)]
pub struct LogConfig {
    pub enabled: bool,
    pub mode: LogMode,
    pub level: LogLevel,
    pub storage: String,
    pub enabled_log_types: HashSet<String>,
    pub disabled_log_types: HashSet<String>,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: LogMode::LEVEL_ONLY,
            level: LogLevel::default(),
            storage: LogConfig::IN_MEMORY_STORAGE_NAME.to_string(),
            enabled_log_types: HashSet::new(),
            disabled_log_types: HashSet::new(),
        }
    }
}

impl LogConfig {
    pub const IN_MEMORY_STORAGE_NAME: &'static str = "memory";
    pub const STDOUT_STORAGE_NAME: &'static str = "stdout";
    pub const FILE_STORAGE_NAME: &'static str = "file";
    pub const DEFAULT_LOG_LEVEL: LogLevel = LogLevel::LOG_INFO;
    pub const DEFAULT_LOG_STORAGE: &'static str = Self::IN_MEMORY_STORAGE_NAME;

    pub fn is_consistent(&self) -> bool {
        match self.mode {
            LogMode::LEVEL_ONLY => {
                self.enabled_log_types.is_empty() && self.disabled_log_types.is_empty()
            }
            LogMode::DISABLE_SELECTED => {
                self.enabled_log_types.is_empty() && !self.disabled_log_types.is_empty()
            }
            LogMode::ENABLE_SELECTED => {
                !self.enabled_log_types.is_empty() && self.disabled_log_types.is_empty()
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoggingContext {
    pub scope: LogContextScope,
    pub thread_id: Option<usize>,
    pub connection_id: Option<usize>,
    pub transaction_id: Option<usize>,
    pub query_id: Option<usize>,
}

impl LoggingContext {
    pub fn new(scope: LogContextScope) -> Self {
        Self {
            scope,
            thread_id: None,
            connection_id: None,
            transaction_id: None,
            query_id: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegisteredLoggingContext {
    pub context_id: usize,
    pub context: LoggingContext,
}

/// Get current timestamp in microseconds since epoch
pub fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as i64
}

/// Forward declaration for LogManager
pub struct LogManager {
    // Will be implemented later
}

impl LogManager {
    pub fn new() -> Self {
        Self {}
    }

    pub fn write_log_entry(
        &self,
        _timestamp: i64,
        _level: LogLevel,
        _log_type: &str,
        _message: &str,
        _context: &RegisteredLoggingContext,
    ) {
        // TODO: implement
    }

    pub fn flush(&self) {
        // TODO: implement
    }
}

// Re-export log types
pub use log_type::*;
pub use logger::{LogContextSource, MutableLogger, NopLogger, ThreadLocalLogger, ThreadSafeLogger};
