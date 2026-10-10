//! Log manager, mirroring `src/include/duckdb/logging/log_manager.hpp` and
//! `src/logging/log_manager.cpp`.

use std::{
    collections::HashSet,
    sync::{
        Mutex,
        RwLock,
        atomic::{
            AtomicU64,
            Ordering,
        },
    },
};

use crate::logging::{
    log_storage::{
        CLogStorage,
        LoggingTargetTable,
    },
    log_type::LogTypeRegistry,
    logger::LoggerKind,
    logging::{
        LogConfig,
        LogConfigSnapshot,
        LogFilter,
        LogLevel,
        LogMode,
        RegisteredLoggingContextLayout,
        current_timestamp,
    },
};

/// The result of `LogManager::create_logger`.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct CreatedLogger {
    pub kind: LoggerKind,
    pub context: RegisteredLoggingContextLayout,
    /// What the logger filters with, so that the kind that was chosen here and
    /// the config the logger applies can never disagree. Absent for
    /// [`LoggerKind::Nop`]: that logger never filters anything, and it is the
    /// kind logging is disabled by default, so the filter would be built,
    /// carried across the boundary and dropped for nothing.
    pub filter: Option<LogFilter>,
}

/// Holds global logging state:
/// - Handles configuration changes
/// - Creates Loggers with cached configuration
/// - Main sink for logs
/// - Holds the log storage
///
/// Must be allocated on the heap (through `new`) and never moved: the loggers
/// it hands out keep a raw pointer to it, exactly like the C++ `LogManager&`
/// they hold.
///
/// # Locking
/// The write, flush, truncate and `can_scan` paths take no lock of their own.
/// They are serialized by the C++ `LogManager::lock` that every [`CLogStorage`]
/// trampoline acquires on the way into the storage - the same lock (and
/// the same guarantee) as the C++ `LogManager` this replaced, which held it for
/// the whole write.
///
/// `storage` is therefore not something a switch has to write: it is a bare
/// function table whose `ctx` is the C++ `LogManager`, so every call through it
/// lands in whichever storage that manager currently has installed. Swapping it
/// could never change where a write goes, which is why it is built once and
/// never touched again.
///
/// # The one thing a lock-free path still has to serialize
/// `create_logger` hands out logging context ids, and the id is the identity of
/// a context - the storage looks a context up by it, so two contexts sharing
/// one would be merged, and the storage would be missing a
/// `duckdb_log_contexts` row. The counter serializes that itself, with a single
/// atomic read-modify-write (`register_logging_context_internal`), which is why
/// this path needs no lock of its own - and also why the read and the increment
/// may not be split into two operations. Apart from reading `config` (a mutex
/// the rest of the module takes anyway, held here for the length of a
/// `CreateLogger` rather than for a write), nothing else in this function has
/// to be serialized. It reads `config`, which is why nothing here may call into
/// C++ while holding `config` either.
///
/// `registered_log_types` is never held while `config` is
/// (`set_enable_structured_loggers` drops the registry guard before it takes
/// the config).
pub struct LogManager {
    /// The sink every write, flush, truncate and `can_scan` goes through.
    /// Installed once, at construction
    storage: CLogStorage,
    config: Mutex<LogConfig>,

    next_registered_logging_context_index: AtomicU64,

    // Any additional LogStorages registered (by extensions for example). C++ owns the storage
    // objects, Rust only needs to know which names may be selected - see
    // `is_known_log_storage`.
    registered_log_storages: RwLock<HashSet<String>>,
    registered_log_types: RwLock<LogTypeRegistry>,
}

/// Heap-allocates a `LogManager` that writes to `storage`.
///
/// The loggers keep a raw pointer to the manager, so its address has to stay
/// stable: this is what guarantees that. # Safety
/// The returned pointer must eventually be passed to [`delete_log_manager`],
/// exactly once.
pub fn new_log_manager(storage: CLogStorage) -> *mut LogManager {
    Box::into_raw(Box::new(LogManager::new(storage)))
}

/// Deletes a `LogManager` created by [`new_log_manager`].
///
/// # Safety
/// `ptr` must come from [`new_log_manager`] and must not be used afterwards.
pub unsafe fn delete_log_manager(ptr: *mut LogManager) { unsafe { drop(Box::from_raw(ptr)) } }

