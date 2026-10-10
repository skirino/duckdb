//! Scaffolding shared by the unit tests of the logging modules.
//!
//! None of them reach a storage - they all test decisions that happen before a
//! write - so a sink that fails loudly if it is ever called is enough. The
//! C-ABI signatures live here, next to `CLogStorage::new`, which is the one
//! place they have to stay in step with.

use std::ffi::{
    c_char,
    c_void,
};

use crate::logging::{
    log_manager::LogManager,
    log_storage::CLogStorage,
    logging::RegisteredLoggingContextLayout,
};

extern "C" fn no_write(
    _ctx: *mut c_void,
    _timestamp: i64,
    _level: u8,
    _log_type: *const c_char,
    _log_type_len: usize,
    _log_message: *const c_char,
    _log_message_len: usize,
    _context: *const RegisteredLoggingContextLayout,
) -> bool {
    panic!("these tests never reach the storage")
}

extern "C" fn no_op(_ctx: *mut c_void) -> bool { panic!("these tests never reach the storage") }

extern "C" fn no_scan(_ctx: *mut c_void, _table: u8) -> bool { false }

/// A [`LogManager`] whose storage is the sink above. The manager has to outlive
/// every logger built from it, which holds a raw pointer to it just like the
/// loggers C++ creates.
pub(crate) fn test_manager() -> LogManager {
    LogManager::new(CLogStorage::new(std::ptr::null_mut(), no_write, no_op, no_op, no_scan))
}

/// The context C++ would build for a database-scope logging context without any
/// ids set
pub(crate) fn context() -> RegisteredLoggingContextLayout {
    const DATABASE_SCOPE: u8 = 10;
    RegisteredLoggingContextLayout {
        context_id: 0,
        scope: DATABASE_SCOPE,
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
