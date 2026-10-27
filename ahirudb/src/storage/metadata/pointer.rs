//! Pointer encoding/decoding for the metadata block layer.
//!
//! The `MetaBlockPointer` bitfield packs a block index in the low 56 bits and
//! a metadata sub-block index in the high 8 bits. `BlockPointer` is the block id
//! plus a byte offset within the block. Converting between the two uses the
//! metadata block size, which is the same arithmetic the C++ originals did.

#![allow(nonstandard_style)]

pub type idx_t = u64;

pub const METADATA_BLOCK_COUNT: idx_t = 64;
const INVALID_BLOCK: i64 = -1;

const INDEX_BITS: u32 = 8;
const INDEX_MASK: idx_t = (1u64 << INDEX_BITS) - 1;

/// `MetaBlockPointer` bitfield: block index in the low 56 bits, metadata
/// sub-block index in the high 8 bits.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct MetaBlockPointer {
    pub block_pointer: idx_t,
    pub offset: u32,
    pub unused_padding: u32,
}

impl MetaBlockPointer {
    pub fn new(block_pointer: idx_t, offset: u32) -> Self {
        Self {
            block_pointer,
            offset,
            unused_padding: 0,
        }
    }

    pub fn get_block_id(&self) -> i64 {
        let mask = !(INDEX_MASK << 56);
        (self.block_pointer & mask) as i64
    }

    pub fn get_block_index(&self) -> u32 {
        (self.block_pointer >> 56) as u32
    }

    pub fn is_valid(&self) -> bool {
        self.block_pointer != idx_t::MAX
    }
}

impl Default for MetaBlockPointer {
    fn default() -> Self {
        Self::new(idx_t::MAX, 0)
    }
}

/// `MetadataPointer`: block index + sub-block index.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct MetadataPointer {
    pub block_index: idx_t,
    pub index: u8,
}

/// `BlockPointer`: block id + byte offset within the block.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct BlockPointer {
    pub block_id: i64,
    pub offset: u32,
    pub unused_padding: u32,
}

impl BlockPointer {
    pub fn new(block_id: i64, offset: u32) -> Self {
        Self {
            block_id,
            offset,
            unused_padding: 0,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.block_id != INVALID_BLOCK
    }
}

impl Default for BlockPointer {
    fn default() -> Self {
        Self::new(INVALID_BLOCK, 0)
    }
}

/// `MetadataPointer` -> `MetaBlockPointer` with offset.
pub fn get_disk_pointer(block_index: idx_t, index: u32, offset: u32) -> MetaBlockPointer {
    let block_pointer = block_index | (index as idx_t) << 56;
    MetaBlockPointer::new(block_pointer, offset)
}

/// `MetaBlockPointer` -> `BlockPointer` using the metadata block size.
pub fn to_block_pointer(
    meta_pointer: MetaBlockPointer,
    metadata_block_size: idx_t,
) -> Result<BlockPointer, String> {
    let index = meta_pointer.get_block_index();
    if index >= METADATA_BLOCK_COUNT as u32 {
        return Err(format!(
            "Metadata block index {} exceeds metadata block count {}",
            index, METADATA_BLOCK_COUNT
        ));
    }
    let offset = index as idx_t * metadata_block_size + meta_pointer.offset as idx_t;
    Ok(BlockPointer::new(
        meta_pointer.get_block_id(),
        offset as u32,
    ))
}

/// `BlockPointer` -> `MetaBlockPointer` using the metadata block size.
pub fn from_block_pointer(
    block_pointer: BlockPointer,
    metadata_block_size: idx_t,
) -> Result<MetaBlockPointer, String> {
    if !block_pointer.is_valid() {
        return Ok(MetaBlockPointer::default());
    }
    let index = block_pointer.offset as idx_t / metadata_block_size;
    let offset = block_pointer.offset as idx_t % metadata_block_size;
    if index >= METADATA_BLOCK_COUNT {
        return Err(format!(
            "Metadata block offset {} exceeds metadata block capacity {}",
            block_pointer.offset,
            metadata_block_size * METADATA_BLOCK_COUNT
        ));
    }
    let block_pointer = block_pointer.block_id as idx_t | index << 56;
    Ok(MetaBlockPointer::new(block_pointer, offset as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_disk_pointer_round_trips_block_index_and_sub_index() {
        for block_index in [0u64, 1, 42, 1000, (1u64 << 55) - 1] {
            for index in 0u32..64 {
                let pointer = get_disk_pointer(block_index, index, 0);
                assert_eq!(pointer.get_block_id(), block_index as i64);
                assert_eq!(pointer.get_block_index(), index);
            }
        }
    }

    #[test]
    fn a_disk_pointer_carries_its_offset() {
        let pointer = get_disk_pointer(5, 3, 42);
        assert_eq!(pointer.offset, 42);
    }

    #[test]
    fn a_block_pointer_round_trips_through_meta() {
        let block_size: idx_t = 8192;
        let meta = get_disk_pointer(7, 3, 100);
        let block = to_block_pointer(meta, block_size).unwrap();
        assert_eq!(block.block_id, 7);
        assert_eq!(block.offset as idx_t, 3 * block_size + 100);
        let back = from_block_pointer(block, block_size).unwrap();
        assert_eq!(back, meta);
    }

    #[test]
    fn an_invalid_block_pointer_yields_a_default_meta_pointer() {
        let block = BlockPointer::default();
        let meta = from_block_pointer(block, 8192).unwrap();
        assert_eq!(meta, MetaBlockPointer::default());
    }

    #[test]
    fn an_out_of_range_block_index_is_refused() {
        let meta = get_disk_pointer(0, 64, 0);
        assert!(to_block_pointer(meta, 8192).is_err());
    }

    #[test]
    fn an_out_of_range_offset_is_refused() {
        let block = BlockPointer::new(1, 8192 * 64);
        assert!(from_block_pointer(block, 8192).is_err());
    }
}