impl LogManager {
    pub fn new(storage: CLogStorage) -> Self {
        Self {
            storage,
            config: Mutex::new(LogConfig::default()),
            next_registered_logging_context_index: AtomicU64::new(0),
            registered_log_storages: RwLock::new(HashSet::new()),
            registered_log_types: RwLock::new(LogTypeRegistry::new()),
        }
    }

    /// Records the name of the log storage that C++ has installed, which is
    /// what `get_config` reports.
    ///
    /// It runs after C++ has installed the storage, so the decision to switch
    /// is taken against the name C++ owns (`resolve_log_storage`), never
    /// against this copy.
    pub fn record_log_storage_name(&self, name: &str) {
        self.config.lock().unwrap().storage = name.to_lowercase();
    }

    /// Mirrors `LogManager::SetConfigInternal`: replaces the whole config in
    /// one critical section, so that a concurrent `get_config` can never
    /// observe a half-applied change.
    pub fn set_config(
        &self,
        enabled: bool,
        mode: u8,
        level: u8,
        storage: &str,
        enabled_log_types: &[&str],
        disabled_log_types: &[&str],
    ) {
        *self.config.lock().unwrap() =
            LogConfig::new(enabled, mode, level, storage, enabled_log_types, disabled_log_types);
    }

    /// Returns a snapshot of the config, for the C++ `LogManager::GetConfig`.
    pub fn get_config(&self) -> LogConfigSnapshot {
        LogConfigSnapshot::from(&*self.config.lock().unwrap())
    }

