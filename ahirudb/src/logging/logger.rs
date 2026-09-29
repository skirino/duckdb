#![allow(non_camel_case_types)]
#![allow(nonstandard_style)]

use super::{
    current_timestamp, LogConfig, LogLevel, LogManager, LogMode, LoggingContext,
    RegisteredLoggingContext,
};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Trait for all loggers
pub trait Logger: Send + Sync {
    fn should_log(&self, log_type: &str, log_level: LogLevel) -> bool;
    fn write_log(&self, log_type: &str, log_level: LogLevel, message: &str);
    fn flush(&self);
    fn is_thread_safe(&self) -> bool;
    fn is_mutable(&self) -> bool {
        false
    }
    fn update_config(&mut self, _new_config: LogConfig) {
        panic!("Cannot update the config of this logger!");
    }
    fn get_config(&self) -> &LogConfig;

    // Helper methods for different string types
    fn write_log_str(&self, log_type: &str, log_level: LogLevel, message: &str) {
        self.write_log(log_type, log_level, message);
    }

    fn write_log_formatted(
        &self,
        log_type: &str,
        log_level: LogLevel,
        format_string: &str,
        _args: &[&dyn std::fmt::Display],
    ) {
        // Simple formatting - in real implementation would use format! macro
        self.write_log(log_type, log_level, format_string);
    }
}

/// Trait for types that can provide a logger
pub trait LogContextSource {
    fn get_logger(&self) -> &dyn Logger;
}

// Thread-safe wrapper for LogManager
type LogManagerRef = Arc<LogManager>;

// Thread-safe logger
pub struct ThreadSafeLogger {
    config: LogConfig,
    lock: Mutex<()>,
    context: RegisteredLoggingContext,
    manager: Arc<LogManager>,
}

impl ThreadSafeLogger {
    pub fn new(
        config: LogConfig,
        context: RegisteredLoggingContext,
        manager: Arc<LogManager>,
    ) -> Self {
        Self {
            config,
            lock: Mutex::new(()),
            context,
            manager,
        }
    }

    pub fn new_with_logging_context(
        config: LogConfig,
        context: LoggingContext,
        manager: Arc<LogManager>,
    ) -> Self {
        let registered = RegisteredLoggingContext {
            context_id: 0,
            context,
        };
        Self::new(config, registered, manager)
    }
}

impl Logger for ThreadSafeLogger {
    fn should_log(&self, log_type: &str, log_level: LogLevel) -> bool {
        let config = &self.config;
        if !config.enabled {
            return false;
        }
        if (log_level as u8) < (config.level as u8) {
            return false;
        }
        match config.mode {
            LogMode::LEVEL_ONLY => true,
            LogMode::DISABLE_SELECTED => !config.disabled_log_types.contains(log_type),
            LogMode::ENABLE_SELECTED => config.enabled_log_types.contains(log_type),
        }
    }

    fn write_log(&self, log_type: &str, log_level: LogLevel, message: &str) {
        if !self.should_log(log_type, log_level) {
            return;
        }
        let _guard = self.lock.lock().unwrap();
        self.manager.write_log_entry(
            current_timestamp(),
            log_type,
            log_level,
            message,
            Some(&self.context),
        );
    }

    fn flush(&self) {
        self.manager.flush();
    }

    fn is_thread_safe(&self) -> bool {
        true
    }

    fn get_config(&self) -> &LogConfig {
        &self.config
    }
}

// Non Thread-safe logger (caches locally)
pub struct ThreadLocalLogger {
    config: LogConfig,
    context: RegisteredLoggingContext,
    manager: Arc<LogManager>,
}

impl ThreadLocalLogger {
    pub fn new(
        config: LogConfig,
        context: RegisteredLoggingContext,
        manager: Arc<LogManager>,
    ) -> Self {
        Self {
            config,
            context,
            manager,
        }
    }

    pub fn new_with_logging_context(
        config: LogConfig,
        context: LoggingContext,
        manager: Arc<LogManager>,
    ) -> Self {
        let registered = RegisteredLoggingContext {
            context_id: 0,
            context,
        };
        Self::new(config, registered, manager)
    }
}

