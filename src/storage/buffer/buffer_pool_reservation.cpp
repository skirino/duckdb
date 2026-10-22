#include "duckdb/storage/buffer/buffer_pool_reservation.hpp"

#include "buffer_rust_bridge.hpp"
#include "duckdb/storage/buffer/buffer_pool.hpp"

namespace duckdb {

BufferPoolReservation::BufferPoolReservation(MemoryTag tag, BufferPool &pool) : tag(tag), pool(pool) {
}

BufferPoolReservation::BufferPoolReservation(BufferPoolReservation &&src) noexcept : tag(src.tag), pool(src.pool) {
	size = src.size;
	src.size = 0;
}

BufferPoolReservation &BufferPoolReservation::operator=(BufferPoolReservation &&src) noexcept {
	auto release = ahirudb::buffer::reservation::move_assign_release(size);
	if (release.has_value()) {
		pool.UpdateUsedMemory(tag, *release);
	}
	tag = src.tag;
	size = src.size;
	src.size = 0;
	return *this;
}

BufferPoolReservation::~BufferPoolReservation() {
	D_ASSERT(size == 0);
}

void BufferPoolReservation::Resize(idx_t new_size) {
	auto delta = ahirudb::buffer::reservation::resize_delta(size, new_size);
	if (!delta.has_value()) {
		throw InternalException("BufferPoolReservation::Resize: size conversion failed");
	}
	pool.UpdateUsedMemory(tag, *delta);
	size = new_size;
}

void BufferPoolReservation::Merge(BufferPoolReservation src) {
	auto merged = ahirudb::buffer::reservation::merge_size(size, src.size);
	if (merged.has_value()) {
		size = *merged;
	}
	src.size = 0;
}

} // namespace duckdb