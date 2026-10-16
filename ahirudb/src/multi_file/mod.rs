//! The multi-file reader's index and file-list arithmetic.
//!
//! `src/common/multi_file` is virtual classes over `LogicalType`, `Value`,
//! `Expression`, `ClientContext`, `FileSystem` and the execution pipeline. Under
//! the limits crubit documents - no generics, no `dyn`, no exceptions - none of
//! that behaviour crosses. What does cross is the arithmetic underneath it: which
//! index is valid, what a filter entry means, how many files a list holds, and
//! when its expansion is over.
//!
//! The module is split the way the C++ is:
//!
//! - [`data`]: `multi_file_data.hpp` — the index rules and the maps over them
//! - [`list`]: `multi_file_list.hpp` — the rules a file list answers with
//!
//! # The structs stay in C++
//!
//! Only the sources in `src/common/multi_file` may include `crubit/ahirudb.h`, and
//! `multi_file_data.hpp` and `multi_file_list.hpp` are public headers that every
//! reader extension includes. So the structs stay where they are, holding their
//! `idx_t` members, and this module holds the rules over those members.
//! `multi_file_rust_bridge.hpp` is the one place that knows both sides.
//!
//! # What stays in C++, and why
//!
//! Everything else under `src/common/multi_file` stays where it is, because
//! crossing it needs a type that is not ported yet:
//!
//! - `MultiFileColumnDefinition` holds a `LogicalType`, a `ParsedExpression` and
//!   nested definitions of its own
//! - `MultiFileConstantEntry` holds a `Value`
//! - `HivePartitioningIndex` is serialized through `common/serializer`
//! - the file list itself stores `OpenFileInfo`, resolves globs through
//!   `FileSystem` and is driven through virtual dispatch
//! - `MultiFileReader`, `MultiFileColumnMapper` and `MultiFileFunction` are
//!   virtual classes over `ClientContext`, `TableFilterSet`, `Expression` and
//!   the execution pipeline
//!
//! The seam is the one the design asks for: the arithmetic moves, the objects
//! that own it stay.
//!
//! # Panics
//!
//! Nothing here is reachable from C++ with a panic: an index that is not valid, or
//! one that is out of range, is an `Err` rather than a panic, and the bridge turns
//! it into the `InternalException` the original code threw. The one rule this
//! module adds is where the C++ had none: an index into `MultiFileLocalColumnIds`,
//! `MultiFileFilterMap` or `MultiFileConstantMap` is checked before it is used.

#![deny(unsafe_code)]

pub mod data;
pub mod list;