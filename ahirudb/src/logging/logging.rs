//! Core logging types, mirroring `src/include/duckdb/logging/logging.hpp`.

use std::collections::HashSet;

// Note: `LOG_` prefix is to avoid problems with DEBUG and ERROR macros, these
// are ToString-ed without the prefix
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum LogLevel {
    LOG_TRACE = 10,
    LOG_DEBUG = 20,
    #[default]
    LOG_INFO = 30,
    LOG_WARNING = 40,
    LOG_ERROR = 50,
    LOG_FATAL = 60,
}

impl From<u8> for LogLevel {
    fn from(value: u8) -> Self {
        match value
        {
            10 => LogLevel::LOG_TRACE,
            20 => LogLevel::LOG_DEBUG,
            30 => LogLevel::LOG_INFO,
            40 => LogLevel::LOG_WARNING,
            50 => LogLevel::LOG_ERROR,
            60 => LogLevel::LOG_FATAL,
            _ =>
            {
                // Every value comes from a C++ `LogLevel`, which has no other
                // discriminants. A byte that is not one of them
                // would silently become `LOG_INFO`
                debug_assert!(false, "unknown log level discriminant {value}");
                LogLevel::LOG_INFO
            },
        }
    }
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
        match value
        {
            0 => LogMode::LEVEL_ONLY,
            1 => LogMode::DISABLE_SELECTED,
            2 => LogMode::ENABLE_SELECTED,
            _ =>
            {
                // A `LogMode` only filters by level, which lets every log type
                // through - the wrong way round for a
                // corrupt byte to end up as
                debug_assert!(false, "unknown log mode discriminant {value}");
                LogMode::LEVEL_ONLY
            },
        }
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

impl LogConfig {
    pub const IN_MEMORY_STORAGE_NAME: &'static str = "memory";
    pub const STDOUT_STORAGE_NAME: &'static str = "stdout";
    pub const FILE_STORAGE_NAME: &'static str = "file";

    pub const DEFAULT_LOG_LEVEL: LogLevel = LogLevel::LOG_INFO;
    pub const DEFAULT_LOG_STORAGE: &'static str = Self::IN_MEMORY_STORAGE_NAME;

    /// Builds a config out of its parts, the form a config takes when it comes
    /// from C++ (`mode` and `level` are `u8` there, and the storage name is
    /// lowercased).
    pub fn new(
        enabled: bool,
        mode: u8,
        level: u8,
        storage: &str,
        enabled_log_types: &[&str],
        disabled_log_types: &[&str],
    ) -> Self {
        Self {
            enabled,
            mode: LogMode::from(mode),
            level: LogLevel::from(level),
            storage: storage.to_lowercase(),
            enabled_log_types: enabled_log_types.iter().map(|v| v.to_string()).collect(),
            disabled_log_types: disabled_log_types.iter().map(|v| v.to_string()).collect(),
        }
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: LogMode::LEVEL_ONLY,
            level: LogConfig::DEFAULT_LOG_LEVEL,
            storage: LogConfig::DEFAULT_LOG_STORAGE.to_string(),
            enabled_log_types: HashSet::new(),
            disabled_log_types: HashSet::new(),
        }
    }
}

/// Plain-scalar mirror of [`LogConfig`], used to hand a config to C++ in one
/// atomic read. `LogConfig` itself cannot cross the FFI boundary because it
/// holds `HashSet`s.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct LogConfigSnapshot {
    pub enabled: bool,
    pub mode: u8,
    pub level: u8,
    pub storage: String,
    /// Sorted log type names, so that the snapshot is deterministic and a
    /// [`LogFilter`] can binary-search it
    pub enabled_log_types: Vec<String>,
    /// Sorted log type names, so that the snapshot is deterministic and a
    /// [`LogFilter`] can binary-search it
    pub disabled_log_types: Vec<String>,
}

impl From<&LogConfig> for LogConfigSnapshot {
    fn from(config: &LogConfig) -> Self {
        Self {
            enabled: config.enabled,
            mode: config.mode as u8,
            level: config.level as u8,
            storage: config.storage.clone(),
            enabled_log_types: sorted_names(&config.enabled_log_types),
            disabled_log_types: sorted_names(&config.disabled_log_types),
        }
    }
}

