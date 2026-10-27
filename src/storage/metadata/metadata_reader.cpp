//===----------------------------------------------------------------------===//
//                         DuckDB
//
// storage/metadata/metadata_reader.cpp
//
//===----------------------------------------------------------------------===//

#include "duckdb/storage/metadata/metadata_reader.hpp"
#include "metadata_rust_bridge.hpp"

#include "duckdb/main/client_context.hpp"

namespace duckdb {

MetadataReader::MetadataReader(MetadataManager &manager, MetaBlockPointer pointer,
                               optional_ptr<vector<MetaBlockPointer>> read_pointers_p, BlockReaderType type)
    : manager(manager), type(type), next_pointer(pointer), has_next_block(true), read_pointers(read_pointers_p),
      index(0), offset(0), next_offset(pointer.offset), capacity(0) {
}

MetadataReader::MetadataReader(MetadataManager &manager, BlockPointer pointer)
    : MetadataReader(manager, MetadataManager::FromBlockPointer(pointer, manager.GetMetadataBlockSize())) {
}

MetadataReader::~MetadataReader() {
}

MetadataPointer MetadataReader::FromDiskPointer(MetaBlockPointer pointer) {
	if (type == BlockReaderType::EXISTING_BLOCKS) {
		return manager.FromDiskPointer(pointer);
	} else {
		return manager.RegisterDiskPointer(pointer);
	}
}

void MetadataReader::ReadData(data_ptr_t buffer, idx_t read_size) {
	ReadData(QueryContext(), buffer, read_size);
}

void MetadataReader::ReadData(QueryContext context, data_ptr_t buffer, idx_t read_size) {
	auto rust_segments = rust::metadata::bridge::compute_read_data(offset, capacity, read_size, GetMetadataManager().GetMetadataBlockSize(), next_offset);
	for (size_t i = 0; i < rust_segments.count; i++) {
		auto &seg = rust_segments.segments[i];
		if (seg.copy_amount > 0) {
			memcpy(buffer, Ptr(), seg.copy_amount);
			read_size -= seg.copy_amount;
			buffer += seg.copy_amount;
			offset += seg.copy_amount;
		}
		if (seg.next_block_after) {
			ReadNextBlock(context);
		}
	}
	rust::metadata::bridge::free_read_segment_list(rust_segments);
}

MetaBlockPointer MetadataReader::GetMetaBlockPointer() {
	if (capacity == 0) {
		throw InternalException("GetMetaBlockPointer called but there is no active pointer");
	}
	auto rust_ptr = rust::metadata::bridge::reader_get_meta_block_pointer(block.pointer.block_index, block.pointer.index, UnsafeNumericCast<uint32_t>(offset));
	return rust::ToCppMetaBlockPointer(rust_ptr);
}

void MetadataReader::ReadNextBlock() {
	ReadNextBlock(QueryContext());
}

void MetadataReader::ReadNextBlock(QueryContext context) {
	if (!has_next_block) {
		throw IOException("No more data remaining in MetadataReader");
	}
	if (read_pointers) {
		read_pointers->push_back(next_pointer);
	}
	auto next_disk_pointer = FromDiskPointer(next_pointer);
	block = manager.Pin(context, next_disk_pointer);
	index = next_disk_pointer.index;

	idx_t next_block = Load<idx_t>(BasePtr());
	if (rust::metadata::bridge::chain_link_is_terminal(next_block)) {
		has_next_block = false;
	} else {
		next_pointer = MetaBlockPointer(next_block, 0);
	}
	auto [new_offset, new_capacity] = rust::metadata::bridge::compute_read_next_block_state(next_offset, GetMetadataManager().GetMetadataBlockSize());
	if (new_offset > GetMetadataManager().GetMetadataBlockSize()) {
		throw InternalException("next_offset cannot be bigger than block size");
	}
	offset = new_offset;
	next_offset = sizeof(block_id_t);
	capacity = new_capacity;
}

data_ptr_t MetadataReader::BasePtr() {
	return block.handle.GetDataMutable() + index * GetMetadataManager().GetMetadataBlockSize();
}

data_ptr_t MetadataReader::Ptr() {
	return BasePtr() + offset;
}

} // namespace duckdb