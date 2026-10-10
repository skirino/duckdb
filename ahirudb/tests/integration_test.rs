//! End-to-end tests for the Rust logging port: a `LogManager` wired to an
//! in-Rust storage through the C-ABI bridge, exercising the same behaviour as
//! `test/sql/logging/*.test` on the C++ side.

use std::{
    ffi::c_void,
    sync::Mutex,
};

use ahirudb::logging::{
    CLogStorage,
    LogConfig,
    LogLevel,
    LogManager,
    LogMode,
    MutableLogger,
    RegisteredLoggingContextLayout,
    StorageSwitchErrorKind,
    ThreadSafeLogger,
    log_storage::LoggingTargetTable,
};

#[derive(Debug, PartialEq, Eq, Clone)]
struct WrittenEntry {
    timestamp: i64,
    level: LogLevel,
    log_type: String,
    message: String,
    context_id: u64,
}

#[derive(Debug, Default)]
struct InMemoryStorage {
    entries: Mutex<Vec<WrittenEntry>>,
    flush_count: Mutex<usize>,
    truncate_count: Mutex<usize>,
}

impl InMemoryStorage {
    fn write_log_entry(
        &self,
        timestamp: i64,
        level: LogLevel,
        log_type: &str,
        log_message: &str,
        context: &RegisteredLoggingContextLayout,
    ) {
        self.entries.lock().unwrap().push(WrittenEntry {
            timestamp,
            level,
            log_type: log_type.to_string(),
            message: log_message.to_string(),
            context_id: context.context_id,
        });
    }

    fn flush_all(&self) { *self.flush_count.lock().unwrap() += 1; }

    fn truncate(&self) {
        *self.truncate_count.lock().unwrap() += 1;
        self.entries.lock().unwrap().clear();
    }
}

/// Turns a borrowed byte slice back into a `&str` without allocating, mirroring
/// what the C++ bridge does.
unsafe fn to_str<'a>(bytes: *const i8, len: usize) -> &'a str {
    unsafe { std::str::from_utf8(std::slice::from_raw_parts(bytes.cast::<u8>(), len)) }.unwrap()
}

/// Reports `false` when the storage call fails, which is how the C++ bridge
/// signals that it caught an exception.
extern fn c_write_log_entry(
    ctx: *mut c_void,
    timestamp: i64,
    level: u8,
    log_type: *const i8,
    log_type_len: usize,
    log_message: *const i8,
    log_message_len: usize,
    context: *const RegisteredLoggingContextLayout,
) -> bool {
    let storage = unsafe { &*(ctx as *const InMemoryStorage) };
    storage.write_log_entry(
        timestamp,
        LogLevel::from(level),
        unsafe { to_str(log_type, log_type_len) },
        unsafe { to_str(log_message, log_message_len) },
        unsafe { &*context },
    );
    true
}

extern fn c_flush_all(ctx: *mut c_void) -> bool {
    unsafe { &*(ctx as *const InMemoryStorage) }.flush_all();
    true
}

extern fn c_truncate(ctx: *mut c_void) -> bool {
    unsafe { &*(ctx as *const InMemoryStorage) }.truncate();
    true
}

// Turns the raw byte the C ABI hands back into the enum. C++ passes the target
// table in as a `u8` and only takes a `bool` out, so the crate itself never has
// to go the other way.
fn target_table(value: u8) -> LoggingTargetTable {
    match value
    {
        0 => LoggingTargetTable::ALL_LOGS,
        1 => LoggingTargetTable::LOG_ENTRIES,
        2 => LoggingTargetTable::LOG_CONTEXTS,
        _ => LoggingTargetTable::ALL_LOGS,
    }
}

extern fn c_can_scan(_ctx: *mut c_void, table: u8) -> bool {
    matches!(target_table(table), LoggingTargetTable::LOG_ENTRIES)
}

fn c_log_storage(storage: &InMemoryStorage) -> CLogStorage {
    CLogStorage::new(
        storage as *const InMemoryStorage as *mut c_void,
        c_write_log_entry,
        c_flush_all,
        c_truncate,
        c_can_scan,
    )
}

/// The `LogContextScope` values C++ uses
const DATABASE_SCOPE: u8 = 10;
const CONNECTION_SCOPE: u8 = 20;
const THREAD_SCOPE: u8 = 30;

