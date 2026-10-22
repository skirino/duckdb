//! Logger implementations, mirroring `src/include/duckdb/logging/logger.hpp`
//! and `src/logging/logger.cpp`.

use std::{
    ptr::NonNull,
    sync::{
        RwLock,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
};

use crate::logging::{
    log_manager::LogManager,
    logging::{LogConfigSnapshot, LogFilter, LogLevel, RegisteredLoggingContextLayout},
};

/// Shared handle to the [`LogManager`] a logger writes to.
///
/// This mirrors the C++ `LogManager &manager` member: the LogManager is heap
/// allocated by C++ and outlives every logger it creates, so holding a raw
/// pointer is sound here.
#[derive(Copy, Clone)]
pub(crate) struct LogManagerRef(NonNull<LogManager>);

// SAFETY: access to the LogManager is serialized by the LogManager's own
// `lock`.
unsafe impl Send for LogManagerRef {}
unsafe impl Sync for LogManagerRef {}

impl LogManagerRef {
    pub(crate) fn new(manager: &LogManager) -> Self {
        Self(NonNull::from(manager))
    }

    /// # Safety
    /// The LogManager must outlive the logger holding this handle, which is
    /// guaranteed for loggers created by the LogManager itself.
    unsafe fn get(&self) -> &LogManager {
        unsafe { &*self.0.as_ptr() }
    }
}

/// The kind of logger that a new logging context gets, mirroring the branches
/// of `LogManager::CreateLogger`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoggerKind {
    Nop = 0,
    ThreadSafe = 1,
    Mutable = 2,
}

/// Thread-safe logger.
pub struct ThreadSafeLogger {
    config: LogFilter,
    context: RegisteredLoggingContextLayout,
    manager: LogManagerRef,
}

impl ThreadSafeLogger {
    /// `config` is the filter the decision to create this logger was based on,
    /// so that the kind of the logger and the filter it applies can never
    /// disagree. It is cloned here rather than derived from a config again:
    /// this runs per query and per task, so every allocation it does not
    /// make is one per logger.
    ///
    /// It is borrowed, because crubit generates no move constructor for a
    /// struct it models with a destructor, so ownership cannot be handed
    /// over from C++ at all: a by-value parameter would copy the filter (and
    /// every name in it) once more on every logger, before this clone.
    pub fn new(
        config: &LogFilter,
        context: RegisteredLoggingContextLayout,
        manager: &LogManager,
    ) -> Self {
        // A `NopLogger` should be used instead. This is a tripwire for the
        // tests of this crate only: the crubit staticlib is built from
        // the release profile, so the assert is compiled out of a DuckDB build.
        debug_assert!(
            config.enabled,
            "ThreadSafeLogger requires an enabled config"
        );
        Self {
            config: config.clone(),
            context,
            manager: LogManagerRef::new(manager),
        }
    }

    pub fn should_log(&self, log_type: &[u8], log_level: LogLevel) -> bool {
        self.config.should_log(log_type, log_level)
    }

    pub fn write_log_internal(&self, log_type: &str, log_level: LogLevel, log_message: &str) {
        // An error is rethrown by the C++ caller that entered Rust; see
        // `LogStorageBridge`
        unsafe { self.manager.get() }.write_log_entry(
            log_type,
            log_level,
            log_message,
            &self.context,
        );
    }

    pub fn flush(&self) {
        unsafe { self.manager.get() }.flush();
    }
}

/// Thread-safe logger with mutable log settings.
pub struct MutableLogger {
    // Lock-free pre-filter, so that the answer for "logging is off" or "this level is below the
    // configured one" costs two atomic loads and no lock at all - which is every log call
    // while logging is disabled, the default.
    //
    // These may only ever suppress, never admit. `should_log` reads them before it takes the
    // filter's read lock, so a config published in that window leaves it with the pre-filter
    // of the older config next to the filter of the newer one, and the stale `enabled`/`level`
    // then drop a call the newer config would have logged. That direction is the one that is
    // free: a reconfiguration racing a log call was always allowed to lose that entry.
    //
    // What cannot happen is the other one. `set_config` stores both atomics before the filter and
    // holds the write lock across all three, so the filter a reader acquires the read lock for
    // is never older than the atomics it just read, and the filter still has the last word on
    // every call that gets that far. So a permissive value left over in an atomic cannot let a
    // call through that the newest filter rejects. The mode is not mirrored here either, for the
    // same reason stated the other way round: its permissive direction (`LEVEL_ONLY`) logs
    // everything, and the answer for a log type selection has to come from the filter.
    enabled: AtomicBool,
    level: AtomicU8,

