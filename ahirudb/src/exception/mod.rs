//! The exception module: the part of `src/common/exception` that is now Rust.
//!
//! The module is split the same way the C++ is, and keeps the same split of
//! labour at the boundary:
//!
//! - [`types`]: `ExceptionType`, `EXCEPTION_MAP`, and the types that marshal
//!   across FFI
//! - [`classification`]: which types invalidate what, and which are execution
//!   errors
//! - [`extra_info`]: the `position`/`location` keys a query location
//!   contributes
//! - [`factories`]: the message and `extra_info` of the exception factories
//! - [`bridge`]: the only `pub` functions, i.e. what C++ may call
//!
//! What stays in C++ is everything that needs `fmt`, a Levenshtein search, or
//! `Function`'s naming rules: the exception classes themselves (they have to be
//! throwable and catchable by base type), the `%s` substitution, the candidate
//! ranking, and the JSON payload of `what()`.

// The exception type names are kept identical to their C++ counterparts.
#![allow(nonstandard_style)]
#![allow(clippy::upper_case_acronyms)]

pub mod bridge;
pub mod classification;
pub mod extra_info;
pub mod factories;
pub mod types;
