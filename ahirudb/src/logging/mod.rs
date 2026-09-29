#![allow(non_camel_case_types)]
#![allow(nonstandard_style)]

use std::cmp::min;
use std::collections::{HashMap, HashSet};
use std::ffi::{c_void, CStr};
use std::os::raw::c_char;
use std::sync::{Arc, Mutex, RwLock};

pub mod log_type;
pub mod logger;
pub mod log_storage;

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

pub type LogStorageRef = Arc<dyn LogStorage>;

/// Get current timestamp in microseconds since epoch
pub fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as i64
}

/// Full LogManager implementation
pub struct LogManager {
    lock: Mutex<()>,
    config: RwLock<LogConfig>,
    global_logger: Mutex<Option<Arc<dyn logger::Logger>>>,
    log_storage: Mutex<Option<LogStorageRef>>,
    next_registered_logging_context_index: Mutex<usize>,
    registered_log_storages: Mutex<HashMap<String, LogStorageRef>>,
    registered_log_types: Mutex<HashMap<String, Arc<dyn log_type::LogTypeRegistryEntry>>>,
}

impl LogManager {
    pub fn new() -> Arc<Self> {
        let manager = Arc::new(Self {
            lock: Mutex::new(()),
            config: RwLock::new(LogConfig::default()),
            global_logger: Mutex::new(None),
            log_storage: Mutex::new(None),
            next_registered_logging_context_index: Mutex::new(0),
            registered_log_storages: Mutex::new(HashMap::new()),
            registered_log_types: Mutex::new(HashMap::new()),
        });
        // Initialize with InMemoryLogStorage
        let storage = log_storage::InMemoryLogStorage::new();
        *manager.log_storage.lock().unwrap() = Some(storage.clone());
        manager
    }

    pub fn create_logger(&self, context: LoggingContext, thread_safe: bool, mutable_settings: bool) -> Box<dyn logger::Logger> {
        let _lock = self.lock.lock().unwrap();
        let registered_context = self.register_logging_context_internal(context);

        let config = self.config.read().unwrap().clone();

        if mutable_settings {
            return Box::new(logger::MutableLogger::new(config, registered_context, Arc::new(self.clone())));
        }
        if !config.enabled {
            return Box::new(logger::NopLogger::new(Arc::new(self.clone())));
        }
        if !config.enabled {
            return Box::new(logger::NopLogger::new(Arc::new(self.clone())));
        }
        Box::new(logger::ThreadSafeLogger::new(config, registered_context, Arc::new(self.clone())))
    }

    pub fn register_logging_context(&self, context: LoggingContext) -> RegisteredLoggingContext {
        self.register_logging_context_internal(context)
    }

    fn register_logging_context_internal(&self, context: LoggingContext) -> RegisteredLoggingContext {
        let mut idx = self.next_registered_logging_context_index.lock().unwrap();
        let result = RegisteredLoggingContext {
            context_id: *idx,
            context,
        };
        *idx += 1;
        result
    }

    pub fn register_log_storage(&self, name: &str, storage: LogStorageRef) -> bool {
        let mut storages = self.registered_log_storages.lock().unwrap();
        let name_lower = name.to_lowercase();
        if storages.contains_key(&name_lower) {
            return false;
        }
        storages.insert(name_lower, storage);
        true
    }

    pub fn global_logger(&self) -> Option<Arc<dyn logger::Logger>> {
        self.global_logger.lock().unwrap().clone()
    }

    pub fn set_global_logger(&self, logger: Arc<dyn logger::Logger>) {
        *self.global_logger.lock().unwrap() = Some(logger);
    }

    pub fn flush(&self) {
        let _lock = self.lock.lock().unwrap();
        if let Some(storage) = self.log_storage.lock().unwrap().as_ref() {
            storage.flush_all();
        }
    }

    pub fn get_log_storage(&self) -> Option<LogStorageRef> {
        self.log_storage.lock().unwrap().clone()
    }

    pub fn can_scan(&self, table: log_storage::LoggingTargetTable) -> bool {
        if let Some(storage) = self.log_storage.lock().unwrap().as_ref() {
            storage.can_scan(table)
        } else {
            false
        }
    }

    pub fn set_config(&self, config: LogConfig) {
        let mut config_lock = self.config.write().unwrap();
        *config_lock = config;
    }

    pub fn set_enable_logging(&self, enable: bool) {
        let mut config = self.config.write().unwrap();
        config.enabled = enable;
        // Update global logger config would go here
    }

    pub fn set_log_mode(&self, mode: LogMode) {
        let mut config = self.config.write().unwrap();
        config.mode = mode;
    }