/// The context C++ hands over when it registers a logging context. `context_id`
/// is what the LogManager assigns, so the value the caller passes in does not
/// matter.
fn context_layout(scope: u8) -> RegisteredLoggingContextLayout {
    RegisteredLoggingContextLayout {
        context_id: 0,
        scope,
        has_thread_id: false,
        has_connection_id: false,
        has_transaction_id: false,
        has_query_id: false,
        thread_id: 0,
        connection_id: 0,
        transaction_id: 0,
        query_id: 0,
    }
}

fn register_log_types(manager: &LogManager) {
    for (name, level) in [("QueryLog", LogLevel::LOG_INFO), ("FileSystem", LogLevel::LOG_TRACE)]
    {
        manager.register_log_type(name, level as u8).unwrap();
    }
}

/// Builds the logger the LogManager decided on, exactly like the C++
/// `LogManager::CreateLogger` does: from the filter it returned, so that there
/// is a single config read per logger.
fn thread_safe_logger(manager: &LogManager) -> ThreadSafeLogger {
    let created = manager
        .create_logger(context_layout(DATABASE_SCOPE), false)
        .unwrap();
    ThreadSafeLogger::new(
        &created.filter.expect("a thread-safe logger needs a filter"),
        created.context,
        manager,
    )
}

fn mutable_logger(manager: &LogManager) -> MutableLogger {
    let created = manager
        .create_logger(context_layout(DATABASE_SCOPE), true)
        .unwrap();
    MutableLogger::new(
        &created.filter.expect("a mutable logger needs a filter"),
        created.context,
        manager,
    )
}

/// The storage is leaked because the C++-style bridge passes a raw pointer to
/// it, which has to stay valid for as long as the manager that uses it.
fn make_manager() -> (&'static InMemoryStorage, *mut LogManager) {
    let storage = Box::leak(Box::new(InMemoryStorage::default()));
    let manager = LogManager::new(c_log_storage(storage));
    register_log_types(&manager);
    // What C++ does after installing a storage: record its name in the config
    manager.record_log_storage_name("test_log_storage");
    (storage, Box::into_raw(Box::new(manager)))
}

fn destroy_manager(manager: *mut LogManager) { unsafe { drop(Box::from_raw(manager)) }; }

#[test]
fn log_config_default_matches_cpp() {
    let config = LogConfig::default();
    assert!(!config.enabled);
    assert_eq!(config.mode, LogMode::LEVEL_ONLY);
    assert_eq!(config.level, LogLevel::LOG_INFO);
    assert_eq!(config.storage, LogConfig::IN_MEMORY_STORAGE_NAME);
    assert!(config.enabled_log_types.is_empty());
    assert!(config.disabled_log_types.is_empty());
}

#[test]
fn log_levels_and_modes_roundtrip() {
    for level in [
        LogLevel::LOG_TRACE,
        LogLevel::LOG_DEBUG,
        LogLevel::LOG_INFO,
        LogLevel::LOG_WARNING,
        LogLevel::LOG_ERROR,
        LogLevel::LOG_FATAL,
    ]
    {
        assert_eq!(LogLevel::from(level as u8), level);
    }
    for mode in [LogMode::LEVEL_ONLY, LogMode::DISABLE_SELECTED, LogMode::ENABLE_SELECTED]
    {
        assert_eq!(LogMode::from(mode as u8), mode);
    }
}

#[test]
fn level_only_logging_writes_every_type() {
    let (storage, manager) = make_manager();
    let manager_ref = unsafe { &*manager };
    manager_ref.set_enable_logging(true);
    manager_ref.set_log_level(LogLevel::LOG_WARNING as u8);

    let created = manager_ref
        .create_logger(context_layout(DATABASE_SCOPE), false)
        .unwrap();
    assert_eq!(created.kind, ahirudb::logging::LoggerKind::ThreadSafe);
    assert_eq!(created.context.context_id, 0);
    let rust_logger = ThreadSafeLogger::new(
        &created.filter.expect("a thread-safe logger needs a filter"),
        created.context,
        manager_ref,
    );

    assert!(!rust_logger.should_log(b"QueryLog", LogLevel::LOG_INFO));
    assert!(rust_logger.should_log(b"QueryLog", LogLevel::LOG_WARNING));

    rust_logger.write_log_internal("QueryLog", LogLevel::LOG_ERROR, "hello");
    rust_logger.flush();

    let entries = storage.entries.lock().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].log_type, "QueryLog");
    assert_eq!(entries[0].message, "hello");
    assert_eq!(entries[0].level, LogLevel::LOG_ERROR);
    assert_eq!(entries[0].context_id, 0);
    assert!(entries[0].timestamp > 0);
    drop(entries);
    assert!(*storage.flush_count.lock().unwrap() >= 1);

    destroy_manager(manager);
}

