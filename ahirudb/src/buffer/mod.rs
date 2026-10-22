#![allow(nonstandard_style)]

//! Buffer module: a Rust port of the accounting in `src/storage/buffer`.
//!
//! What moved here is the pure arithmetic and bookkeeping the C++ side
//! used to do by hand:
//!
//! - [`reservation`]: charge arithmetic for `BufferPoolReservation`.
//! - [`memory_usage`]: the cached-counter flush behind `BufferPool::MemoryUsage`.
//! - [`block_copy`]: raw buffer copies with bounds checks.
//! - [`cast`]: checked signed/unsigned conversions.
//! - [`dead_nodes`]: dead-node counter bookkeeping for the eviction queue.
//!
//! What did not move, and why: `BlockMemory`, `BlockHandle`, `BufferPool`,
//! `BufferHandle` and the eviction queue stay in C++ - they are the
//! `shared_ptr`/`weak_ptr`/`mutex`/concurrent-queue graph the whole engine
//! pins. Rust owns numbers, slices and counters only.

pub mod block_copy;
pub mod bridge;
pub mod cast;
pub mod dead_nodes;
pub mod memory_usage;
pub mod reservation;

pub type idx_t = u64;
