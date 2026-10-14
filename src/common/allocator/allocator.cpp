#include "duckdb/common/allocator.hpp"

#include "duckdb/common/assert.hpp"
#include "duckdb/common/exception.hpp"
#include "duckdb/common/helper.hpp"
#include "duckdb/common/time_point.hpp"

#include "allocator_rust_bridge.hpp"

#ifdef __GLIBC__
#include <malloc.h>
#endif

namespace duckdb {

constexpr const idx_t Allocator::MAXIMUM_ALLOC_SIZE;

AllocatedData::AllocatedData() : allocator(nullptr), pointer(nullptr), allocated_size(0) {
}

AllocatedData::AllocatedData(Allocator &allocator, data_ptr_t pointer, idx_t allocated_size)
    : allocator(&allocator), pointer(pointer), allocated_size(allocated_size) {
	if (!pointer) {
		throw InternalException("AllocatedData object constructed with nullptr");
	}
}
AllocatedData::~AllocatedData() {
	Reset();
}

AllocatedData::AllocatedData(AllocatedData &&other) noexcept
    : allocator(other.allocator), pointer(nullptr), allocated_size(0) {
	std::swap(pointer, other.pointer);
	std::swap(allocated_size, other.allocated_size);
}

AllocatedData &AllocatedData::operator=(AllocatedData &&other) noexcept {
	std::swap(allocator, other.allocator);
	std::swap(pointer, other.pointer);
	std::swap(allocated_size, other.allocated_size);
	return *this;
}

void AllocatedData::Reset() {
	if (!pointer) {
		return;
	}
	D_ASSERT(allocator);
	allocator->FreeData(pointer, allocated_size);
	allocated_size = 0;
	pointer = nullptr;
}

//===--------------------------------------------------------------------===//
// Debug Info
//===--------------------------------------------------------------------===//
struct AllocatorDebugInfo {
#ifdef DEBUG
	AllocatorDebugInfo();
	~AllocatorDebugInfo();