    pub fn set_log_level(&self, level: LogLevel) {
        let mut config = self.config.write().unwrap();
        config.level = level;
    }

    pub fn set_enabled_log_types(&self, enabled_log_types: Option<HashSet<String>>) {
        let mut config = self.config.write().unwrap();
        config.enabled_log_types = enabled_log_types.unwrap_or_default();
    }

    pub fn set_disabled_log_types(&self, disabled_log_types: Option<HashSet<String>>) {
        let mut config = self.config.write().unwrap();
        config.disabled_log_types = disabled_log_types.unwrap_or_default();
    }

    pub fn set_log_storage(&self, storage_name: &str) -> Result<(), String> {
        let name_lower = storage_name.to_lowercase();
        let mut storages = self.registered_log_storages.lock().unwrap();
        
        if name_lower == LogConfig::FILE_STORAGE_NAME {
            // File storage requires path - reject direct switch
            return Err("Cannot select 'file' log storage via direct switch".to_string());
        }

        if let Some(storage) = storages.get(&name_lower).cloned() {
            *self.log_storage.lock().unwrap() = Some(storage);
            Ok(())
        } else if name_lower == LogConfig::IN_MEMORY_STORAGE_NAME {
            let storage = log_storage::InMemoryLogStorage::new();
            *self.log_storage.lock().unwrap() = Some(storage);
            Ok(())
        } else if name_lower == LogConfig::STDOUT_STORAGE_NAME {
            let storage = log_storage::StdOutLogStorage::new();
            *self.log_storage.lock().unwrap() = Some(storage);
            Ok(())
        } else if let Some(storage) = storages.get(&name_lower).cloned() {
            *self.log_storage.lock().unwrap() = Some(storage);
            Ok(())
        } else {
            Err(format!("Log storage '{}' is not yet registered", storage_name))
        }
    }

    pub fn update_log_storage_config(&self, config_value: HashMap<String, String>) {
        if let Some(storage) = self.log_storage.lock().unwrap().as_ref() {
            storage.update_config(config_value);
        }
    }

    pub fn set_enable_structured_loggers(&self, enabled_logger_types: Vec<String>) -> Result<(), String> {
        let mut config = self.config.write().unwrap();
        let mut new_config = config.clone();
        new_config.enabled_log_types.clear();

        let mut min_log_level = LogLevel::LOG_FATAL;

        for enabled_logger_type in enabled_logger_types {
            let lookup = self.lookup_log_type_internal(&enabled_logger_type);
            if lookup.is_none() {
                return Err(format!("Unknown log type: '{}'", enabled_logger_type));
            }
            let entry = lookup.unwrap();

            new_config.enabled_log_types.insert(entry.name().to_string());
            min_log_level = min(min_log_level, entry.level());
        }

        new_config.level = min_log_level;
        new_config.mode = LogMode::ENABLE_SELECTED;
        new_config.enabled = true;

        *config = new_config;
        Ok(())
    }

    pub fn truncate_log_storage(&self) {
        let _lock = self.lock.lock().unwrap();
        if let Some(storage) = self.log_storage.lock().unwrap().as_ref() {
            storage.truncate();
        }
    }

    pub fn get_config(&self) -> LogConfig {
        self.config.read().unwrap().clone()
    }

    pub fn lookup_log_type(&self, type_name: &str) -> Option<Arc<dyn log_type::LogTypeRegistryEntry>> {
        self.registered_log_types.lock().unwrap().get(type_name).cloned()
    }

    fn lookup_log_type_internal(&self, type_name: &str) -> Option<Arc<dyn log_type::LogTypeRegistryEntry>> {
        self.registered_log_types.lock().unwrap().get(type_name).cloned()
    }

    pub fn register_log_type(&self, log_type: Arc<dyn log_type::LogTypeRegistryEntry>) -> Result<(), String> {
        let mut types = self.registered_log_types.lock().unwrap();
        if types.contains_key(&log_type.name().to_string()) {
            return Err(format!("Registered log writer '{}' already exists", log_type.name()));
        }
        types.insert(log_type.name().to_string(), log_type);
        Ok(())
    }

