//! Allocator module: a Rust port of the allocation bookkeeping in
//! DuckDB's `src/common/allocator`.
//!
//! What moved here is the stateful half of the module:
//!
//! - [`debug`]: the debug tracker that `duckdb::AllocatorDebugInfo` shells out
//!   to - an outstanding-bytes counter and, in `DUCKDB_DEBUG_ALLOCATION`
//!   builds, a pointer-to-(size, stack-trace) map. This is the module's manual
//!   memory bookkeeping, which is what the migration targets: invariants the
//!   C++ asserted only under `D_ASSERT` are total functions here, reporting a
//!   mismatch with a `false` answer instead of silently corrupting the counter.
//! - [`trim`]: the rate-limit gate behind `duckdb::Allocator::MallocTrim` - a
//!   100ms interval and the compare-exchange race that decides which caller
//!   trims. The `malloc_trim` call itself stays in C++ (a glibc call), so the
//!   crate takes no libc dependency.
//!
//! What did *not* move, and why:
//!
//! - `Allocator` is held as `Allocator &`/`shared_ptr<Allocator>` everywhere
//!   and constructed from C++ function pointers, and its `MAXIMUM_ALLOC_SIZE`
//!   bounds check runs on every allocation in every build: an FFI call there
//!   would tax all builds, so the check and its constant stay inline in the
//!   class.
//! - `AllocatedData` is an RAII value type held by value across the whole
//!   engine; its destructor frees through the allocator that lent the memory,
//!   and Rust cannot lend out `char *`s into memory it owns.
//! - The malloc and jemalloc backends *are* the C++ function pointers the
//!   `Allocator` is constructed from, including from extensions and from
//!   `DBConfig.allocator`.
//!
//! The module mirrors the C++ file layout, and [`bridge`] is the
//! C-ABI surface: the only functions C++ calls.

pub mod bridge;
pub mod debug;
pub mod trim;