    config: RwLock<LogFilter>,
    context: RegisteredLoggingContextLayout,
    manager: LogManagerRef,
}

impl MutableLogger {
    /// `config` is the filter the decision to create this logger was based on,
    /// so that the kind of the logger and the filter it applies can never
    /// disagree. It is cloned here rather than derived from a config again:
    /// this runs per query and per task, so every allocation it does not
    /// make is one per logger.
    ///
    /// It is borrowed, for the reason `ThreadSafeLogger::new` gives.
    pub fn new(
        config: &LogFilter,
        context: RegisteredLoggingContextLayout,
        manager: &LogManager,
    ) -> Self {
        Self {
            enabled: AtomicBool::new(config.enabled),
            level: AtomicU8::new(config.level as u8),
            config: RwLock::new(config.clone()),
            context,
            manager: LogManagerRef::new(manager),
        }
    }

    pub fn should_log(&self, log_type: &[u8], log_level: LogLevel) -> bool {
        // The lock-free pre-filter. `enabled` is false by default, so this is
        // the whole answer for every log call while logging is disabled
        if !self.enabled.load(Ordering::Acquire)
            || LogLevel::from(self.level.load(Ordering::Acquire)) > log_level
        {
            return false;
        }
        // The mode is only answered from the filter, under its read lock - see
        // the comment on the atomics above
        let config = self.config.read().unwrap();
        config.should_log(log_type, log_level)
    }

    pub fn write_log_internal(&self, log_type: &str, log_level: LogLevel, log_message: &str) {
        // An error is rethrown by the C++ caller that entered Rust; see
        // `LogStorageBridge`
        unsafe { self.manager.get() }.write_log_entry(
            log_type,
            log_level,
            log_message,
            &self.context,
        );
    }

    pub fn flush(&self) {
        unsafe { self.manager.get() }.flush();
    }

    /// Replaces the cached config, for the C++ `MutableLogger::UpdateConfig`.
    ///
    /// `&self`, not `&mut self`: C++ reaches this from every `SET` that changes
    /// the log settings, none of which hold a lock, so two connections can
    /// be inside it at the same time. Everything the body touches is
    /// individually synchronized, so a shared borrow is all it needs - and
    /// requiring the exclusive one would be an aliasing promise
    /// that nothing keeps, i.e. the next plain field added here would be a data
    /// race.
    ///
    /// The atomics and the filter are published as one critical section, and
    /// the atomics go first. That ordering is what makes a lock-free
    /// pre-filter of the two sound: the filter a reader then acquires is never
    /// older than the pre-filter it read, and the filter has the last word
    /// on every call that gets that far, so a permissive value left over in
    /// an atomic cannot admit a call the newest filter rejects.
    ///
    /// What the write lock does *not* buy is atomicity of the pair as observed
    /// by `should_log`, which reads the two halves at different instants
    /// and under different synchronization: a config published in between
    /// leaves it with the pre-filter of one config next to the filter of
    /// another. The stale half is then the pre-filter, so the answer
    /// can under-log - a call the newer config would have logged is dropped -
    /// and it can never over-log. Losing that entry to a reconfiguration
    /// that raced the log call is not a regression, and holding the read lock
    /// across the atomic loads to exclude it is precisely the lock the
    /// default `LEVEL_ONLY` path is built to avoid. See the comment on the
    /// atomics and
    /// `the_lock_free_pre_filter_can_drop_a_call_the_newest_config_would_log`.
    ///
    /// Publishing the halves outside one critical section would not be a benign
    /// race either - `MutableLogger` is the global logger, and an answer
    /// that disagrees with the config it was handed keeps disagreeing until the
    /// next change.
    pub fn set_config(&self, config: &LogConfigSnapshot) {
        let new_config = LogFilter::from(config);
        let mut filter = self.config.write().unwrap();
        self.enabled.store(new_config.enabled, Ordering::Release);
        self.level.store(new_config.level as u8, Ordering::Release);
        *filter = new_config;
    }
}