    pub fn register_default_log_types(&self) {
        self.register_log_type(Arc::new(log_type::DefaultLogType::new())).unwrap();
        self.register_log_type(Arc::new(log_type::FileSystemLogType::new())).unwrap();
        self.register_log_type(Arc::new(log_type::HTTPLogType::new())).unwrap();
        self.register_log_type(Arc::new(log_type::QueryLogType::new())).unwrap();
        self.register_log_type(Arc::new(log_type::PhysicalOperatorLogType::new())).unwrap();
        self.register_log_type(Arc::new(log_type::MetricsLogType::new())).unwrap();
        self.register_log_type(Arc::new(log_type::AdaptiveFilterLogType::new())).unwrap();
        self.register_log_type(Arc::new(log_type::ParquetPrefetchLogType::new())).unwrap();
        self.register_log_type(Arc::new(log_type::AsyncTaskScheduleLogType::new())).unwrap();
        self.register_log_type(Arc::new(log_type::ExternalResourceLogType::new())).unwrap();
    }

    pub fn write_log_entry(
        &self,
        timestamp: i64,
        log_type: &str,
        level: LogLevel,
        message: &str,
        context: Option<&RegisteredLoggingContext>,
    ) {
        // Check for warning as error
        if level == LogLevel::LOG_WARNING {
            // Warning as error check would go here
        }

        let _lock = self.lock.lock().unwrap();
        if let Some(storage) = self.log_storage.lock().unwrap().as_ref() {
            storage.write_log_entry(timestamp, level, log_type, message, context);
        }
    }

    pub fn flush_cached_log_entries(&self, _chunk: &mut log_storage::DataChunk, _context: &RegisteredLoggingContext) {
        // Not implemented
    }
}

impl Clone for LogManager {
    fn clone(&self) -> Self {
        Self {
            lock: Mutex::new(()),
            config: RwLock::new(self.config.read().unwrap().clone()),
            global_logger: Mutex::new(self.global_logger.lock().unwrap().clone()),
            log_storage: Mutex::new(self.log_storage.lock().unwrap().clone()),
            next_registered_logging_context_index: Mutex::new(*self.next_registered_logging_context_index.lock().unwrap()),
            registered_log_storages: Mutex::new(self.registered_log_storages.lock().unwrap().clone()),
            registered_log_types: Mutex::new(self.registered_log_types.lock().unwrap().clone()),
        }
    }
}

// C-compatible logging functions
#[unsafe(no_mangle)]
pub extern "C" fn ahirudb_logging_write_log(
    log_type: *const c_char,
    level: u8,
    message: *const c_char,
) {
    let log_type_str = unsafe { CStr::from_ptr(log_type).to_string_lossy() };
    let message_str = unsafe { CStr::from_ptr(message).to_string_lossy() };
    let level = LogLevel::from(level);
    eprintln!("[{:?}] {}: {}", level, log_type_str, message_str);
}

#[unsafe(no_mangle)]
pub extern "C" fn ahirudb_logging_write_log_with_context(
    log_type: *const c_char,
    level: u8,
    message: *const c_char,
    scope: u8,
    thread_id: u64,
    connection_id: u64,
    transaction_id: u64,
    query_id: u64,
) {
    let log_type_str = unsafe { CStr::from_ptr(log_type).to_string_lossy() };
    let message_str = unsafe { CStr::from_ptr(message).to_string_lossy() };
    let level = LogLevel::from(level);
    eprintln!(
        "[{:?}] {}: {} (scope={}, thread={}, conn={}, txn={}, query={})",
        level, log_type_str, message_str, scope, thread_id, connection_id, transaction_id, query_id
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn ahirudb_logging_current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as i64
}

// Re-export log types
pub use log_type::*;
pub use logger::{LogContextSource, MutableLogger, NopLogger, ThreadLocalLogger, ThreadSafeLogger};
pub use log_storage::{LoggingTargetTable, LogStorage, InMemoryLogStorage, StdOutLogStorage, DataChunk};

// C-compatible functions for C++ interop
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ahirudb_logging_log_manager_new() -> *mut c_void {
    let manager = Arc::new(LogManager::new());
    Arc::into_raw(manager) as *mut c_void
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ahirudb_logging_write_log_entry(
    timestamp: i64,
    log_type: *const c_char,
    level: u8,
    message: *const c_char,
) {
    let log_type_str = unsafe { CStr::from_ptr(log_type).to_string_lossy() };
    let message_str = unsafe { CStr::from_ptr(message).to_string_lossy() };
    let level = match level {
        10 => LogLevel::LOG_TRACE,
        20 => LogLevel::LOG_DEBUG,
        30 => LogLevel::LOG_INFO,
        40 => LogLevel::LOG_WARNING,
        50 => LogLevel::LOG_ERROR,
        60 => LogLevel::LOG_FATAL,
        _ => LogLevel::LOG_INFO,
    };
    eprintln!("[{:?}] {}: {}", level, log_type_str, message_str);
}