impl Logger for ThreadLocalLogger {
    fn should_log(&self, log_type: &str, log_level: LogLevel) -> bool {
        let config = &self.config;
        if !config.enabled {
            return false;
        }
        if (log_level as u8) < (config.level as u8) {
            return false;
        }
        match config.mode {
            LogMode::LEVEL_ONLY => true,
            LogMode::DISABLE_SELECTED => !config.disabled_log_types.contains(log_type),
            LogMode::ENABLE_SELECTED => config.enabled_log_types.contains(log_type),
        }
    }

    fn write_log(&self, log_type: &str, log_level: LogLevel, message: &str) {
        if !self.should_log(log_type, log_level) {
            return;
        }
        self.manager.write_log_entry(
            current_timestamp(),
            log_type,
            log_level,
            message,
            Some(&self.context),
        );
    }

    fn flush(&self) {
        self.manager.flush();
    }

    fn is_thread_safe(&self) -> bool {
        false
    }

    fn get_config(&self) -> &LogConfig {
        &self.config
    }
}

// Thread-safe Logger with mutable log settings
pub struct MutableLogger {
    enabled: AtomicBool,
    mode: AtomicU8,
    level: AtomicU8,
    lock: Mutex<()>,
    config: LogConfig,
    context: RegisteredLoggingContext,
    manager: Arc<LogManager>,
}

impl MutableLogger {
    pub fn new(
        config: LogConfig,
        context: RegisteredLoggingContext,
        manager: Arc<LogManager>,
    ) -> Self {
        let enabled = AtomicBool::new(config.enabled);
        let mode = AtomicU8::new(config.mode as u8);
        let level = AtomicU8::new(config.level as u8);
        Self {
            enabled,
            mode,
            level,
            lock: Mutex::new(()),
            config,
            context,
            manager,
        }
    }

    pub fn new_with_logging_context(
        config: LogConfig,
        context: LoggingContext,
        manager: Arc<LogManager>,
    ) -> Self {
        let registered = RegisteredLoggingContext {
            context_id: 0,
            context,
        };
        Self::new(config, registered, manager)
    }
}

impl Logger for MutableLogger {
    fn should_log(&self, log_type: &str, log_level: LogLevel) -> bool {
        if !self.enabled.load(Ordering::Acquire) {
            return false;
        }
        let level = LogLevel::from(self.level.load(Ordering::Acquire));
        if (log_level as u8) < (level as u8) {
            return false;
        }
        let mode = LogMode::from(self.mode.load(Ordering::Acquire));
        let config = &self.config;
        match mode {
            LogMode::LEVEL_ONLY => true,
            LogMode::DISABLE_SELECTED => !config.disabled_log_types.contains(log_type),
            LogMode::ENABLE_SELECTED => config.enabled_log_types.contains(log_type),
        }
    }

    fn write_log(&self, log_type: &str, log_level: LogLevel, message: &str) {
        if !self.should_log(log_type, log_level) {
            return;
        }
        let _guard = self.lock.lock().unwrap();
        self.manager.write_log_entry(
            current_timestamp(),
            log_type,
            log_level,
            message,
            Some(&self.context),
        );
    }

    fn flush(&self) {
        self.manager.flush();
    }

    fn is_thread_safe(&self) -> bool {
        true
    }

    fn is_mutable(&self) -> bool {
        true
    }

    fn get_config(&self) -> &LogConfig {
        &self.config
    }

    fn update_config(&mut self, new_config: LogConfig) {
        self.enabled.store(new_config.enabled, Ordering::Release);
        self.mode.store(new_config.mode as u8, Ordering::Release);
        self.level.store(new_config.level as u8, Ordering::Release);
        let _config_guard = self.lock.lock().unwrap();
        self.config = new_config;
    }
}

// For when logging is disabled: NOPs everything
pub struct NopLogger {
    manager: Arc<LogManager>,
}

impl NopLogger {
    pub fn new(manager: Arc<LogManager>) -> Self {
        Self { manager }
    }
}

impl Logger for NopLogger {
    fn should_log(&self, _log_type: &str, _log_level: LogLevel) -> bool {
        false
    }

    fn write_log(&self, _log_type: &str, _log_level: LogLevel, _message: &str) {
        // NOP
    }

    fn flush(&self) {
        // NOP
    }

    fn is_thread_safe(&self) -> bool {
        true
    }

    fn get_config(&self) -> &LogConfig {
        panic!("Called GetConfig on NopLogger");
    }
}