/// The Rust side of the bridge passes `(pointer, length)`, so a message is not
/// cut short at a NUL byte. This is a statement about the boundary inside this
/// crate only: C++ hands the message to `RustStr` as a `const char *`, which
/// reads it as a NUL-terminated string, so a message that reaches a C++ storage
/// through it is still truncated there.
#[test]
fn messages_with_interior_nul_reach_the_storage_intact() {
    let (storage, manager) = make_manager();
    let manager_ref = unsafe { &*manager };
    manager_ref.set_enable_logging(true);

    let rust_logger = thread_safe_logger(manager_ref);
    rust_logger.write_log_internal("QueryLog", LogLevel::LOG_INFO, "before\0after");

    let entries = storage.entries.lock().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].message, "before\0after");
    drop(entries);

    destroy_manager(manager);
}

#[test]
fn logging_is_disabled_hands_out_the_nop_kind() {
    let (storage, manager) = make_manager();
    let manager_ref = unsafe { &*manager };

    let created = manager_ref
        .create_logger(context_layout(CONNECTION_SCOPE), false)
        .unwrap();
    // C++ turns this into its own `NopLogger`, which never reaches Rust
    assert_eq!(created.kind, ahirudb::logging::LoggerKind::Nop);
    assert_eq!(created.context.context_id, 0);

    manager_ref.set_enable_logging(true);
    assert!(thread_safe_logger(manager_ref).should_log(b"QueryLog", LogLevel::LOG_INFO));
    manager_ref.set_enable_logging(false);

    assert!(storage.entries.lock().unwrap().is_empty());

    destroy_manager(manager);
}

#[test]
fn mutable_logger_picks_up_config_changes() {
    let (storage, manager) = make_manager();
    let manager_ref = unsafe { &*manager };

    manager_ref.set_enable_logging(true);
    manager_ref.set_log_level(LogLevel::LOG_ERROR as u8);
    manager_ref.set_log_mode(LogMode::DISABLE_SELECTED as u8);
    manager_ref.set_disabled_log_types(&["QueryLog"]);

    let created = manager_ref
        .create_logger(context_layout(DATABASE_SCOPE), true)
        .unwrap();
    assert_eq!(created.kind, ahirudb::logging::LoggerKind::Mutable);
    let rust_logger = MutableLogger::new(
        &created.filter.expect("a mutable logger needs a filter"),
        created.context,
        manager_ref,
    );

    assert!(!rust_logger.should_log(b"QueryLog", LogLevel::LOG_INFO));
    assert!(rust_logger.should_log(b"FileSystem", LogLevel::LOG_FATAL));
    assert!(!rust_logger.should_log(b"QueryLog", LogLevel::LOG_FATAL));
    assert!(!rust_logger.should_log(b"FileSystem", LogLevel::LOG_DEBUG));

    rust_logger.write_log_internal("FileSystem", LogLevel::LOG_FATAL, "written");
    let entries = storage.entries.lock().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].message, "written");
    drop(entries);

    // ENABLE_SELECTED restricts logging to the enabled types
    manager_ref.set_enabled_log_types(&["FileSystem"]);
    manager_ref.set_disabled_log_types(&[]);
    manager_ref.set_log_mode(LogMode::ENABLE_SELECTED as u8);
    let rust_logger = mutable_logger(manager_ref);
    assert!(rust_logger.should_log(b"FileSystem", LogLevel::LOG_FATAL));
    assert!(!rust_logger.should_log(b"QueryLog", LogLevel::LOG_FATAL));

    destroy_manager(manager);
}