/// The sorted log type names of a set, so that a snapshot is deterministic and
/// a [`LogFilter`] can binary-search it
fn sorted_names(names: &HashSet<String>) -> Vec<String> {
    let mut sorted: Vec<String> = names.iter().cloned().collect();
    sorted.sort();
    sorted
}

/// The part of a config that a logger filters with, derived from a
/// [`LogConfig`].
///
/// This is what crosses the FFI boundary into a logger rather than a
/// [`LogConfigSnapshot`]: `CreateLogger` runs per query and per task, and a
/// snapshot would additionally clone the storage name, which no logger ever
/// reads. Building this from the config sorts the log type names once, and a
/// logger then clones the filter it is created from instead of deriving one of
/// its own - so that the kind of the logger and the filter it applies can never
/// disagree.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct LogFilter {
    pub enabled: bool,
    pub mode: LogMode,
    pub level: LogLevel,
    /// Sorted, so that lookups are a binary search
    pub enabled_log_types: Vec<String>,
    /// Sorted, so that lookups are a binary search
    pub disabled_log_types: Vec<String>,
}

/// Whether a log call of this type and level passes this filter.
///
/// `log_type` is bytes rather than a `&str`: this runs for every `DUCKDB_LOG_*`
/// macro invocation - including the `ShouldLog` calls in `FileSystem`, which is
/// the hottest logging call site - and a `&str` would make the C++ caller
/// validate that the name is UTF-8 before it may even be looked up. Log type
/// names are ASCII, so matching the bytes answers the same question, and a name
/// that is not valid UTF-8 matches nothing: it cannot equal an ASCII name byte
/// for byte, exactly like the `HashSet` lookup of the sanitized string it
/// replaced.
impl LogFilter {
    pub fn should_log(&self, log_type: &[u8], log_level: LogLevel) -> bool {
        if !self.enabled
        {
            return false;
        }
        if self.level > log_level
        {
            return false;
        }
        // Binary search over the sorted names, without allocating a `String`
        // for the lookup key
        let contains = |names: &[String]| {
            names
                .binary_search_by(|name| name.as_bytes().cmp(log_type))
                .is_ok()
        };
        if self.mode == LogMode::ENABLE_SELECTED
        {
            return contains(&self.enabled_log_types);
        }
        if self.mode == LogMode::DISABLE_SELECTED
        {
            return !contains(&self.disabled_log_types);
        }
        true
    }
}

impl From<&LogConfig> for LogFilter {
    fn from(config: &LogConfig) -> Self {
        Self {
            enabled: config.enabled,
            mode: config.mode,
            level: config.level,
            enabled_log_types: sorted_names(&config.enabled_log_types),
            disabled_log_types: sorted_names(&config.disabled_log_types),
        }
    }
}

impl From<&LogConfigSnapshot> for LogFilter {
    fn from(snapshot: &LogConfigSnapshot) -> Self {
        // The membership tests below are binary searches, so the sortedness
        // these two `Vec`s rely on has to hold. Only
        // `LogConfigSnapshot::from` produces them, and the fields are `pub`, so
        // this is the one place that can check it
        debug_assert!(snapshot.enabled_log_types.is_sorted());
        debug_assert!(snapshot.disabled_log_types.is_sorted());
        Self {
            enabled: snapshot.enabled,
            mode: LogMode::from(snapshot.mode),
            level: LogLevel::from(snapshot.level),
            enabled_log_types: snapshot.enabled_log_types.clone(),
            disabled_log_types: snapshot.disabled_log_types.clone(),
        }
    }
}

/// Mirrors a `LogConfig` that lives on the C++ side as a [`LogConfigSnapshot`],
/// which is the only form a config can take when it crosses the FFI boundary or
/// enters a logger.
pub fn log_config_snapshot(
    enabled: bool,
    mode: u8,
    level: u8,
    storage: &str,
    enabled_log_types: &[&str],
    disabled_log_types: &[&str],
) -> LogConfigSnapshot {
    LogConfigSnapshot::from(&LogConfig::new(
        enabled,
        mode,
        level,
        storage,
        enabled_log_types,
        disabled_log_types,
    ))
}

