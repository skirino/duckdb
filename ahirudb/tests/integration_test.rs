use ahirudb::logging::{
    LogConfig, LogContextScope, LogContextSource, LogLevel, LogManager, LogMode, LoggingContext,
    RegisteredLoggingContext, ThreadSafeLogger, logger::Logger,
};

#[test]
fn test_log_config_default() {
    let config = LogConfig::default();
    assert!(!config.enabled);
    assert_eq!(config.mode, LogMode::LEVEL_ONLY);
    assert_eq!(config.level, LogLevel::LOG_INFO);
    assert_eq!(config.storage, "memory");
    assert!(config.enabled_log_types.is_empty());
    assert!(config.disabled_log_types.is_empty());
}

#[test]
fn test_log_config_is_consistent() {
    let mut config = LogConfig::default();
    assert!(config.is_consistent());

    // Test DISABLE_SELECTED
    config.mode = LogMode::DISABLE_SELECTED;
    config.disabled_log_types.insert("test".to_string());
    assert!(config.is_consistent());

    // Test ENABLE_SELECTED
    config.mode = LogMode::ENABLE_SELECTED;
    config.enabled_log_types.insert("test".to_string());
    config.disabled_log_types.clear();
    assert!(config.is_consistent());
}

#[test]
fn test_logging_context() {
    let ctx = LoggingContext::new(LogContextScope::CONNECTION);
    assert_eq!(ctx.scope, LogContextScope::CONNECTION);
    assert!(ctx.thread_id.is_none());
    assert!(ctx.connection_id.is_none());
    assert!(ctx.transaction_id.is_none());
    assert!(ctx.query_id.is_none());
}

#[test]
fn test_log_level_from_u8() {
    assert_eq!(LogLevel::from(10), LogLevel::LOG_TRACE);
    assert_eq!(LogLevel::from(20), LogLevel::LOG_DEBUG);
    assert_eq!(LogLevel::from(30), LogLevel::LOG_INFO);
    assert_eq!(LogLevel::from(40), LogLevel::LOG_WARNING);
    assert_eq!(LogLevel::from(50), LogLevel::LOG_ERROR);
    assert_eq!(LogLevel::from(60), LogLevel::LOG_FATAL);
    assert_eq!(LogLevel::from(99), LogLevel::LOG_INFO);
}

#[test]
fn test_log_mode_from_u8() {
    assert_eq!(LogMode::from(0), LogMode::LEVEL_ONLY);
    assert_eq!(LogMode::from(1), LogMode::DISABLE_SELECTED);
    assert_eq!(LogMode::from(2), LogMode::ENABLE_SELECTED);
    assert_eq!(LogMode::from(99), LogMode::LEVEL_ONLY);
}

#[test]
fn test_registered_logging_context() {
    let ctx = LoggingContext::new(LogContextScope::DATABASE);
    let registered = RegisteredLoggingContext {
        context_id: 42,
        context: ctx,
    };
    assert_eq!(registered.context_id, 42);
    assert_eq!(registered.context.scope, LogContextScope::DATABASE);
}

#[test]
fn test_log_manager_creation() {
    let manager = LogManager::new();
    // Just verify it can be created
    let _ = manager;
}

#[test]
fn test_thread_safe_logger() {
    let manager = std::sync::Arc::new(LogManager::new());
    let config = LogConfig::default();
    let context = RegisteredLoggingContext {
        context_id: 0,
        context: LoggingContext::new(LogContextScope::CONNECTION),
    };

    let logger = ThreadSafeLogger::new(config, context, manager);

    // Test should_log
    assert!(!logger.should_log("test", LogLevel::LOG_INFO));

    // Enable logging
    let mut config = LogConfig::default();
    config.enabled = true;
    let context = RegisteredLoggingContext {
        context_id: 0,
        context: LoggingContext::new(LogContextScope::CONNECTION),
    };
    let logger = ThreadSafeLogger::new(config, context, std::sync::Arc::new(LogManager::new()));
    assert!(logger.should_log("test", LogLevel::LOG_INFO));
    assert!(!logger.should_log("test", LogLevel::LOG_DEBUG)); // Below INFO level
}

#[test]
fn test_log_context_source() {
    struct TestSource {
        logger: ThreadSafeLogger,
    }

    impl LogContextSource for TestSource {
        fn get_logger(&self) -> &dyn ahirudb::logging::logger::Logger {
            &self.logger
        }
    }

    let manager = std::sync::Arc::new(LogManager::new());
    let config = LogConfig::default();
    let context = RegisteredLoggingContext {
        context_id: 0,
        context: LoggingContext::new(LogContextScope::CONNECTION),
    };
    let logger = ThreadSafeLogger::new(config, context, manager);
    let source = TestSource { logger };

    let _ = source.get_logger();
}