#[test]
fn structured_loggers_use_the_lowest_registered_level() {
    let (storage, manager) = make_manager();
    let manager_ref = unsafe { &*manager };

    assert!(
        manager_ref
            .set_enable_structured_loggers(&["QueryLog", "filesystem"])
            .is_ok()
    );
    let config = manager_ref.get_config();
    assert!(config.enabled);
    assert_eq!(config.mode, LogMode::ENABLE_SELECTED as u8);
    assert_eq!(config.level, LogLevel::LOG_TRACE as u8);

    let created = manager_ref
        .create_logger(context_layout(DATABASE_SCOPE), false)
        .unwrap();
    assert_eq!(created.kind, ahirudb::logging::LoggerKind::ThreadSafe);
    let rust_logger = ThreadSafeLogger::new(
        &created.filter.expect("a thread-safe logger needs a filter"),
        created.context,
        manager_ref,
    );

    assert!(rust_logger.should_log(b"FileSystem", LogLevel::LOG_TRACE));
    assert!(rust_logger.should_log(b"QueryLog", LogLevel::LOG_TRACE));
    assert!(!rust_logger.should_log(b"Metrics", LogLevel::LOG_TRACE));

    rust_logger.write_log_internal("FileSystem", LogLevel::LOG_TRACE, "opened");
    rust_logger.write_log_internal("QueryLog", LogLevel::LOG_INFO, "select 1");
    assert_eq!(storage.entries.lock().unwrap().len(), 2);

    assert_eq!(
        manager_ref
            .set_enable_structured_loggers(&["Nonexistent"])
            .unwrap_err(),
        "Unknown log type: 'Nonexistent'"
    );

    destroy_manager(manager);
}

#[test]
fn truncate_and_can_scan_reach_the_storage() {
    let (storage, manager) = make_manager();
    let manager_ref = unsafe { &*manager };
    manager_ref.set_enable_logging(true);

    let rust_logger = thread_safe_logger(manager_ref);
    rust_logger.write_log_internal("QueryLog", LogLevel::LOG_INFO, "before truncate");
    assert_eq!(storage.entries.lock().unwrap().len(), 1);

    assert!(manager_ref.can_scan(LoggingTargetTable::LOG_ENTRIES));
    assert!(!manager_ref.can_scan(LoggingTargetTable::ALL_LOGS));

    manager_ref.truncate_log_storage();
    assert_eq!(*storage.truncate_count.lock().unwrap(), 1);
    assert!(storage.entries.lock().unwrap().is_empty());

    destroy_manager(manager);
}

#[test]
fn log_context_ids_are_unique_per_manager() {
    let (_storage, manager) = make_manager();
    let manager_ref = unsafe { &*manager };

    let first = manager_ref
        .create_logger(context_layout(DATABASE_SCOPE), true)
        .unwrap();
    let second = manager_ref
        .create_logger(context_layout(THREAD_SCOPE), false)
        .unwrap();
    assert_ne!(first.context.context_id, second.context.context_id);

    destroy_manager(manager);
}

#[test]
fn switching_storage_reports_the_new_name_without_flushing() {
    let (storage, manager) = make_manager();
    let manager_ref = unsafe { &*manager };

    // The active storage is the one installed by `make_manager`, and C++ hands
    // its (lowercased) name in
    let active = "test_log_storage";
    assert_eq!(manager_ref.get_config().storage, active);
    assert_eq!(
        manager_ref
            .set_log_storage_name(active, "TEST_LOG_STORAGE", true)
            .unwrap(),
        None
    );
    assert!(
        manager_ref
            .set_log_storage_name(active, "file", true)
            .is_err()
    );
    assert!(
        manager_ref
            .set_log_storage_name(active, "unregistered", true)
            .is_err()
    );
    // Rejected for the LocalFileSystem, not for the name.
    // `set_log_storage_name` cannot reach this: 'file' needs a path there,
    // so it is rejected even earlier.
    let error = manager_ref
        .resolve_log_storage(active, "file", false)
        .unwrap_err();
    assert_eq!(error.kind, StorageSwitchErrorKind::FileSystemDisabled);
    assert_eq!(error.message, "Can not enable file logging with the LocalFileSystem disabled");
    assert!(manager_ref.register_log_storage("extra"));
    assert_eq!(
        manager_ref
            .set_log_storage_name(active, "EXTRA", true)
            .unwrap(),
        Some("extra".to_string())
    );
    // Neither a rejected nor an accepted switch reaches the storage: C++
    // flushes it in the same critical section that installs its
    // replacement, which is the only way that flush can be atomic with respect
    // to a concurrent write
    assert_eq!(*storage.flush_count.lock().unwrap(), 0);

    destroy_manager(manager);
}

