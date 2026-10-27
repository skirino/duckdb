#include "duckdb/storage/metadata/metadata_manager.hpp"
#include "metadata_rust_bridge.hpp"

#include "duckdb/common/serializer/read_stream.hpp"
#include "duckdb/common/serializer/write_stream.hpp"
#include "duckdb/main/client_context.hpp"
#include "duckdb/storage/buffer/block_handle.hpp"
#include "duckdb/storage/buffer_manager.hpp"
#include "duckdb/storage/database_size.hpp"

namespace duckdb {

MetadataBlock::MetadataBlock() : block_id(INVALID_BLOCK), dirty(false) {
}

MetadataBlock::MetadataBlock(MetadataBlock &&other) noexcept : dirty(false) {
	std::swap(block, other.block);
	std::swap(block_id, other.block_id);
	std::swap(free_blocks, other.free_blocks);
	auto dirty_val = dirty.load();
	dirty = other.dirty.load();
	other.dirty = dirty_val;
}

MetadataBlock &MetadataBlock::operator=(MetadataBlock &&other) noexcept {
	std::swap(block, other.block);
	std::swap(block_id, other.block_id);
	std::swap(free_blocks, other.free_blocks);
	auto dirty_val = dirty.load();
	dirty = other.dirty.load();
	other.dirty = dirty_val;
	return *this;
}

string MetadataBlock::ToString() const {
	string result;
	for (idx_t i = 0; i < MetadataManager::METADATA_BLOCK_COUNT; i++) {
		if (std::find(free_blocks.begin(), free_blocks.end(), i) != free_blocks.end()) {
			if (!result.empty()) {
				result += ", ";
			}
			result += to_string(i);
		}
	}
	return "block_id: " + to_string(block_id) + " [" + result + "]";
}

MetadataManager::MetadataManager(BlockManager &block_manager, BufferManager &buffer_manager)
    : block_manager(block_manager), buffer_manager(buffer_manager), metadata_state(nullptr) {
	metadata_state = rust::metadata::bridge::new_metadata_manager();
}

MetadataManager::~MetadataManager() {
	if (metadata_state != nullptr) {
		rust::metadata::bridge::delete_metadata_manager(static_cast<rust::MetadataManagerState *>(metadata_state));
	}
}

MetadataHandle MetadataManager::AllocateHandle() {
	// check if there is any free space left in an existing block
	// if not allocate a new block
	MetadataPointer pointer;
	unique_lock<mutex> guard(block_lock);
	block_id_t free_block = INVALID_BLOCK;
	for (auto &kv : blocks) {
		auto &block = kv.second;
		D_ASSERT(kv.first == block.block_id);
		if (!block.free_blocks.empty()) {
			free_block = kv.first;
			break;
		}
	}
	guard.unlock();
	if (free_block == INVALID_BLOCK || free_block > PeekNextBlockId()) {
		free_block = AllocateNewBlock(guard);
	} else {
		guard.lock();
	}
	D_ASSERT(guard.owns_lock());
	D_ASSERT(free_block != INVALID_BLOCK);

	// select the first free metadata block we can find
	pointer.block_index = UnsafeNumericCast<idx_t>(free_block);
	auto &block = blocks[free_block];
	// the block is now dirty
	block.dirty = true;
	if (block.block->BlockId() < MAXIMUM_BLOCK) {
		// this block is a disk-backed block, yet we are planning to write to it
		// we need to convert it into a transient block before we can write to it
		ConvertToTransient(guard, block);
		D_ASSERT(block.block->BlockId() >= MAXIMUM_BLOCK);
	}
	D_ASSERT(!block.free_blocks.empty());
	pointer.index = block.free_blocks.back();
	// mark the block as used
	block.free_blocks.pop_back();
	D_ASSERT(pointer.index < METADATA_BLOCK_COUNT);
	guard.unlock();
	// pin the block
	return Pin(pointer);
}

MetadataHandle MetadataManager::Pin(const MetadataPointer &pointer) {
	return Pin(QueryContext(), pointer);
}

MetadataHandle MetadataManager::Pin(const QueryContext &context, const MetadataPointer &pointer) {
	D_ASSERT(pointer.index < METADATA_BLOCK_COUNT);
	shared_ptr<BlockHandle> block_handle;
	{
		lock_guard<mutex> guard(block_lock);
		auto entry = blocks.find(UnsafeNumericCast<int64_t>(pointer.block_index));
		if (entry == blocks.end()) {
			throw InternalException("Trying to pin block %llu - but the block did not exist", pointer.block_index);
		}
		auto &block = entry->second;
		block_handle = block.block;
	}

	MetadataHandle handle;
	handle.pointer.block_index = pointer.block_index;
	handle.pointer.index = pointer.index;
	handle.handle = buffer_manager.Pin(block_handle);
	return handle;
}

void MetadataManager::ConvertToTransient(unique_lock<mutex> &block_lock, MetadataBlock &metadata_block) {
	D_ASSERT(block_lock.owns_lock());
	auto old_block = metadata_block.block;
	block_lock.unlock();
	// pin the old block
	auto old_buffer = buffer_manager.Pin(old_block);

	// allocate a new transient block to replace it
	auto new_buffer = buffer_manager.Allocate(MemoryTag::METADATA, &block_manager, false);
	auto new_block = new_buffer.GetBlockHandle();

	// copy the data to the transient block
	memcpy(new_buffer.GetDataMutable(), old_buffer.Ptr(), block_manager.GetBlockSize());

	// unregister the old block
	block_manager.UnregisterBlock(metadata_block.block_id);

	block_lock.lock();
	metadata_block.block = std::move(new_block);
	metadata_block.dirty = true;
}

block_id_t MetadataManager::AllocateNewBlock(unique_lock<mutex> &block_lock) {
	D_ASSERT(!block_lock.owns_lock());
	auto new_block_id = GetNextBlockId();

	MetadataBlock new_block;
	auto handle = buffer_manager.Allocate(MemoryTag::METADATA, &block_manager, false);
	new_block.block = handle.GetBlockHandle();
	new_block.block_id = new_block_id;
	for (idx_t i = 0; i < METADATA_BLOCK_COUNT; i++) {
		new_block.free_blocks.push_back(NumericCast<uint8_t>(METADATA_BLOCK_COUNT - i - 1));
	}
	new_block.dirty = true;
	// zero-initialize the handle
	memset(handle.GetDataMutable(), 0, block_manager.GetBlockSize());

	block_lock.lock();
	AddBlock(block_lock, std::move(new_block));
	return new_block_id;
}

void MetadataManager::AddBlock(unique_lock<mutex> &block_lock, MetadataBlock new_block, bool if_exists) {
	D_ASSERT(block_lock.owns_lock());
	if (blocks.find(new_block.block_id) != blocks.end()) {
		if (if_exists) {
			return;
		}
		throw InternalException("Block id with id %llu already exists", new_block.block_id);
	}
	blocks[new_block.block_id] = std::move(new_block);
}

void MetadataManager::AddAndRegisterBlock(unique_lock<mutex> &block_lock, MetadataBlock block) {
	if (block.block) {
		throw InternalException("Calling AddAndRegisterBlock on block that already exists");
	}
	if (block.block_id >= MAXIMUM_BLOCK) {
		throw InternalException("AddAndRegisterBlock called with a transient block id");
	}
	block_lock.unlock();
	block.block = block_manager.RegisterBlock(block.block_id);
	block_lock.lock();
	AddBlock(block_lock, std::move(block), true);
}

MetaBlockPointer MetadataManager::GetDiskPointer(const MetadataPointer &pointer, uint32_t offset) {
	auto rust_ptr = rust::metadata::bridge::get_disk_pointer(pointer.block_index, pointer.index, offset);
	return rust::ToCppMetaBlockPointer(rust_ptr);
}

block_id_t MetaBlockPointer::GetBlockId() const {
	return rust::ToRustMetaBlockPointer(*this).get_block_id();
}

uint32_t MetaBlockPointer::GetBlockIndex() const {
	return rust::ToRustMetaBlockPointer(*this).get_block_index();
}

MetadataPointer MetadataManager::FromDiskPointer(MetaBlockPointer pointer) {
	unique_lock<mutex> guard(block_lock);
	return FromDiskPointerInternal(guard, pointer);
}

MetadataPointer MetadataManager::FromDiskPointerInternal(unique_lock<mutex> &block_lock, MetaBlockPointer pointer) {
	auto rust_ptr = rust::ToRustMetaBlockPointer(pointer);
	auto index = rust_ptr.get_block_index();
	if (index >= METADATA_BLOCK_COUNT) {
		throw IOException("Metadata block index %llu exceeds metadata block count %llu", index, METADATA_BLOCK_COUNT);
	}

	auto block_id = rust_ptr.get_block_id();
	auto entry = blocks.find(block_id);
	if (entry == blocks.end()) { // LCOV_EXCL_START
		throw InternalException("Failed to load metadata pointer (id %llu, idx %llu, ptr %llu)\n", block_id, index,
		                        pointer.block_pointer);
	} // LCOV_EXCL_STOP
	MetadataPointer result;
	result.block_index = UnsafeNumericCast<idx_t>(block_id);
	result.index = UnsafeNumericCast<uint8_t>(index);
	return result;
}

MetadataPointer MetadataManager::RegisterDiskPointer(MetaBlockPointer pointer) {
	unique_lock<mutex> guard(block_lock);

	auto rust_ptr = rust::ToRustMetaBlockPointer(pointer);
	auto block_id = rust_ptr.get_block_id();
	MetadataBlock block;
	block.block_id = block_id;
	AddAndRegisterBlock(guard, std::move(block));
	return FromDiskPointerInternal(guard, pointer);
}

BlockPointer MetadataManager::ToBlockPointer(MetaBlockPointer meta_pointer, const idx_t metadata_block_size) {
	auto rust_meta = rust::ToRustMetaBlockPointer(meta_pointer);
	auto rust_result = rust::metadata::bridge::to_block_pointer(rust_meta, metadata_block_size);
	if (!rust_result.has_value()) {
		throw IOException("Metadata block index %llu exceeds metadata block count %llu",
		                 rust_meta.get_block_index(),
		                 MetadataManager::METADATA_BLOCK_COUNT);
	}
	return rust::ToCppBlockPointer(rust_result.value());
}

MetaBlockPointer MetadataManager::FromBlockPointer(BlockPointer block_pointer, const idx_t metadata_block_size) {
	auto rust_block = rust::ToRustBlockPointer(block_pointer);
	auto rust_result = rust::metadata::bridge::from_block_pointer(rust_block, metadata_block_size);
	if (!rust_result.has_value()) {
		throw IOException("Metadata block offset %llu exceeds metadata block capacity %llu", block_pointer.offset,
		                 metadata_block_size * MetadataManager::METADATA_BLOCK_COUNT);
	}
	return rust::ToCppMetaBlockPointer(rust_result.value());
}

idx_t MetadataManager::BlockCount() {
	return blocks.size();
}

void MetadataManager::Flush(QueryContext context) {
	// Write the blocks of the metadata manager to disk.
	const idx_t total_metadata_size = GetMetadataBlockSize() * METADATA_BLOCK_COUNT;

	unique_lock<mutex> guard(block_lock, std::defer_lock);
	for (auto &kv : blocks) {
		auto &block = kv.second;
		if (!block.dirty) {
			if (block.block->BlockId() >= MAXIMUM_BLOCK) {
				throw InternalException("Transient blocks must always be marked as dirty");
			}
			continue;
		}
		auto block_handle = block.block;
		auto handle = buffer_manager.Pin(block_handle);
		// zero-initialize the few leftover bytes
		memset(handle.GetDataMutable() + total_metadata_size, 0, block_manager.GetBlockSize() - total_metadata_size);
		D_ASSERT(kv.first == block.block_id);
		if (block_handle->BlockId() >= MAXIMUM_BLOCK) {
			// Convert the temporary block to a persistent block.
			// we cannot use ConvertToPersistent as another thread might still be reading the block
			// so we use the safe version of ConvertToPersistent
			auto new_block = block_manager.ConvertToPersistent(context, kv.first, std::move(block_handle),
			                                                   std::move(handle), ConvertToPersistentMode::THREAD_SAFE);

			guard.lock();
			block.block = std::move(new_block);
			guard.unlock();
		} else {
			// Already a persistent block, so we only need to write it.
			D_ASSERT(block.block->BlockId() == block.block_id);
			block_manager.Write(context, handle.GetFileBuffer(), block.block_id);
		}
		// the block is no longer dirty
		block.dirty = false;
	}
}

void MetadataManager::Write(WriteStream &sink) {
	sink.Write<uint64_t>(blocks.size());
	for (auto &kv : blocks) {
		kv.second.Write(sink);
	}
}

void MetadataManager::Read(ReadStream &source) {
	auto block_count = source.Read<uint64_t>();
	for (idx_t i = 0; i < block_count; i++) {
		auto block = MetadataBlock::Read(source);

		unique_lock<mutex> guard(block_lock);
		auto entry = blocks.find(block.block_id);
		if (entry == blocks.end()) {
			// block does not exist yet
			AddAndRegisterBlock(guard, std::move(block));
		} else {
			// block was already created - only copy over the free list
			entry->second.free_blocks = std::move(block.free_blocks);
		}
	}
}

void MetadataBlock::Write(WriteStream &sink) {
	sink.Write<block_id_t>(block_id);
	sink.Write<idx_t>(FreeBlocksToInteger());
}

idx_t MetadataManager::GetMetadataBlockSize() const {
	return rust::metadata::bridge::get_metadata_block_size(block_manager.GetBlockSize());
}

MetadataBlock MetadataBlock::Read(ReadStream &source) {
	MetadataBlock result;
	result.block_id = source.Read<block_id_t>();
	auto free_list = source.Read<idx_t>();
	result.FreeBlocksFromInteger(free_list);
	return result;
}

idx_t MetadataBlock::FreeBlocksToInteger() {
	return rust::metadata::bridge::free_blocks_to_integer(free_blocks.data(), free_blocks.size());
}

vector<uint8_t> MetadataBlock::BlocksFromInteger(idx_t free_list) {
	std::vector<std::uint8_t> result;
	auto list = rust::metadata::bridge::blocks_from_integer(free_list);
	rust::CopyBlockList(list, result);
	return result;
}

void MetadataBlock::FreeBlocksFromInteger(idx_t free_list) {
	free_blocks.clear();
	if (free_list == 0) {
		return;
	}
	auto list = rust::metadata::bridge::free_blocks_from_integer(free_list);
	rust::CopyBlockList(list, free_blocks);
}

void MetadataManager::MarkBlocksAsModified() {
	unique_lock<mutex> guard(block_lock);
	// for any blocks that were modified in the last checkpoint - set them to free blocks currently
	for (auto &kv : modified_blocks) {
		auto block_id = kv.first;
		idx_t modified_list = kv.second;
		auto entry = blocks.find(block_id);
		D_ASSERT(entry != blocks.end());
		auto &block = entry->second;
		idx_t current_free_blocks = block.FreeBlocksToInteger();
		// merge the current set of free blocks with the modified blocks
		idx_t new_free_blocks = rust::metadata::bridge::merge_modified_free_blocks(current_free_blocks, modified_list);
		if (new_free_blocks == NumericLimits<idx_t>::Maximum()) {
			// if new free_blocks is all blocks - mark entire block as modified
			blocks.erase(entry);

			guard.unlock();
			block_manager.MarkBlockAsModified(block_id);
			guard.lock();
		} else {
			// set the new set of free blocks
			block.FreeBlocksFromInteger(new_free_blocks);
		}
	}

	modified_blocks.clear();

	for (auto &kv : blocks) {
		auto &block = kv.second;
		idx_t free_list = block.FreeBlocksToInteger();
		idx_t occupied_list = rust::metadata::bridge::occupied_blocks(free_list);
		modified_blocks[block.block_id] = occupied_list;
	}
}

void MetadataManager::ClearModifiedBlocks(const vector<MetaBlockPointer> &pointers) {
	if (pointers.empty()) {
		return;
	}
	unique_lock<mutex> guard(block_lock);
	for (auto &pointer : pointers) {
		auto rust_ptr = rust::ToRustMetaBlockPointer(pointer);
		auto block_id = rust_ptr.get_block_id();
		auto block_index = rust_ptr.get_block_index();
		auto entry = modified_blocks.find(block_id);
		if (entry == modified_blocks.end()) {
			throw InternalException("ClearModifiedBlocks - Block id %llu not found in modified_blocks", block_id);
		}
		auto &modified_list = entry->second;
		// unset the bit
		modified_list = rust::metadata::bridge::clear_modified_bit(modified_list, block_index);
	}
}

bool MetadataManager::BlockIsModified(const MetaBlockPointer &pointer) {
	unique_lock<mutex> guard(block_lock);
	auto rust_ptr = rust::ToRustMetaBlockPointer(pointer);
	auto block_id = rust_ptr.get_block_id();
	auto entry = blocks.find(block_id);
	if (entry == blocks.end()) {
		throw InternalException("BlockIsNotModified - Block id %llu not found in blocks", block_id);
	}
	auto entry2 = modified_blocks.find(block_id);
	return entry2 != modified_blocks.end();
}

bool MetadataManager::BlockHasBeenCleared(const MetaBlockPointer &pointer) {
	unique_lock<mutex> guard(block_lock);
	auto rust_ptr = rust::ToRustMetaBlockPointer(pointer);
	auto block_id = rust_ptr.get_block_id();
	auto block_index = rust_ptr.get_block_index();
	auto entry = modified_blocks.find(block_id);
	if (entry == modified_blocks.end()) {
		throw InternalException("BlockHasBeenCleared - Block id %llu not found in modified_blocks", block_id);
	}
	auto &modified_list = entry->second;
	return rust::metadata::bridge::bit_is_cleared(modified_list, block_index);
}

vector<MetadataBlockInfo> MetadataManager::GetMetadataInfo() const {
	std::vector<MetadataBlockInfo> result;
	unique_lock<mutex> guard(block_lock);
	for (auto &block : blocks) {
		MetadataBlockInfo block_info;
		block_info.block_id = block.second.block_id;
		block_info.total_blocks = MetadataManager::METADATA_BLOCK_COUNT;
		for (auto free_block : block.second.free_blocks) {
			block_info.free_list.push_back(free_block);
		}
		std::sort(block_info.free_list.begin(), block_info.free_list.end());
		result.push_back(std::move(block_info));
	}
	std::sort(result.begin(), result.end(),
	          [](const MetadataBlockInfo &a, const MetadataBlockInfo &b) { return a.block_id < b.block_id; });
	return result;
}

vector<shared_ptr<BlockHandle>> MetadataManager::GetBlocks() const {
	std::vector<shared_ptr<BlockHandle>> result;
	unique_lock<mutex> guard(block_lock);
	for (auto &entry : blocks) {
		result.push_back(entry.second.block);
	}
	return result;
}

block_id_t MetadataManager::PeekNextBlockId() const {
	return block_manager.PeekFreeBlockId();
}

block_id_t MetadataManager::GetNextBlockId() const {
	return block_manager.GetFreeBlockIdForCheckpoint();
}

} // namespace duckdb
