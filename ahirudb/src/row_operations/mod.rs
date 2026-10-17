//! Row operations: the aggregate-state lifecycle and the row matcher.
//!
//! This module is the Rust port of `src/common/row_operations`. It mirrors the
//! C++ file split:
//!
//! - [`row_aggregate`] is `row_aggregate.cpp`: the seven aggregate-state
//!   lifecycle operations (`InitializeStates`, `DestroyStates`, `UpdateStates`,
//!   `UpdateFilteredStates`, `UpdateStatesClustered`, `CombineStates`,
//!   `FinalizeStates`).
//! - [`row_matcher`] is `row_matcher.cpp`: `RowMatcher::Initialize`,
//!   `RowMatcher::Match` and the match-function dispatch.
//!
//! # The boundary
//!
//! Everything these loops operate on - `TupleDataLayout`, `Vector`,
//! `DataChunk`, `AggregateObject`, the aggregate callbacks, `VectorOperations`
//! - lives in C++ that is not ported yet. The loops therefore reach back into
//! C++ through two hand-written `extern "C"` function tables,
//! [`row_aggregate::AggregateOps`] and [`row_matcher::MatcherOps`], filled once
//! by the bridge in `src/common/row_operations/row_operations_rust_bridge.cpp`.
//! That is the same shape `src/logging` uses for `CLogStorage`.
//!
//! The tables keep the Rust side free of raw pointer arithmetic: an opaque C++
//! object crosses as a `*mut c_void` handle, and the bridge dereferences it.
//! The only raw pointers Rust ever forms are the opaque handles themselves,
//! which it passes straight back.

pub mod row_aggregate;
pub mod row_matcher;