/// The write path reads the sink without taking a lock of its own and
/// `set_config` takes `config`, so the two run against each other here: every
/// entry has to reach the storage and neither thread may deadlock.
#[test]
fn concurrent_writes_and_config_changes_do_not_deadlock() {
    let storage = Box::leak(Box::new(InMemoryStorage::default()));
    let manager = LogManager::new(c_log_storage(storage));

    let entries = 500usize;
    std::thread::scope(|scope| {
        let writer = scope.spawn(|| {
            for _ in 0..entries
            {
                manager.write_log_entry(
                    "QueryLog",
                    LogLevel::LOG_INFO,
                    "written",
                    &context_layout(DATABASE_SCOPE),
                );
            }
        });
        let switcher = scope.spawn(|| {
            for i in 0..entries
            {
                let level = if i % 2 == 0
                {
                    LogLevel::LOG_INFO
                }
                else
                {
                    LogLevel::LOG_DEBUG
                };
                manager.set_config(
                    true,
                    LogMode::LEVEL_ONLY as u8,
                    level as u8,
                    LogConfig::IN_MEMORY_STORAGE_NAME,
                    &[],
                    &[],
                );
            }
        });
        writer.join().unwrap();
        switcher.join().unwrap();
    });

    assert_eq!(storage.entries.lock().unwrap().len(), entries);
}

/// `set_config` replaces the whole config in one step, so the storage name in
/// the config and the other settings cannot be read half-updated. The sink
/// itself is not part of that: it is built once, and a config change cannot
/// redirect where a write lands (in C++ that is what installing a storage under
/// the write lock does).
#[test]
fn set_config_replaces_the_config_in_one_step() {
    let (storage, manager) = make_manager();
    let manager_ref = unsafe { &*manager };
    manager_ref.set_enable_logging(true);

    manager_ref.set_config(
        true,
        LogMode::ENABLE_SELECTED as u8,
        LogLevel::LOG_DEBUG as u8,
        "Memory",
        &["QueryLog"],
        &[],
    );

    let config = manager_ref.get_config();
    assert_eq!(config.storage, "memory");
    assert_eq!(config.level, LogLevel::LOG_DEBUG as u8);
    assert_eq!(config.enabled_log_types, ["QueryLog"]);

    // Log entries still land in the storage the manager was built with
    let rust_logger = thread_safe_logger(manager_ref);
    rust_logger.write_log_internal("QueryLog", LogLevel::LOG_INFO, "written");
    assert_eq!(storage.entries.lock().unwrap().len(), 1);
    // And the name the config records is what C++ recorded, not something a
    // write can change
    assert_eq!(manager_ref.get_config().storage, "memory");

    destroy_manager(manager);
}

/// A storage that reports failure cannot leave the manager in a state where it
/// cannot be used again.
#[test]
fn a_failing_storage_does_not_wedge_the_manager() {
    extern fn failing_write(
        _ctx: *mut c_void,
        _timestamp: i64,
        _level: u8,
        _log_type: *const i8,
        _log_type_len: usize,
        _log_message: *const i8,
        _log_message_len: usize,
        _context: *const RegisteredLoggingContextLayout,
    ) -> bool {
        false
    }
    extern fn failing_noop(_ctx: *mut c_void) -> bool { false }

    let storage = CLogStorage::new(
        std::ptr::null_mut(),
        failing_write,
        failing_noop,
        failing_noop,
        c_can_scan,
    );
    let manager = LogManager::new(storage);
    manager.record_log_storage_name("memory");
    manager.set_enable_logging(true);

    let rust_logger = thread_safe_logger(&manager);
    // The failures are reported out of band (the C++ side rethrows them), so
    // all this can assert is that the write path survives them
    rust_logger.write_log_internal("QueryLog", LogLevel::LOG_INFO, "dropped");
    rust_logger.flush();
    manager.flush();
    manager.truncate_log_storage();
    assert!(manager.can_scan(LoggingTargetTable::LOG_ENTRIES));

    // The manager is still usable afterwards
    rust_logger.write_log_internal("QueryLog", LogLevel::LOG_INFO, "dropped");
}