// For when logging is disabled, C++ hands out its own `NopLogger`: it never
// reaches Rust, so there is nothing to port.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logging::{
        logging::{LogMode, log_config_snapshot},
        test_support::{context, test_manager},
    };

    fn filter(
        mode: LogMode,
        level: LogLevel,
        enabled_log_types: &[&str],
        disabled_log_types: &[&str],
    ) -> LogFilter {
        LogFilter::from(&log_config_snapshot(
            true,
            mode as u8,
            level as u8,
            "memory",
            enabled_log_types,
            disabled_log_types,
        ))
    }

    /// The atomics are a pre-filter that can only suppress, never admit, which
    /// is what makes it sound for `should_log` to read them without a lock:
    /// whichever half of a publish it catches, the filter - published after
    /// them, so never the older of the two - decides everything that gets
    /// that far. This is the half of the invariant that protects
    /// against over-logging, and it is why the mode is not mirrored in an
    /// atomic at all: `LEVEL_ONLY` is the most permissive mode there is, so
    /// a stale copy of it would land on the admitting side of the invariant
    /// rather than the suppressing one.
    ///
    /// The atomics are forced to the most permissive values a config can
    /// produce here, which is the state a reader in `set_config`'s window
    /// would have seen: a pre-filter that admits everything next to a filter
    /// that excludes the log type.
    #[test]
    fn the_lock_free_pre_filter_cannot_over_log() {
        let manager = test_manager();
        let logger = MutableLogger::new(
            &filter(
                LogMode::ENABLE_SELECTED,
                LogLevel::LOG_TRACE,
                &["QueryLog"],
                &[],
            ),
            context(),
            &manager,
        );
        logger.enabled.store(true, Ordering::Release);
        logger
            .level
            .store(LogLevel::LOG_TRACE as u8, Ordering::Release);

        assert!(logger.should_log(b"QueryLog", LogLevel::LOG_INFO));
        assert!(!logger.should_log(b"FileSystem", LogLevel::LOG_INFO));
        assert!(!logger.should_log(b"FileSystem", LogLevel::LOG_TRACE));
    }

    /// The other half of the same window, and the one that is accepted rather
    /// than excluded: `should_log` reads the atomics before it takes the
    /// read lock, so a config published in between leaves it with a pre-filter
    /// from the old config next to the filter of the new one, and the
    /// restrictive half of that pair drops a call the new config would have
    /// logged. Losing it is not a regression - `MutableLogger` is the global
    /// logger, and a reconfiguration racing a log call was always free to
    /// drop the entry - and the alternative, holding the read lock across the
    /// atomic loads, is the lock the default `LEVEL_ONLY` path exists to avoid.
    /// The state is staged directly rather than raced for, so the test pins
    /// the behaviour instead of the schedule that produces it.
    #[test]
    fn the_lock_free_pre_filter_can_drop_a_call_the_newest_config_would_log() {
        let manager = test_manager();
        let logger = MutableLogger::new(
            &filter(LogMode::LEVEL_ONLY, LogLevel::LOG_TRACE, &[], &[]),
            context(),
            &manager,
        );
        // What a reader in `set_config`'s window holds: a pre-filter left over
        // from a stricter config, next to a filter that is the
        // permissive one
        logger.enabled.store(false, Ordering::Release);
        logger
            .level
            .store(LogLevel::LOG_FATAL as u8, Ordering::Release);
        assert!(
            logger
                .config
                .read()
                .unwrap()
                .should_log(b"QueryLog", LogLevel::LOG_TRACE)
        );

        // Both the level and the `enabled` half can be the stale one, and both
        // skip the call the filter admits
        assert!(!logger.should_log(b"QueryLog", LogLevel::LOG_INFO));
        logger.enabled.store(true, Ordering::Release);
        assert!(!logger.should_log(b"QueryLog", LogLevel::LOG_INFO));

        // A `set_config` that does land before the read republishes the
        // pre-filter with the filter, and the answer is the filter's
        logger.set_config(&log_config_snapshot(
            true,
            LogMode::LEVEL_ONLY as u8,
            LogLevel::LOG_TRACE as u8,
            "memory",
            &[],
            &[],
        ));
        assert!(logger.should_log(b"QueryLog", LogLevel::LOG_INFO));
    }

    /// The other direction: a pre-filter that has already been told logging is
    /// off, or that the level is too low, skips the call without taking the
    /// filter's lock at all. Both are the answers that the filter itself would
    /// give.
    #[test]
    fn the_lock_free_pre_filter_answers_the_disabled_and_too_low_cases() {
        let manager = test_manager();
        let logger = MutableLogger::new(
            &filter(LogMode::LEVEL_ONLY, LogLevel::LOG_WARNING, &[], &[]),
            context(),
            &manager,
        );
        assert!(!logger.should_log(b"QueryLog", LogLevel::LOG_INFO));
        assert!(logger.should_log(b"QueryLog", LogLevel::LOG_WARNING));

        logger.set_config(&log_config_snapshot(
            false,
            LogMode::LEVEL_ONLY as u8,
            LogLevel::LOG_TRACE as u8,
            "memory",
            &[],
            &[],
        ));
        // Disabled at `LOG_TRACE`, so nothing passes at any level
        assert!(!logger.should_log(b"QueryLog", LogLevel::LOG_FATAL));

        // And a config change moves the atomics along with the filter
        logger.set_config(&log_config_snapshot(
            true,
            LogMode::LEVEL_ONLY as u8,
            LogLevel::LOG_INFO as u8,
            "memory",
            &[],
            &[],
        ));
        assert!(!logger.should_log(b"QueryLog", LogLevel::LOG_DEBUG));
        assert!(logger.should_log(b"QueryLog", LogLevel::LOG_INFO));
    }

    /// `MutableLogger` is the global logger, so `set_config` runs on every
    /// connection that changes the logging settings while every other
    /// thread is asking it whether to log. The lock-free pre-filter and the
    /// filter it guards are published as one critical section, so a reader
    /// that observes the filter - the one every answer is decided from -
    /// observes a matching pre-filter as well. Publishing the two halves
    /// separately leaves a reader holding the filter of one config next to
    /// the pre-filter of another, and since the mode is deliberately not
    /// mirrored at all, such a reader answers for a config that was never
    /// published.
    ///
    /// This reader takes the read lock *before* it reads the atomics, which is
    /// what lets it claim that: it holds the lock that keeps a writer from
    /// publishing either half, so it can only catch a publish whole.
    /// `should_log` reads the atomics first and then takes the read lock,
    /// so it can be that far into a publish; what that costs it is
    /// stated on the atomics, and pinned by
    /// `the_lock_free_pre_filter_can_drop_a_call_the_newest_config_would_log`.
    ///
    /// Two writer threads with configs that differ in `enabled`, in the level
    /// and in the mode are what make the halves disagree; the reader can
    /// only ever catch it, never report it falsely, because it observes both
    /// halves while holding the read lock that keeps a writer from
    /// publishing either of them.
    #[test]
    fn concurrent_config_changes_publish_the_pre_filter_and_the_filter_together() {
        let manager = test_manager();
        let logger = MutableLogger::new(
            &filter(LogMode::LEVEL_ONLY, LogLevel::LOG_INFO, &[], &[]),
            context(),
            &manager,
        );

        /// A config that differs from every other one in all three settings the
        /// pre-filter mirrors
        fn config(enabled: bool, mode: LogMode, level: LogLevel) -> LogConfigSnapshot {
            log_config_snapshot(enabled, mode as u8, level as u8, "memory", &[], &[])
        }

        const WRITES: usize = 20_000;
        let mismatched = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..2 {
                scope.spawn(|| {
                    for _ in 0..WRITES {
                        logger.set_config(&config(
                            true,
                            LogMode::ENABLE_SELECTED,
                            LogLevel::LOG_TRACE,
                        ));
                        logger.set_config(&config(
                            false,
                            LogMode::DISABLE_SELECTED,
                            LogLevel::LOG_FATAL,
                        ));
                    }
                });
            }
            scope.spawn(|| {
                for _ in 0..WRITES {
                    let filter = logger.config.read().unwrap();
                    if logger.enabled.load(Ordering::Acquire) != filter.enabled
                        || LogLevel::from(logger.level.load(Ordering::Acquire)) != filter.level
                    {
                        mismatched.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });
        });

        assert_eq!(mismatched.load(Ordering::Relaxed), 0);

        // Whichever config the storm ended on, the logger ends up with exactly
        // that one
        logger.set_config(&log_config_snapshot(
            true,
            LogMode::ENABLE_SELECTED as u8,
            LogLevel::LOG_INFO as u8,
            "memory",
            &["QueryLog"],
            &[],
        ));
        assert!(logger.should_log(b"QueryLog", LogLevel::LOG_INFO));
        assert!(!logger.should_log(b"FileSystem", LogLevel::LOG_INFO));
        assert!(!logger.should_log(b"QueryLog", LogLevel::LOG_DEBUG));
    }
}