    /// Registers a logging context and assigns it the next context id.
    fn register_logging_context_internal(
        &self,
        context: RegisteredLoggingContextLayout,
    ) -> Result<RegisteredLoggingContextLayout, String> {
        // The read and the increment have to be one operation: `CreateLogger`
        // runs concurrently on every connection and every worker
        // thread, and a separate `load` followed by a `fetch_add` would let two
        // of them read the same value and hand out the same id. The id
        // is the identity of the context - the storage looks the context up by
        // it, so two contexts sharing one lose a `duckdb_log_contexts`
        // row and their entries get merged.
        //
        // The increment saturates rather than wrapping: a `fetch_add` past
        // `u64::MAX` would wrap the counter to 0, and the
        // next `CreateLogger` would hand out id 0 again - the same id the very
        // first context got, which is the exact collision this function
        // exists to prevent. So the last id handed out is `u64::MAX - 1` and
        // every attempt after that fails and leaves the counter where
        // it is.
        let context_id = self
            .next_registered_logging_context_index
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| current.checked_add(1))
            .map_err(|_| "Ran out of available log context ids.".to_string())?;
        Ok(RegisteredLoggingContextLayout { context_id, ..context })
    }

    /// Mirrors `LogManager::CreateLogger`: registers the logging context and
    /// picks the logger implementation for it. C++ turns the returned kind,
    /// context and filter into the matching concrete logger.
    pub fn create_logger(
        &self,
        context: RegisteredLoggingContextLayout,
        mutable_settings: bool,
    ) -> Result<CreatedLogger, String> {
        // One read of the config, from which both the kind and the filter the
        // logger is built with are taken
        let config = self.config.lock().unwrap();
        let kind = if mutable_settings
        {
            LoggerKind::Mutable
        }
        else if !config.enabled
        {
            LoggerKind::Nop
        }
        else
        {
            LoggerKind::ThreadSafe
        };
        let filter = match kind
        {
            LoggerKind::Nop => None,
            _ => Some(LogFilter::from(&*config)),
        };
        drop(config);

        let registered_context = self.register_logging_context_internal(context)?;

        Ok(CreatedLogger { kind, context: registered_context, filter })
    }

    /// This is to be called by the Loggers only, it does not verify log_level
    /// and log_type
    ///
    /// A failure surfaces as an exception in the C++ caller that entered Rust.
    pub fn write_log_entry(
        &self,
        log_type: &str,
        log_level: LogLevel,
        log_message: &str,
        context: &RegisteredLoggingContextLayout,
    ) {
        // No lock: the storage cannot be swapped out mid-write, because the
        // write reaches it through the C++ `LogManager::lock` that the
        // bridge takes
        self.storage.write_log_entry(
            current_timestamp(),
            log_level,
            log_type,
            log_message,
            context,
        );
    }

    /// Flush everything
    pub fn flush(&self) { self.storage.flush_all(); }

    pub fn can_scan(&self, table: LoggingTargetTable) -> bool { self.storage.can_scan(table) }

    /// Every config mutator returns the config it produced, so that the C++
    /// caller can hand exactly that config to the global logger instead of
    /// re-reading it (which a concurrent mutator could have changed in
    /// between).
    pub fn set_enable_logging(&self, enable: bool) -> LogConfigSnapshot {
        let mut config = self.config.lock().unwrap();
        config.enabled = enable;
        LogConfigSnapshot::from(&*config)
    }

    pub fn set_log_mode(&self, mode: u8) -> LogConfigSnapshot {
        let mut config = self.config.lock().unwrap();
        config.mode = LogMode::from(mode);
        LogConfigSnapshot::from(&*config)
    }

    pub fn set_log_level(&self, level: u8) -> LogConfigSnapshot {
        let mut config = self.config.lock().unwrap();
        config.level = LogLevel::from(level);
        LogConfigSnapshot::from(&*config)
    }

    pub fn set_enabled_log_types(&self, enabled_log_types: &[&str]) -> LogConfigSnapshot {
        let mut config = self.config.lock().unwrap();
        config.enabled_log_types = enabled_log_types.iter().map(|v| v.to_string()).collect();
        LogConfigSnapshot::from(&*config)
    }

    pub fn set_disabled_log_types(&self, disabled_log_types: &[&str]) -> LogConfigSnapshot {
        let mut config = self.config.lock().unwrap();
        config.disabled_log_types = disabled_log_types.iter().map(|v| v.to_string()).collect();
        LogConfigSnapshot::from(&*config)
    }

    /// Mirrors `LogManager::SetLogStorage`: rejects the storages that cannot be
    /// selected without extra configuration.
    ///
    /// `file_logging_available` is what only C++ can answer: it asks the
    /// `DatabaseInstance` whether the LocalFileSystem is enabled.
    pub fn set_log_storage_name(
        &self,
        active_storage: &str,
        storage_name: &str,
        file_logging_available: bool,
    ) -> Result<Option<String>, StorageSwitchError> {
        let storage_name_to_lower = storage_name.to_lowercase();
        // 'SET logging_storage' cannot supply the path that file storage
        // requires, so reject the switch here (active
        // storage preserved) and point users at enable_logging instead of
        // installing a path-less storage that throws on every later
        // flush.
        if storage_name_to_lower == LogConfig::FILE_STORAGE_NAME
            && active_storage != storage_name_to_lower
        {
            return Err(StorageSwitchError {
                kind: StorageSwitchErrorKind::InvalidConfiguration,
                message: "Cannot select 'file' log storage via 'SET logging_storage' because it \
                          requires a path. Use CALL enable_logging(storage='file', \
                          storage_path='...') instead."
                    .to_string(),
            });
        }
        self.resolve_lowered_log_storage(
            active_storage,
            &storage_name_to_lower,
            file_logging_available,
        )
    }

    /// Mirrors the validation half of `LogManager::SetLogStorage`: reports the
    /// (lowercased) name of the storage that C++ has to create, or `None`
    /// when the requested storage is already active.
    ///
    /// `active_storage` is the name of the storage that is active right now.
    /// C++ owns it (it sits next to the storage itself) and reads it under
    /// the lock that guards the switch, because only the C++ side can be
    /// authoritative about which storage is installed: the name in this
    /// config is recorded after the storage has been swapped in.
    pub fn resolve_log_storage(
        &self,
        active_storage: &str,
        storage_name: &str,
        file_logging_available: bool,
    ) -> Result<Option<String>, StorageSwitchError> {
        self.resolve_lowered_log_storage(
            active_storage,
            &storage_name.to_lowercase(),
            file_logging_available,
        )
    }

    /// This only validates. In particular it does not flush: the storage that
    /// is being replaced has to be flushed and swapped out in one critical
    /// section on the C++ side (see `LogManager::FlushAndSetActiveLogStorage`),
    /// because a flush here and a swap there would leave a window in which
    /// a concurrent write buffers an entry into a storage that has
    /// already been flushed and then throws away.
    ///
    /// Both names have to be lowercased already, so that the two entry points
    /// above lower once between them.
    fn resolve_lowered_log_storage(
        &self,
        active_storage: &str,
        storage_name: &str,
        file_logging_available: bool,
    ) -> Result<Option<String>, StorageSwitchError> {
        if active_storage == storage_name
        {
            return Ok(None);
        }

        if storage_name == LogConfig::FILE_STORAGE_NAME && !file_logging_available
        {
            return Err(StorageSwitchError {
                kind: StorageSwitchErrorKind::FileSystemDisabled,
                message: "Can not enable file logging with the LocalFileSystem disabled"
                    .to_string(),
            });
        }

        if !self.is_known_log_storage(storage_name)
        {
            return Err(StorageSwitchError {
                kind: StorageSwitchErrorKind::NotRegistered,
                message: format!("Log storage '{storage_name}' is not yet registered"),
            });
        }

        Ok(Some(storage_name.to_string()))
    }

    /// Whether a registered storage may be selected. C++ keeps the same names
    /// in `LogManager::registered_log_storages`, because it owns the
    /// storages themselves; this set is a cache of that registry, not a second
    /// source of truth: it only answers the question here, so that a switch
    /// is rejected before anything is touched. The error is the one
    /// `LogManager::CreateLogStorage` throws for the same condition, except
    /// that this one reports the lowercased name and `CreateLogStorage`
    /// reports the name as the caller spelled it.
    fn is_known_log_storage(&self, storage_name: &str) -> bool {
        if storage_name == LogConfig::IN_MEMORY_STORAGE_NAME
            || storage_name == LogConfig::STDOUT_STORAGE_NAME
            || storage_name == LogConfig::FILE_STORAGE_NAME
        {
            return true;
        }
        self.registered_log_storages
            .read()
            .unwrap()
            .contains(storage_name)
    }

    pub fn register_log_storage(&self, name: &str) -> bool {
        self.registered_log_storages
            .write()
            .unwrap()
            .insert(name.to_lowercase())
    }

    /// Mirrors `LogManager::SetEnableStructuredLoggers`
    pub fn set_enable_structured_loggers(
        &self,
        enabled_logger_types: &[&str],
    ) -> Result<LogConfigSnapshot, String> {
        let mut enabled_log_types = HashSet::new();
        let mut min_log_level = LogLevel::LOG_FATAL;

        {
            let registry = self.registered_log_types.read().unwrap();
            for enabled_logger_type in enabled_logger_types
            {
                let lookup = match registry.lookup(enabled_logger_type)
                {
                    Some(lookup) => lookup,
                    None => return Err(format!("Unknown log type: '{enabled_logger_type}'")),
                };

                enabled_log_types.insert(lookup.name.clone());

                min_log_level = min_log_level.min(lookup.level);
            }
        }

        // The config is locked for the whole update, so that a concurrent
        // config change cannot be lost
        let mut current = self.config.lock().unwrap();
        current.enabled_log_types = enabled_log_types;
        current.level = min_log_level;
        current.mode = LogMode::ENABLE_SELECTED;
        current.enabled = true;
        Ok(LogConfigSnapshot::from(&*current))
    }

    pub fn truncate_log_storage(&self) { self.storage.truncate(); }

    /// Returns the canonically-cased name of a registered log type.
    pub fn lookup_log_type(&self, type_name: &str) -> Option<String> {
        self.registered_log_types
            .read()
            .unwrap()
            .lookup(type_name)
            .map(|entry| entry.name.clone())
    }

    pub fn register_log_type(&self, name: &str, level: u8) -> Result<(), String> {
        self.registered_log_types
            .write()
            .unwrap()
            .register_entry(name, LogLevel::from(level))
    }
}