/// Get current timestamp in microseconds since epoch, matching DuckDB's
/// `timestamp_t`.
pub fn current_timestamp() -> i64 {
    use std::time::{
        SystemTime,
        UNIX_EPOCH,
    };
    match SystemTime::now().duration_since(UNIX_EPOCH)
    {
        Ok(since_epoch) => i64::try_from(since_epoch.as_micros()).unwrap_or(i64::MAX),
        // A clock that sits before 1970 is negative, not zero
        Err(before_epoch) => i64::try_from(before_epoch.duration().as_micros())
            .unwrap_or(i64::MAX)
            .saturating_neg(),
    }
}

/// Returns `value` with every invalid UTF-8 sequence replaced by the
/// replacement character.
///
/// Log messages are not guaranteed to be valid UTF-8 (they can carry arbitrary
/// bytes out of files, out of error strings or in from the C API), while `&str`
/// is, so the bytes have to be sanitized before they cross the FFI boundary.
pub fn utf8_lossy(value: &[u8]) -> String { String::from_utf8_lossy(value).into_owned() }

/// Layout-compatible mirror of the C++ `duckdb::RegisteredLoggingContext`, used
/// to hand a registered context back to the C++ log storage. The `has_*` flags
/// stand in for `optional_idx`, which is what C++ builds this from.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RegisteredLoggingContextLayout {
    pub context_id: u64,
    pub scope: u8,
    pub has_thread_id: bool,
    pub has_connection_id: bool,
    pub has_transaction_id: bool,
    pub has_query_id: bool,
    pub thread_id: u64,
    pub connection_id: u64,
    pub transaction_id: u64,
    pub query_id: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_timestamp_is_microseconds_since_the_epoch() {
        assert!(current_timestamp() > 1_600_000_000_000_000);
    }

    /// A `LogFilter` is what a logger filters with, so it has to answer
    /// membership the same way a `HashSet` does. The names are searched for
    /// rather than hashed, which only works while they are sorted - which the
    /// debug build checks on the way in.
    #[test]
    fn log_filter_answers_membership_like_the_config_it_came_from() {
        let filter = LogFilter::from(&log_config_snapshot(
            true,
            LogMode::ENABLE_SELECTED as u8,
            LogLevel::LOG_INFO as u8,
            "memory",
            &["FileSystem", "QueryLog"],
            &[],
        ));
        assert!(
            filter
                .enabled_log_types
                .binary_search(&"FileSystem".to_string())
                .is_ok()
        );
        assert!(
            filter
                .enabled_log_types
                .binary_search(&"QueryLog".to_string())
                .is_ok()
        );
        assert!(
            filter
                .enabled_log_types
                .binary_search(&"Metrics".to_string())
                .is_err()
        );
        assert!(filter.disabled_log_types.is_empty());
    }

    /// A disabled config logs nothing at all, whatever the level, the mode or
    /// the log type selection say. A logger is only ever built from an
    /// enabled config (`LogManager::create_logger` hands out `LoggerKind::Nop`
    /// otherwise), so from C++ this is a tripwire rather than a filter a
    /// caller can reach - but it is the property that makes the filter safe
    /// to hand to a logger without also passing "is logging on" separately, and
    /// `HEAD`'s `ThreadSafeLogger::ShouldLog` did not have it.
    #[test]
    fn a_disabled_filter_rejects_every_log_call() {
        for mode in [LogMode::LEVEL_ONLY, LogMode::ENABLE_SELECTED, LogMode::DISABLE_SELECTED]
        {
            let filter = LogFilter::from(&log_config_snapshot(
                false,
                mode as u8,
                LogLevel::LOG_TRACE as u8,
                "memory",
                &["FileSystem"],
                &[],
            ));
            for level in [LogLevel::LOG_TRACE, LogLevel::LOG_INFO, LogLevel::LOG_FATAL]
            {
                assert!(!filter.should_log(b"FileSystem", level));
                assert!(!filter.should_log(b"QueryLog", level));
            }
        }
    }
}
