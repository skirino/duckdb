//! Vector module: a Rust port of the pointer arithmetic in DuckDB's
//! `src/common/vector` module.
//!
//! What moved here are the selection-vector and validity-mask loops that sit
//! under `FlatVector`, `ListVector` and `ArrayVector` — the `memcpy` and bit
//! manipulation that used to be hand-written over raw `data_ptr_t`s. They are
//! the part of the module that is pure arithmetic over buffers, and the part
//! where an out-of-range index was undefined behaviour rather than an error.
//!
//! What did *not* move is the `VectorBuffer` class hierarchy. Those classes are
//! virtual-dispatch nodes in a `Vector`'s buffer chain, and they call back into
//! `Vector`, `VectorCache`, `Value`, `Allocator` and `SelectionVector`, none of
//! which are in Rust; Crubit bridges neither `dyn` nor exceptions. `StringHeap`
//! stayed for a related reason: it is a thin wrapper over `ArenaAllocator`, and
//! it lends out `char *`s into memory it owns, which a Rust owner cannot do
//! without an `unsafe` escape hatch — see
//! `tmp/002_migrate_vector_module/impl.md`.
//!
//! The module mirrors the C++ file layout: [`flat_vector`], [`list_vector`] and
//! [`array_vector`] correspond to the `.cpp` files of the same name, and
//! [`selection`] holds the two views those files share.
//!
//! Nothing is re-exported from here: the C++ side reaches each kernel through
//! the module path its `.cpp` counterpart carries, so a name appears once.

pub mod array_vector;
pub mod flat_vector;
pub mod list_vector;
pub(crate) mod selection;