/// Why a log storage could not be selected
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageSwitchErrorKind {
    /// The storage name does not belong to any registered storage
    NotRegistered = 0,
    /// The storage cannot be selected without extra configuration
    InvalidConfiguration = 1,
    /// File logging was requested but the LocalFileSystem is disabled
    FileSystemDisabled = 2,
}

/// The error of resolving a `SET logging_storage` /
/// `enable_logging(storage=...)` request
#[repr(C)]
#[derive(Debug, Clone)]
pub struct StorageSwitchError {
    pub kind: StorageSwitchErrorKind,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logging::test_support::{
        context,
        test_manager,
    };

    fn assert_storage_is_created(
        storage_name: &str,
        result: &Result<Option<String>, StorageSwitchError>,
    ) {
        match result
        {
            Ok(Some(name)) => assert_eq!(name, storage_name),
            Ok(None) => panic!("expected a storage switch to {storage_name}"),
            Err(error) => panic!("expected a storage switch to {storage_name}, got {error:?}"),
        }
    }

    fn register_log_types(manager: &LogManager) {
        for (name, level) in [
            ("QueryLog", LogLevel::LOG_INFO),
            ("FileSystem", LogLevel::LOG_TRACE),
            ("Metrics", LogLevel::LOG_INFO),
        ]
        {
            manager.register_log_type(name, level as u8).unwrap();
        }
    }