	void AllocateData(data_ptr_t pointer, idx_t size);
	void FreeData(data_ptr_t pointer, idx_t size);
	void ReallocateData(data_ptr_t pointer, data_ptr_t new_pointer, idx_t old_size, idx_t new_size);

private:
	//! The Rust-side tracker doing the bookkeeping for this
	//! allocator, owned through the opaque pointer the bridge
	//! handed us.
	ahirudb::allocator::debug::DebugTracker *tracker;
#endif
};

PrivateAllocatorData::PrivateAllocatorData() {
}

PrivateAllocatorData::~PrivateAllocatorData() {
}

//===--------------------------------------------------------------------===//
// Allocator
//===--------------------------------------------------------------------===//
Allocator::Allocator()
    : Allocator(Allocator::DefaultAllocate, Allocator::DefaultFree, Allocator::DefaultReallocate, nullptr) {
}

Allocator::Allocator(allocate_function_ptr_t allocate_function_p, free_function_ptr_t free_function_p,
                     reallocate_function_ptr_t reallocate_function_p, unique_ptr<PrivateAllocatorData> private_data_p)
    : allocate_function(allocate_function_p), free_function(free_function_p),
      reallocate_function(reallocate_function_p), private_data(std::move(private_data_p)) {
	D_ASSERT(allocate_function);
	D_ASSERT(free_function);
	D_ASSERT(reallocate_function);
#ifdef DEBUG
	if (!private_data) {
		private_data = make_uniq<PrivateAllocatorData>();
	}
	private_data->debug_info = make_uniq<AllocatorDebugInfo>();
#endif
}

Allocator::~Allocator() {
}

data_ptr_t Allocator::AllocateData(idx_t size) {
	D_ASSERT(size > 0);
	if (size >= MAXIMUM_ALLOC_SIZE) {
		D_ASSERT(false);
		throw InternalException("Requested allocation size of %llu is out of range - maximum allocation size is %llu",
		                        size, MAXIMUM_ALLOC_SIZE);
	}
	auto result = allocate_function(private_data.get(), size);
	if (!result) {
		throw OutOfMemoryException("Failed to allocate block of %llu bytes (bad allocation)", size);
	}
#ifdef DEBUG
	if (ShouldUseDebugInfo()) {
		private_data->debug_info->AllocateData(result, size);
	}
#endif
	return result;
}

void Allocator::FreeData(data_ptr_t pointer, idx_t size) {
	if (!pointer) {
		return;
	}
	D_ASSERT(size > 0);
#ifdef DEBUG
	if (ShouldUseDebugInfo()) {
		private_data->debug_info->FreeData(pointer, size);
	}
#endif
	free_function(private_data.get(), pointer, size);
}

data_ptr_t Allocator::ReallocateData(data_ptr_t pointer, idx_t old_size, idx_t size) {
	if (!pointer) {
		return nullptr;
	}
	if (size >= MAXIMUM_ALLOC_SIZE) {
		D_ASSERT(false);
		throw InternalException(
		    "Requested re-allocation size of %llu is out of range - maximum allocation size is %llu", size,
		    MAXIMUM_ALLOC_SIZE);
	}
	auto new_pointer = reallocate_function(private_data.get(), pointer, old_size, size);
	if (!new_pointer) {
		throw OutOfMemoryException("Failed to re-allocate block of %llu bytes (bad allocation)", size);
	}
#ifdef DEBUG
	if (ShouldUseDebugInfo()) {
		private_data->debug_info->ReallocateData(pointer, new_pointer, old_size, size);
	}
#endif
	return new_pointer;
}
shared_ptr<Allocator> &Allocator::DefaultAllocatorReference() {
	static shared_ptr<Allocator> DEFAULT_ALLOCATOR = make_shared_ptr<Allocator>();
	return DEFAULT_ALLOCATOR;
}

Allocator &Allocator::DefaultAllocator() {
	return *DefaultAllocatorReference();
}

void Allocator::MallocTrim(idx_t pad) {
#ifdef __GLIBC__
	const auto current_tick_ms = TimePoint::GetTickMs();
	if (!ahirudb::allocator::bridge::should_trim(current_tick_ms)) {
		return;
	}
	malloc_trim(pad);
#endif
}

//===--------------------------------------------------------------------===//
// Debug Info (extended)
//===--------------------------------------------------------------------===//
#ifdef DEBUG
AllocatorDebugInfo::AllocatorDebugInfo() {
	tracker = ahirudb::allocator::bridge::new_debug_tracker(
#ifdef DUCKDB_DEBUG_ALLOCATION
	    /*capture_stacks=*/true
#else
	    /*capture_stacks=*/false
#endif
	);
}

AllocatorDebugInfo::~AllocatorDebugInfo() {
	auto report = tracker->leak_report();
	if (report.len != 0) {
		// The report is a DEBUG-only diagnostic printed to
		// stdout, exactly as the original did under
		// DUCKDB_DEBUG_ALLOCATION.
		fwrite(report.ptr, 1, report.len, stdout);
		fflush(stdout);
	}
	ahirudb::allocator::bridge::free_leak_report(report);
	//! Verify that there is no outstanding memory still associated with the allocator
	D_ASSERT(tracker->is_empty());
	ahirudb::allocator::bridge::delete_debug_tracker(tracker);
	tracker = nullptr;
}

void AllocatorDebugInfo::AllocateData(data_ptr_t pointer, idx_t size) {
#ifdef DUCKDB_DEBUG_ALLOCATION
	const auto stack = Exception::GetStackTrace();
	tracker->track_allocate(
	    reinterpret_cast<uint64_t>(pointer), size,
	    rs_std::SliceRef<const unsigned char>(reinterpret_cast<const unsigned char *>(stack.data()), stack.size()));
#else
	tracker->track_allocate(reinterpret_cast<uint64_t>(pointer), size,
	                        rs_std::SliceRef<const unsigned char>(nullptr, 0));
#endif
}

void AllocatorDebugInfo::FreeData(data_ptr_t pointer, idx_t size) {
	D_ASSERT(tracker->track_free(reinterpret_cast<uint64_t>(pointer), size));
}

void AllocatorDebugInfo::ReallocateData(data_ptr_t pointer, data_ptr_t new_pointer, idx_t old_size, idx_t new_size) {
#ifdef DUCKDB_DEBUG_ALLOCATION
	const auto stack = Exception::GetStackTrace();
	D_ASSERT(tracker->track_reallocate(
	    reinterpret_cast<uint64_t>(pointer), reinterpret_cast<uint64_t>(new_pointer), old_size, new_size,
	    rs_std::SliceRef<const unsigned char>(reinterpret_cast<const unsigned char *>(stack.data()), stack.size())));
#else
	D_ASSERT(tracker->track_reallocate(reinterpret_cast<uint64_t>(pointer), reinterpret_cast<uint64_t>(new_pointer),
	                                   old_size, new_size, rs_std::SliceRef<const unsigned char>(nullptr, 0)));
#endif
}

#endif

} // namespace duckdb
