#![allow(clippy::module_inception)]

//! Sort module:: pure decisions and arithmetic ported from `src/common/sort`.
//!
//! This module mirrors the C++ file layout of `src/common/sort`:
//!
//! - [`sort_key`]: the sort-key vocabulary (type codes, per-type constants,
//!   byte swap, and the three decisions derived from them).
//! - [`sort`]: the layout derivation (which output column is a key column and
//!   which is a payload column).
//! - [`sort_strategy`]: the strategy choice and the order-materialisation plan.
//! - [`sorted_run_merger`]: the K-way merge-path boundary computation.
//! - [`partition_key_tracker`]: the tracker's bin state machine and hash-based
//!   candidate selection.
//!
//! The memory movement (radix sort, reorder, merge copy, gather/scatter) stays
//! in C++ (see the design notes); this module only owns decisions and
//! arithmetic.
//!
//! The following C++ files in `src/common/sort/` remain as bridge shells because
//! Crubit cannot bridge the underlying constructs, and the reasons are:
//! - `sort.cpp`, `sorted_run.cpp`, `sorted_run_merger.cpp`: in-place radix sort
//!   over pinned `TupleDataCollection` blocks, `SortKey<T>` comparisons, and
//!   block iterators are not bridgeable (generics / raw pointer aliasing).
//! - `full_sort.cpp`, `hashed_sort.cpp`, `natural_sort.cpp`: virtual dispatch on
//!   `SortStrategy` subclasses and operator-framework plumbing.
//! - `partition_key_tracker.cpp`: representative storage in `DataChunk` and
//!   value comparison via `VectorOperations::DistinctFrom` (arbitrary types).
//! - `sort_strategy.cpp`: factory dispatch glue.

pub mod partition_key_tracker;
pub mod sort;
pub mod sort_key;
pub mod sort_strategy;
pub mod sorted_run_merger;