    #[test]
    fn create_logger_picks_a_logger_kind() {
        let manager = test_manager();

        // Logging is disabled by default, so a NopLogger is handed out
        let created = manager.create_logger(context(), false).unwrap();
        assert_eq!(created.kind, LoggerKind::Nop);
        assert_eq!(created.context.context_id, 0);

        let created = manager.create_logger(context(), false).unwrap();
        assert_eq!(created.context.context_id, 1);

        // With mutable settings the global mutable logger is used regardless of
        // the enabled setting
        let created = manager.create_logger(context(), true).unwrap();
        assert_eq!(created.kind, LoggerKind::Mutable);
        assert_eq!(created.context.context_id, 2);

        manager.set_enable_logging(true);
        let created = manager.create_logger(context(), false).unwrap();
        assert_eq!(created.kind, LoggerKind::ThreadSafe);
        assert_eq!(created.context.context_id, 3);
        assert!(created.filter.unwrap().enabled);
    }

    /// A `NopLogger` filters nothing and is the kind handed out for every query
    /// and task while logging is disabled, so its filter is not built at
    /// all
    #[test]
    fn a_nop_logger_is_created_without_a_filter() {
        let manager = test_manager();
        for _ in 0..3
        {
            let created = manager.create_logger(context(), false).unwrap();
            assert_eq!(created.kind, LoggerKind::Nop);
            assert!(created.filter.is_none());
        }

        manager.set_enable_logging(true);
        let created = manager.create_logger(context(), false).unwrap();
        assert!(created.filter.is_some());
    }

    #[test]
    fn logging_context_ids_are_monotonic() {
        let manager = test_manager();
        for expected in 0..5u64
        {
            let created = manager.create_logger(context(), false).unwrap();
            assert_eq!(created.context.context_id, expected);
        }
    }

    /// The context id counter is the one thing `create_logger` cannot let two
    /// threads race on, so it hands the ids out with a single atomic
    /// read-modify-write. A separate load and increment would collide (and
    /// leave the storage with fewer ids than contexts: it looks a context
    /// up by its id). This is the only test that can fail for that - every
    /// other id test is single-threaded.
    #[test]
    fn logging_context_ids_are_unique_across_threads() {
        const THREADS: u64 = 8;
        const IDS_PER_THREAD: u64 = 2000;

        let manager = test_manager();
        let ids = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..THREADS)
                .map(|_| {
                    scope.spawn(|| {
                        let mut ids = Vec::with_capacity(IDS_PER_THREAD as usize);
                        for _ in 0..IDS_PER_THREAD
                        {
                            ids.push(
                                manager
                                    .create_logger(context(), false)
                                    .unwrap()
                                    .context
                                    .context_id,
                            );
                        }
                        ids
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });

        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len());
        assert_eq!(ids.len(), (THREADS * IDS_PER_THREAD) as usize);
    }

