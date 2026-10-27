//! The multi-file reader's index and file-list arithmetic.
//!
//! `src/common/multi_file` is virtual classes over `LogicalType`, `Value`,
//! `Expression`, `ClientContext`, `FileSystem` and the execution pipeline.
//! Under the limits crubit documents - no generics, no `dyn`, no exceptions -
//! none of that behaviour crosses. What does cross is the arithmetic underneath
//! it: which index is valid, what a filter entry means, how many files a list
//! holds, and when its expansion is over.
//!
//! The module is split the way the C++ is:
//!
//! - [`data`]: `multi_file_data.hpp` — the index rules and the maps over them
//! - [`list`]: `multi_file_list.hpp` — the rules a file list answers with
//! - [`reader`]: `MultiFileReader` decision kernels (option parsing, bind planning,
//!   pre-open constants like filename/hive extraction, skip/prune/partition decisions)
//! - [`column_mapper`]: `MultiFileColumnMapper` mapping-plan kernels over flattened
//!   column arenas
//! - [`union_by_name`]: union-by-name merge decisions
//! - [`ops`]: the `MultiFileOps` vtable of trampolines into C++ for facts only C++ has
//!
//! # The structs stay in C++
//!
//! Only the sources in `src/common/multi_file` may include `crubit/ahirudb.h`,
//! and `multi_file_data.hpp` and `multi_file_list.hpp` are included by every reader extension.
//! The structs stay where they are holding their members; this module holds the rules over them.
//! `multi_file_rust_bridge.hpp` is the one place that knows both sides.
//!
//! # Ops table contract
//!
//! Kernels call back to C++ via [`ops::MultiFileOps`] for operations that need unported
//! types (`LogicalType`, `Value`, filter evaluation, etc.). The table is set once by C++
//! via `duckdb_multi_file_set_ops` and then treated as immutable. All trampolines must
//! be wrapped by the call-scope machinery so C++ exceptions unwind safely back to the
//! C++ entry point.
//!
//! # Panics
//!
//! Nothing here is reachable from C++ with a panic: errors are returned as `Result` and
//! the bridge turns them into `InternalException`. Indices are checked before use.

#![deny(unsafe_code)]

pub mod column_mapper;
pub mod data;
pub mod list;
pub mod ops;
pub mod reader;
pub mod union_by_name;
