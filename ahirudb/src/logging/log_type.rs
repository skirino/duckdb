//! The log type registry, mirroring the registry part of `LogManager` in
//! `src/logging/log_manager.cpp`.
//!
//! The log types themselves live in C++
//! (`src/include/duckdb/logging/log_type.hpp`): their structured schema is a
//! DuckDB `LogicalType`, which is not ported to Rust yet, and extensions
//! subclass them. C++ therefore drives this registry
//! through `LogManager::register_log_type`, passing the name and the level of
//! each type it registers - mirroring the C++ `registered_log_types` map
//! exactly. Duplicating the list of built-in types here would only risk the two
//! definitions drifting apart, and so would mirroring `LogType::is_structured`:
//! the schema that flag describes stays with the `LogType` that owns it.

use std::collections::HashMap;

use crate::logging::logging::LogLevel;

/// The registered metadata of a single log type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogTypeEntry {
    pub(crate) name: String,
    pub(crate) level: LogLevel,
}

/// Case-insensitive registry of log types, mirroring
/// `LogManager::registered_log_types`.
#[derive(Debug, Default)]
pub(crate) struct LogTypeRegistry {
    /// Lowercased name -> the canonically-cased name
    lookup: HashMap<String, LogTypeEntry>,
}

impl LogTypeRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Registers a log type, returning an error if the name is already taken
    /// (as in C++).
    pub(crate) fn register_entry(&mut self, name: &str, level: LogLevel) -> Result<(), String> {
        let key = name.to_lowercase();
        if self.lookup.contains_key(&key) {
            return Err(format!("Registered log writer '{name}' already exists"));
        }
        self.lookup.insert(
            key,
            LogTypeEntry {
                name: name.to_string(),
                level,
            },
        );
        Ok(())
    }

    pub(crate) fn lookup(&self, type_name: &str) -> Option<&LogTypeEntry> {
        self.lookup.get(&type_name.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_registered_case_insensitively() {
        let mut registry = LogTypeRegistry::new();
        registry
            .register_entry("FileSystem", LogLevel::LOG_TRACE)
            .unwrap();
        assert_eq!(registry.lookup("filesystem").unwrap().name, "FileSystem");
        assert_eq!(
            registry.lookup("FILESYSTEM").unwrap().level,
            LogLevel::LOG_TRACE
        );
        assert!(registry.lookup("nope").is_none());
    }

    #[test]
    fn duplicate_registration_fails() {
        let mut registry = LogTypeRegistry::new();
        registry
            .register_entry("QueryLog", LogLevel::LOG_INFO)
            .unwrap();
        assert_eq!(
            registry
                .register_entry("QueryLog", LogLevel::LOG_INFO)
                .unwrap_err(),
            "Registered log writer 'QueryLog' already exists"
        );
        // Registration is case-insensitive, so a differently cased duplicate is
        // rejected as well
        assert!(
            registry
                .register_entry("querylog", LogLevel::LOG_INFO)
                .is_err()
        );
        assert!(
            registry
                .register_entry("Metrics", LogLevel::LOG_INFO)
                .is_ok()
        );
        // The differently-cased duplicate left the first registration in place
        assert_eq!(registry.lookup("QueryLog").unwrap().name, "QueryLog");
    }
}