    /// The id counter saturates instead of wrapping: an id is the identity of a
    /// context, so handing out the same one twice merges two contexts and
    /// loses a `duckdb_log_contexts` row. Wrapping would do exactly that by
    /// starting over at 0, so running out has to fail every later attempt
    /// too rather than the first one.
    #[test]
    fn the_log_context_id_counter_saturates_instead_of_wrapping() {
        let manager = test_manager();
        manager
            .next_registered_logging_context_index
            .store(u64::MAX, Ordering::Relaxed);

        let exhausted = |manager: &LogManager| manager.create_logger(context(), false).unwrap_err();
        assert_eq!(exhausted(&manager), "Ran out of available log context ids.");
        // The counter did not move, so the id of the very first context is
        // never handed out a second time
        assert_eq!(
            manager
                .next_registered_logging_context_index
                .load(Ordering::Relaxed),
            u64::MAX
        );
        assert_eq!(exhausted(&manager), "Ran out of available log context ids.");
    }

    #[test]
    fn set_enable_structured_loggers_uses_lowest_level() {
        let manager = test_manager();
        register_log_types(&manager);
        let snapshot = manager
            .set_enable_structured_loggers(&["QueryLog", "filesystem"])
            .unwrap();
        // The returned snapshot is the config the update produced
        assert!(snapshot.enabled);
        assert_eq!(snapshot.mode, LogMode::ENABLE_SELECTED as u8);
        assert_eq!(snapshot.level, LogLevel::LOG_TRACE as u8);
        assert_eq!(snapshot.enabled_log_types, ["FileSystem", "QueryLog"]);

        assert_eq!(manager.get_config().enabled_log_types, snapshot.enabled_log_types);
        assert_eq!(
            manager
                .set_enable_structured_loggers(&["Nonsense"])
                .unwrap_err(),
            "Unknown log type: 'Nonsense'"
        );
    }

    #[test]
    fn config_mutators_return_the_config_they_produced() {
        let manager = test_manager();

        let snapshot = manager.set_enable_logging(true);
        assert!(snapshot.enabled);

        let snapshot = manager.set_log_level(LogLevel::LOG_DEBUG as u8);
        assert!(snapshot.enabled);
        assert_eq!(snapshot.level, LogLevel::LOG_DEBUG as u8);

        let snapshot = manager.set_log_mode(LogMode::DISABLE_SELECTED as u8);
        assert_eq!(snapshot.level, LogLevel::LOG_DEBUG as u8);
        assert_eq!(snapshot.mode, LogMode::DISABLE_SELECTED as u8);

        let snapshot = manager.set_disabled_log_types(&["FileSystem"]);
        assert_eq!(snapshot.disabled_log_types, ["FileSystem"]);

        let snapshot = manager.set_enabled_log_types(&[]);
        assert!(snapshot.enabled_log_types.is_empty());
        assert_eq!(snapshot.disabled_log_types, ["FileSystem"]);
    }

    #[test]
    fn storage_switching() {
        let manager = test_manager();
        assert_eq!(manager.get_config().storage, LogConfig::IN_MEMORY_STORAGE_NAME);

        // Selecting the active storage is a no-op. C++ owns the name of the
        // storage that is installed, and passes it in
        let active = LogConfig::IN_MEMORY_STORAGE_NAME;
        assert_eq!(
            manager
                .set_log_storage_name(active, "memory", true)
                .unwrap(),
            None
        );
        assert_eq!(
            manager
                .set_log_storage_name(active, "MEMORY", true)
                .unwrap(),
            None
        );

        assert_storage_is_created("stdout", &manager.set_log_storage_name(active, "stdout", true));
        let error = manager
            .set_log_storage_name(active, "nope", true)
            .unwrap_err();
        assert_eq!(error.kind, StorageSwitchErrorKind::NotRegistered);
        assert_eq!(error.message, "Log storage 'nope' is not yet registered");
    }

    #[test]
    fn file_storage_cannot_be_selected_without_a_path() {
        let manager = test_manager();
        let error = manager
            .set_log_storage_name(LogConfig::IN_MEMORY_STORAGE_NAME, "file", true)
            .unwrap_err();
        assert_eq!(error.kind, StorageSwitchErrorKind::InvalidConfiguration);
        assert!(
            error
                .message
                .starts_with("Cannot select 'file' log storage via 'SET logging_storage'")
        );
    }

