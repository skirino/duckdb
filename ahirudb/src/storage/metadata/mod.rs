//! Metadata block layer: pointer arithmetic, free-list bitmask bookkeeping,
//! block-boundary streaming for the writer and reader, and the
//! modified-block tracking behind `MetadataManager`.
//!
//! What moved here is the pure arithmetic - the bit manipulation, the
//! pointer packing, the offset/capacity transitions - the three C++ classes
//! used to do by hand. What stays in C++ is the object graph the arithmetic
//! walks over: `BlockManager`/`BufferManager` references, the `mutex`, the
//! `shared_ptr<BlockHandle>` graph, and the `unordered_map`s that carry the
//! per-block and per-manager state. The C++ shells hold those and call the
//! functions below through the bridge in [`bridge`].

pub mod bridge;
pub mod free_list;
pub mod manager;
pub mod pointer;
pub mod reader;
pub mod writer;

#[cfg(test)]
pub mod smoke;