    /// Resolving a storage switch only validates: it must not touch the active
    /// storage, whether it rejects the switch or accepts it. The flush and
    /// the swap are one atomic step on the C++ side, which is the only place
    /// that can do it without leaving a window for a concurrent write.
    #[test]
    fn resolving_a_storage_switch_never_touches_the_active_storage() {
        let manager = test_manager();
        let active = LogConfig::IN_MEMORY_STORAGE_NAME;

        let error = manager
            .resolve_log_storage(active, "file", false)
            .unwrap_err();
        assert_eq!(error.kind, StorageSwitchErrorKind::FileSystemDisabled);
        assert_eq!(error.message, "Can not enable file logging with the LocalFileSystem disabled");

        assert_storage_is_created("stdout", &manager.resolve_log_storage(active, "stdout", true));

        // The accepted switch does not change anything by itself either:
        // installing the storage (and with it its name) is C++'s job,
        // so the config still names the old storage
        assert_eq!(manager.get_config().storage, LogConfig::IN_MEMORY_STORAGE_NAME);
        assert_eq!(
            manager
                .resolve_log_storage(active, LogConfig::IN_MEMORY_STORAGE_NAME, false)
                .unwrap(),
            None
        );
    }

    /// The name that decides whether a switch is a no-op is the one C++ passes
    /// in, not the one the config recorded: a switch is resolved against
    /// the storage that is actually installed, so it cannot be silently dropped
    /// because the config has not caught up with the installed storage yet.
    #[test]
    fn a_switch_is_resolved_against_the_storage_name_cxx_owns() {
        let manager = test_manager();

        assert_storage_is_created("stdout", &manager.resolve_log_storage("memory", "stdout", true));
        assert_eq!(manager.get_config().storage, LogConfig::IN_MEMORY_STORAGE_NAME);

        // C++ has installed `stdout`, so switching back to `memory` is a real
        // switch - even though the config still names the storage that
        // was active before
        assert_storage_is_created("memory", &manager.resolve_log_storage("stdout", "memory", true));
        assert_eq!(
            manager
                .resolve_log_storage("stdout", "stdout", true)
                .unwrap(),
            None
        );
    }

    #[test]
    fn registered_log_storages_can_be_selected() {
        let manager = test_manager();
        let active = LogConfig::IN_MEMORY_STORAGE_NAME;
        assert!(manager.register_log_storage("my_storage"));
        assert!(!manager.register_log_storage("MY_STORAGE"));
        assert_storage_is_created(
            "my_storage",
            &manager.set_log_storage_name(active, "my_storage", true),
        );
        assert_eq!(
            manager
                .set_log_storage_name(active, "other", true)
                .unwrap_err()
                .message,
            "Log storage 'other' is not yet registered"
        );
    }

    #[test]
    fn log_types_are_registered_case_insensitively() {
        let manager = test_manager();
        register_log_types(&manager);
        assert_eq!(manager.lookup_log_type("filesystem").as_deref(), Some("FileSystem"));
        assert!(manager.lookup_log_type("nope").is_none());

        assert_eq!(
            manager.register_log_type("QueryLog", 30).unwrap_err(),
            "Registered log writer 'QueryLog' already exists"
        );
        assert!(manager.register_log_type("CustomLog", 30).is_ok());
        assert_eq!(manager.lookup_log_type("customlog").as_deref(), Some("CustomLog"));
    }

    #[test]
    fn config_updates_are_applied() {
        let manager = test_manager();
        manager.set_log_level(20);
        manager.set_log_mode(1);
        manager.set_enabled_log_types(&[]);
        manager.set_disabled_log_types(&["FileSystem"]);

        let config = manager.get_config();
        assert_eq!(config.level, LogLevel::LOG_DEBUG as u8);
        assert_eq!(config.mode, LogMode::DISABLE_SELECTED as u8);
        assert!(config.enabled_log_types.is_empty());
        assert_eq!(config.disabled_log_types, ["FileSystem"]);
        assert!(manager.get_config().storage == LogConfig::IN_MEMORY_STORAGE_NAME);
    }
}
