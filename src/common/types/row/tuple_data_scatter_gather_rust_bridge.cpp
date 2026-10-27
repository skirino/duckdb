//===----------------------------------------------------------------------===//
//                         DuckDB
//
// common/types/row/tuple_data_scatter_gather_rust_bridge.cpp
//
// The C++ half of the `TupleDataCollection` scatter/gather module: the
// collection-level entry points, the heap-size computation, and the thin
// trampolines that reduce a `Vector` and its `TupleDataVectorFormat` to the
// arrays the Rust kernels in `ahirudb::tuple_data::scatter_gather` read.
//
// The kernels themselves — the row writes, the validity bits, the string heap
// copies and the list/array serialization — live in Rust. What is left here is
// the recursion through nested types, which follows the per-type function
// tables, and the type dispatch that builds those tables.
//
//===----------------------------------------------------------------------===//

#include "tuple_data_scatter_gather_rust_bridge.hpp"

#include "duckdb/common/enum_util.hpp"
#include "duckdb/common/smaller_binary.hpp"
#include "duckdb/common/sorting/sort_key.hpp"
#include "duckdb/common/type_visitor.hpp"
#include "duckdb/common/types/null_value.hpp"
#include "duckdb/common/uhugeint.hpp"
#include "duckdb/common/vector/array_vector.hpp"
#include "duckdb/common/vector/constant_vector.hpp"
#include "duckdb/common/vector/list_vector.hpp"
#include "duckdb/common/vector/map_vector.hpp"
#include "duckdb/common/vector/struct_vector.hpp"

namespace duckdb {

using ValidityBytes = TupleDataLayout::ValidityBytes;

void ThrowTupleDataScatterGatherError(const ::rs::alloc::string::String &error) {
	throw InternalException("%s", string(std::string_view(error.as_str())).c_str());
}

//------------------------------------------------------------------------------
// Heap sizes
//------------------------------------------------------------------------------

static void ResetCombinedListData(vector<TupleDataVectorFormat> &vector_data) {
#ifdef D_ASSERT_IS_ENABLED
	for (auto &vd : vector_data) {
		vd.combined_list_data = nullptr;
		ResetCombinedListData(vd.children);
	}
#else
	(void)vector_data;
#endif
}

void TupleDataCollection::ComputeHeapSizes(TupleDataChunkState &chunk_state, const DataChunk &new_chunk,
                                           const SelectionVector &append_sel, const idx_t append_count) {
	ResetCombinedListData(chunk_state.vector_data);

	auto heap_sizes = FlatVector::GetDataMutable<idx_t>(chunk_state.heap_sizes);
	std::fill_n(heap_sizes, append_count, 0);

	for (idx_t col_idx = 0; col_idx < new_chunk.ColumnCount(); col_idx++) {
		const auto &source_v = new_chunk.data[col_idx];
		auto &source_format = chunk_state.vector_data[col_idx];
		ComputeHeapSizes(chunk_state.heap_sizes, source_v, source_format, append_sel, append_count);
	}
}

static idx_t StringHeapSize(const string_t &val) {
	return !val.IsInlined() * val.GetSize();
}

#if !DUCKDB_SMALLER_BINARY(tuple_data_heap_sizes)
template <bool ALL_VALID, bool HAS_APPEND_SEL, bool HAS_SOURCE_SEL>
#endif
static void ComputeStringHeapSizesInternal(idx_t *const heap_sizes, const UnifiedVectorFormat &source_vector_data,
                                           const SelectionVector &append_sel, const idx_t append_count) {
	const auto source_data = UnifiedVectorFormat::GetData<string_t>(source_vector_data);
	const auto &source_sel = *source_vector_data.sel;
	const auto &source_validity = source_vector_data.validity;

#if DUCKDB_SMALLER_BINARY(tuple_data_heap_sizes)
	const auto ALL_VALID = source_validity.CannotHaveNull();
	const auto HAS_APPEND_SEL = append_sel.IsSet();
	const auto HAS_SOURCE_SEL = source_sel.IsSet();
#endif

	// Fully branchless loop
	const auto null_string_size = StringHeapSize(NullValue<string_t>());
	for (idx_t i = 0; i < append_count; i++) {
		const auto append_idx = HAS_APPEND_SEL ? append_sel.get_index_unsafe(i) : i;
		const auto source_idx = HAS_SOURCE_SEL ? source_sel.get_index_unsafe(append_idx) : append_idx;
		const auto valid = ALL_VALID || source_validity.RowIsValidUnsafe(source_idx);
		heap_sizes[i] += valid * StringHeapSize(source_data[source_idx]) + !valid * null_string_size;
	}
}

void TupleDataCollection::ComputeHeapSizes(Vector &heap_sizes_v, const Vector &source_v,
                                           TupleDataVectorFormat &source_format, const SelectionVector &append_sel,
                                           const idx_t append_count) {
	const auto type = source_v.GetType().InternalType();
	if (type != PhysicalType::VARCHAR && type != PhysicalType::STRUCT && type != PhysicalType::LIST &&
	    type != PhysicalType::ARRAY) {
		return;
	}

	const auto heap_sizes = FlatVector::GetDataMutable<idx_t>(heap_sizes_v);

	// Source
	const auto &source_vector_data = source_format.unified;
	const auto &source_sel = *source_vector_data.sel;
	const auto &source_validity = source_vector_data.validity;

	switch (type) {
	case PhysicalType::VARCHAR: {
		// Only non-inlined strings are stored in the heap
#if DUCKDB_SMALLER_BINARY(tuple_data_heap_sizes)
		ComputeStringHeapSizesInternal(heap_sizes, source_vector_data, append_sel, append_count);
#else
		if (source_validity.CannotHaveNull()) {
			if (append_sel.IsSet()) {
				if (source_sel.IsSet()) {
					ComputeStringHeapSizesInternal<true, true, true>(heap_sizes, source_vector_data, append_sel,
					                                                 append_count);
				} else {
					ComputeStringHeapSizesInternal<true, true, false>(heap_sizes, source_vector_data, append_sel,
					                                                  append_count);
				}
			} else {
				if (source_sel.IsSet()) {
					ComputeStringHeapSizesInternal<true, false, true>(heap_sizes, source_vector_data, append_sel,
					                                                  append_count);
				} else {
					ComputeStringHeapSizesInternal<true, false, false>(heap_sizes, source_vector_data, append_sel,
					                                                   append_count);
				}
			}
		} else {
			if (append_sel.IsSet()) {
				if (source_sel.IsSet()) {
					ComputeStringHeapSizesInternal<false, true, true>(heap_sizes, source_vector_data, append_sel,
					                                                  append_count);
				} else {
					ComputeStringHeapSizesInternal<false, true, false>(heap_sizes, source_vector_data, append_sel,
					                                                   append_count);
				}
			} else {
				if (source_sel.IsSet()) {
					ComputeStringHeapSizesInternal<false, false, true>(heap_sizes, source_vector_data, append_sel,
					                                                   append_count);
				} else {
					ComputeStringHeapSizesInternal<false, false, false>(heap_sizes, source_vector_data, append_sel,
					                                                    append_count);
				}
			}
		}
#endif
		break;
	}
	case PhysicalType::STRUCT: {
		// Recurse through the struct children
		auto &struct_sources = StructVector::GetEntries(source_v);
		for (idx_t struct_col_idx = 0; struct_col_idx < struct_sources.size(); struct_col_idx++) {
			const auto &struct_source = struct_sources[struct_col_idx];
			auto &struct_format = source_format.children[struct_col_idx];
			ComputeHeapSizes(heap_sizes_v, struct_source, struct_format, append_sel, append_count);
		}
		break;
	}
	case PhysicalType::LIST: {
		// Lists are stored entirely in the heap
		for (idx_t i = 0; i < append_count; i++) {
			auto source_idx = source_sel.get_index(append_sel.get_index(i));
			if (source_validity.RowIsValid(source_idx)) {
				heap_sizes[i] += sizeof(uint64_t); // Size of the list
			}
		}

		// Recurse
		D_ASSERT(source_format.children.size() == 1);
		auto &child_source_v = ListVector::GetChild(source_v);
		auto &child_format = source_format.children[0];
		WithinCollectionComputeHeapSizes(heap_sizes_v, child_source_v, child_format, append_sel, append_count,
		                                 source_vector_data);
		break;
	}
	case PhysicalType::ARRAY: {
		// Arrays are stored entirely in the heap
		for (idx_t i = 0; i < append_count; i++) {
			auto source_idx = source_sel.get_index(append_sel.get_index(i));
			if (source_validity.RowIsValid(source_idx)) {
				heap_sizes[i] += sizeof(uint64_t); // Size of the list
			}
		}

		// Recurse
		D_ASSERT(source_format.children.size() == 1);
		auto &child_source_v = ArrayVector::GetChild(source_v);
		auto &child_format = source_format.children[0];
		WithinCollectionComputeHeapSizes(heap_sizes_v, child_source_v, child_format, append_sel, append_count,
		                                 source_vector_data);
		break;
	}
	default:
		throw NotImplementedException("ComputeHeapSizes for %s", EnumUtil::ToString(source_v.GetType().id()));
	}
}

void TupleDataCollection::SortKeyComputeHeapSizes(TupleDataChunkState &chunk_state, const DataChunk &new_chunk,
                                                  const SelectionVector &append_sel, const idx_t append_count,
                                                  const SortKeyType sort_key_type) {
	D_ASSERT(sort_key_type != SortKeyType::INVALID);
	D_ASSERT(!ahirudb::sort::sort_key::is_constant_size(static_cast<uint8_t>(sort_key_type)).value_or(false));
	D_ASSERT(new_chunk.ColumnCount() == 1);
	if (new_chunk.data[0].GetType().id() != LogicalTypeId::BLOB) {
		return;
	}

	const auto heap_sizes = FlatVector::GetDataMutable<idx_t>(chunk_state.heap_sizes);

	const auto &source_vector_data = chunk_state.vector_data[0].unified;
	const auto &source_sel = *source_vector_data.sel;
	const auto &source_validity = source_vector_data.validity;
	const auto source_data = UnifiedVectorFormat::GetData<string_t>(source_vector_data);

	const auto inlined_size = ahirudb::sort::sort_key::inline_length(static_cast<uint8_t>(sort_key_type)).value_or(0);
	for (idx_t i = 0; i < append_count; i++) {
		const auto source_idx = source_sel.get_index(append_sel.get_index(i));
		const auto &string_size = source_data[source_idx].GetSize();
		// If valid, and string cannot be inlined, we need to allocate heap space
		heap_sizes[i] = (source_validity.RowIsValid(source_idx) & (string_size > inlined_size)) * string_size;
	}
}

void TupleDataCollection::WithinCollectionComputeHeapSizes(Vector &heap_sizes_v, const Vector &source_v,
                                                           TupleDataVectorFormat &source_format,
                                                           const SelectionVector &append_sel, const idx_t append_count,
                                                           const UnifiedVectorFormat &list_data) {
	auto type = source_v.GetType().InternalType();
	if (TypeIsConstantSize(type)) {
		ComputeFixedWithinCollectionHeapSizes(heap_sizes_v, source_v, source_format, append_sel, append_count,
		                                      list_data);
		return;
	}
	switch (type) {
	case PhysicalType::VARCHAR:
		StringWithinCollectionComputeHeapSizes(heap_sizes_v, source_v, source_format, append_sel, append_count,
		                                       list_data);
		break;
	case PhysicalType::STRUCT:
		StructWithinCollectionComputeHeapSizes(heap_sizes_v, source_v, source_format, append_sel, append_count,
		                                       list_data);
		break;
	case PhysicalType::LIST:
		CollectionWithinCollectionComputeHeapSizes(heap_sizes_v, source_v, source_format, append_sel, append_count,
		                                           list_data);
		break;
	case PhysicalType::ARRAY:
		CollectionWithinCollectionComputeHeapSizes(heap_sizes_v, source_v, source_format, append_sel, append_count,
		                                           list_data);
		break;
	default:
		throw NotImplementedException("WithinListHeapComputeSizes for %s", EnumUtil::ToString(source_v.GetType().id()));
	}
}

void TupleDataCollection::ComputeFixedWithinCollectionHeapSizes(Vector &heap_sizes_v, const Vector &source_v,
                                                                TupleDataVectorFormat &,
                                                                const SelectionVector &append_sel,
                                                                const idx_t append_count,
                                                                const UnifiedVectorFormat &list_data) {
	// Parent list data
	const auto list_sel = *list_data.sel;
	const auto list_entries = UnifiedVectorFormat::GetDataUnsafe<list_entry_t>(list_data);
	const auto &list_validity = list_data.validity;

	// Target
	auto heap_sizes = FlatVector::GetDataMutable<idx_t>(heap_sizes_v);

	D_ASSERT(TypeIsConstantSize(source_v.GetType().InternalType()));
	const auto type_size = GetTypeIdSize(source_v.GetType().InternalType());
	for (idx_t i = 0; i < append_count; i++) {
		const auto list_idx = list_sel.get_index(append_sel.get_index(i));
		if (!list_validity.RowIsValid(list_idx)) {
			continue; // Original list entry is invalid - no need to serialize the child
		}

		// Get the current list length
		const auto &list_length = list_entries[list_idx].length;
		if (list_length == 0) {
			continue;
		}

		// Size is validity mask and all values
		auto &heap_size = heap_sizes[i];
		heap_size += ValidityBytes::SizeInBytes(list_length);
		heap_size += list_length * type_size;
	}
}

void TupleDataCollection::StringWithinCollectionComputeHeapSizes(Vector &heap_sizes_v, const Vector &,
                                                                 TupleDataVectorFormat &source_format,
                                                                 const SelectionVector &append_sel,
                                                                 const idx_t append_count,
                                                                 const UnifiedVectorFormat &list_data) {
	// Parent list data
	const auto list_sel = *list_data.sel;
	const auto list_entries = UnifiedVectorFormat::GetDataUnsafe<list_entry_t>(list_data);
	const auto &list_validity = list_data.validity;

	// Source
	const auto &source_data = source_format.unified;
	const auto &source_sel = *source_data.sel;
	const auto data = UnifiedVectorFormat::GetData<string_t>(source_data);
	const auto &source_validity = source_data.validity;

	// Target
	auto heap_sizes = FlatVector::GetDataMutable<idx_t>(heap_sizes_v);

	for (idx_t i = 0; i < append_count; i++) {
		const auto list_idx = list_sel.get_index(append_sel.get_index(i));
		if (!list_validity.RowIsValid(list_idx)) {
			continue; // Original list entry is invalid - no need to serialize the child
		}

		// Get the current list entry
		const auto &list_entry = list_entries[list_idx];
		const auto &list_offset = list_entry.offset;
		const auto &list_length = list_entry.length;
		if (list_length == 0) {
			continue;
		}

		// Size is validity mask and all string sizes
		auto &heap_size = heap_sizes[i];
		heap_size += ValidityBytes::SizeInBytes(list_length);
		heap_size += list_length * sizeof(uint32_t);

		// Plus all the actual strings
		for (idx_t child_i = 0; child_i < list_length; child_i++) {
			const auto child_source_idx = source_sel.get_index(list_offset + child_i);
			if (source_validity.RowIsValid(child_source_idx)) {
				heap_size += data[child_source_idx].GetSize();
			}
		}
	}
}

void TupleDataCollection::StructWithinCollectionComputeHeapSizes(Vector &heap_sizes_v, const Vector &source_v,
                                                                 TupleDataVectorFormat &source_format,
                                                                 const SelectionVector &append_sel,
                                                                 const idx_t append_count,
                                                                 const UnifiedVectorFormat &list_data) {
	// Parent list data
	const auto list_sel = *list_data.sel;
	const auto list_entries = UnifiedVectorFormat::GetDataUnsafe<list_entry_t>(list_data);
	const auto &list_validity = list_data.validity;

	// Target
	auto heap_sizes = FlatVector::GetDataMutable<idx_t>(heap_sizes_v);

	for (idx_t i = 0; i < append_count; i++) {
		const auto list_idx = list_sel.get_index(append_sel.get_index(i));
		if (!list_validity.RowIsValid(list_idx)) {
			continue; // Original list entry is invalid - no need to serialize the child
		}

		// Get the current list length
		const auto &list_length = list_entries[list_idx].length;
		if (list_length == 0) {
			continue;
		}

		// Size is just the validity mask
		heap_sizes[i] += ValidityBytes::SizeInBytes(list_length);
	}

	// Recurse
	auto &struct_sources = StructVector::GetEntries(source_v);
	for (idx_t struct_col_idx = 0; struct_col_idx < struct_sources.size(); struct_col_idx++) {
		auto &struct_source = struct_sources[struct_col_idx];

		auto &struct_format = source_format.children[struct_col_idx];
		WithinCollectionComputeHeapSizes(heap_sizes_v, struct_source, struct_format, append_sel, append_count,
		                                 list_data);
	}
}

static void ApplySliceRecursive(const Vector &source_v, TupleDataVectorFormat &source_format,
                                const SelectionVector &combined_sel, const idx_t count) {
	D_ASSERT(source_format.combined_list_data);
	auto &combined_list_data = *source_format.combined_list_data;

	combined_list_data.selection_data = source_format.original_sel->Slice(combined_sel, count);
	source_format.unified.owned_sel.Initialize(combined_list_data.selection_data);
	source_format.unified.sel = &source_format.unified.owned_sel;

	if (source_v.GetType().InternalType() == PhysicalType::STRUCT) {
		// We have to apply it to the child vectors too
		auto &struct_sources = StructVector::GetEntries(source_v);
		for (idx_t struct_col_idx = 0; struct_col_idx < struct_sources.size(); struct_col_idx++) {
			auto &struct_source = struct_sources[struct_col_idx];
			auto &struct_format = source_format.children[struct_col_idx];
#ifdef D_ASSERT_IS_ENABLED
			D_ASSERT(!struct_format.combined_list_data);
#endif
			if (!struct_format.combined_list_data) {
				struct_format.combined_list_data = make_uniq<CombinedListData>();
			}
			ApplySliceRecursive(struct_source, struct_format, *source_format.unified.sel, count);
		}
	}
}

void TupleDataCollection::CollectionWithinCollectionComputeHeapSizes(Vector &heap_sizes_v, const Vector &source_v,
                                                                     TupleDataVectorFormat &source_format,
                                                                     const SelectionVector &append_sel,
                                                                     const idx_t append_count,
                                                                     const UnifiedVectorFormat &list_data) {
	// Parent list data
	const auto list_sel = *list_data.sel;
	const auto list_entries = UnifiedVectorFormat::GetDataUnsafe<list_entry_t>(list_data);
	const auto &list_validity = list_data.validity;

	// Source
	const auto &child_list_data = source_format.unified;
	const auto child_list_sel = *child_list_data.sel;
	const auto child_list_entries = UnifiedVectorFormat::GetDataUnsafe<list_entry_t>(child_list_data);
	const auto &child_list_validity = child_list_data.validity;

	// Target
	auto heap_sizes = FlatVector::GetDataMutable<idx_t>(heap_sizes_v);

	// Figure out actual child list size (can differ from ListVector::GetListSize if dict/const vector),
	// and we cannot use ConstantVector::ZeroSelectionVector because it may need to be longer than STANDARD_VECTOR_SIZE
	idx_t sum_of_sizes = 0;
	for (idx_t i = 0; i < append_count; i++) {
		const auto list_idx = list_sel.get_index(append_sel.get_index(i));
		if (!list_validity.RowIsValid(list_idx)) {
			continue;
		}

		// Get the current list entry
		const auto &list_entry = list_entries[list_idx];
		const auto &list_offset = list_entry.offset;
		const auto &list_length = list_entry.length;
		if (list_length == 0) {
			continue;
		}

		for (idx_t child_i = 0; child_i < list_length; child_i++) {
			const auto child_list_idx = child_list_sel.get_index(list_offset + child_i);
			if (!child_list_validity.RowIsValid(child_list_idx)) {
				continue;
			}

			const auto &child_list_entry = child_list_entries[child_list_idx];
			const auto &child_list_length = child_list_entry.length;

			sum_of_sizes += child_list_length;
		}
	}

	const auto child_list_child_count = MaxValue<idx_t>(
	    sum_of_sizes, source_v.GetType().InternalType() == PhysicalType::LIST ? ListVector::GetListSize(source_v)
	                                                                          : ArrayVector::GetTotalSize(source_v));

	D_ASSERT(source_format.children.size() == 1);
	auto &child_format = source_format.children[0];
#ifdef D_ASSERT_IS_ENABLED
	// Should be deleted by ResetCombinedListData if assertions are enabled
	D_ASSERT(!child_format.combined_list_data);
#endif
	if (!child_format.combined_list_data) {
		child_format.combined_list_data = make_uniq<CombinedListData>();
	}
	auto &combined_list_data = *child_format.combined_list_data;

	// Construct combined list entries and a selection/validity vector for the child list child
	SelectionVector combined_sel(child_list_child_count);
	for (idx_t i = 0; i < child_list_child_count; i++) {
		combined_sel.set_index(i, 0);
	}
	auto &combined_list_entries = combined_list_data.combined_list_entries;
	auto &combined_validity = combined_list_data.combined_validity;
	combined_validity.SetAllValid(STANDARD_VECTOR_SIZE);

	idx_t combined_list_offset = 0;
	for (idx_t i = 0; i < append_count; i++) {
		const auto append_idx = append_sel.get_index(i);
		const auto list_idx = list_sel.get_index(append_idx);
		if (!list_validity.RowIsValid(list_idx)) {
			combined_validity.SetInvalidUnsafe(append_idx);
			continue; // Original list entry is invalid - no need to serialize the child list
		}

		// Get the current list entry
		const auto &list_entry = list_entries[list_idx];
		const auto &list_offset = list_entry.offset;
		const auto &list_length = list_entry.length;

		// Size is the validity mask and the list sizes
		auto &heap_size = heap_sizes[i];
		heap_size += ValidityBytes::SizeInBytes(list_length);
		heap_size += list_length * sizeof(uint64_t);

		idx_t child_list_size = 0;
		for (idx_t child_i = 0; child_i < list_length; child_i++) {
			const auto child_list_idx = child_list_sel.get_index(list_offset + child_i);
			if (child_list_validity.RowIsValid(child_list_idx)) {
				const auto &child_list_entry = child_list_entries[child_list_idx];
				const auto &child_list_offset = child_list_entry.offset;
				const auto &child_list_length = child_list_entry.length;
				if (child_list_length == 0) {
					continue;
				}

				// Add this child's list entries to the combined selection vector
				for (idx_t child_value_i = 0; child_value_i < child_list_length; child_value_i++) {
					auto idx = combined_list_offset + child_list_size + child_value_i;
					auto loc = child_list_offset + child_value_i;
					combined_sel.set_index(idx, loc);
				}

				child_list_size += child_list_length;
			}
		}

		// Combine the child list entries into one
		auto &combined_list_entry = combined_list_entries[append_idx];
		combined_list_entry.offset = combined_list_offset;
		combined_list_entry.length = child_list_size;
		combined_list_offset += child_list_size;
	}

	// TODO: Template this?
	auto &child_source = source_v.GetType().InternalType() == PhysicalType::LIST ? ListVector::GetChild(source_v)
	                                                                             : ArrayVector::GetChild(source_v);
	ApplySliceRecursive(child_source, child_format, combined_sel, child_list_child_count);

	// Create a combined child_list_data to be used as list_data in the recursion
	auto &combined_child_list_data = combined_list_data.combined_data;
	combined_child_list_data.sel = FlatVector::IncrementalSelectionVector();
	combined_child_list_data.data = data_ptr_cast(combined_list_entries);
	combined_child_list_data.validity.Initialize(combined_validity);

	// Recurse
	WithinCollectionComputeHeapSizes(heap_sizes_v, child_source, child_format, append_sel, append_count,
	                                 combined_child_list_data);
}

//------------------------------------------------------------------------------
// Scatter
//------------------------------------------------------------------------------

void TupleDataCollection::Scatter(TupleDataChunkState &chunk_state, const DataChunk &new_chunk,
                                  const SelectionVector &append_sel, const idx_t append_count) const {
#ifdef DEBUG
	Vector heap_locations_copy(LogicalType::POINTER);
	if (!layout.AllConstant()) {
		const auto heap_locations = FlatVector::GetData<data_ptr_t>(chunk_state.heap_locations);
		const auto copied_heap_locations = FlatVector::GetDataMutable<data_ptr_t>(heap_locations_copy);
		for (idx_t i = 0; i < append_count; i++) {
			copied_heap_locations[i] = heap_locations[i];
		}
	}
#endif

	if (layout.IsSortKeyLayout()) {
		const auto &scatter_function = scatter_functions[0];
		scatter_function.function(new_chunk.data[0], chunk_state.vector_data[0], append_sel, append_count, layout,
		                          chunk_state.row_locations, chunk_state.heap_locations, 0,
		                          chunk_state.vector_data[0].unified, scatter_function.child_functions);
	} else {
		const auto row_locations = FlatVector::GetData<data_ptr_t>(chunk_state.row_locations);

		// Set the validity mask for each row before inserting data
		if (layout.CanHaveNull()) {
			ThrowOnTupleDataError(rust::initialize_validity_mask(
			    rs_std::SliceRef<const std::uint64_t>(reinterpret_cast<const std::uint64_t *>(row_locations),
			                                          append_count),
			    append_count, ValidityBytes::SizeInBytes(layout.ColumnCount())));
		}

		if (!layout.AllConstant()) {
			// Set the heap size for each row
			const auto heap_size_offset = layout.GetHeapSizeOffset();
			const auto heap_sizes = FlatVector::GetData<idx_t>(chunk_state.heap_sizes);
			for (idx_t i = 0; i < append_count; i++) {
				Store<idx_t>(heap_sizes[i], row_locations[i] + heap_size_offset);
			}
		}

		// Write the data
		for (const auto &col_idx : chunk_state.column_ids) {
			Scatter(chunk_state, new_chunk.data[col_idx], col_idx, append_sel, append_count);
		}
	}

#ifdef DEBUG
	// Verify that the size of the data written to the heap is the same as the size we computed it would be
	if (!layout.AllConstant()) {
		const auto original_heap_locations = FlatVector::GetData<data_ptr_t>(heap_locations_copy);
		const auto heap_sizes = FlatVector::GetData<idx_t>(chunk_state.heap_sizes);
		const auto offset_heap_locations = FlatVector::GetData<data_ptr_t>(chunk_state.heap_locations);
		for (idx_t i = 0; i < append_count; i++) {
			if (heap_sizes[i] != 0) {
				D_ASSERT(offset_heap_locations[i] == original_heap_locations[i] + heap_sizes[i]);
			}
		}
	}
#endif
}

void TupleDataCollection::Scatter(TupleDataChunkState &chunk_state, const Vector &source, const column_t column_id,
                                  const SelectionVector &append_sel, const idx_t append_count) const {
	const auto &scatter_function = scatter_functions[column_id];
	scatter_function.function(source, chunk_state.vector_data[column_id], append_sel, append_count, layout,
	                          chunk_state.row_locations, chunk_state.heap_locations, column_id,
	                          chunk_state.vector_data[column_id].unified, scatter_function.child_functions);
}

//------------------------------------------------------------------------------
// Scatter kernels
//------------------------------------------------------------------------------

template <class T>
static void TupleDataTemplatedScatter(const Vector &, const TupleDataVectorFormat &source_format,
                                      const SelectionVector &append_sel, const idx_t append_count,
                                      const TupleDataLayout &layout, Vector &row_locations, Vector &, const idx_t col_idx,
                                      const UnifiedVectorFormat &, const vector<TupleDataScatterFunction> &) {
	const auto &source_data = source_format.unified;
	const auto null_value = NullValue<T>();
	ThrowOnTupleDataError(rust::scatter_fixed(
	    RustFormatEntries(source_data, sizeof(T)), sizeof(T),
	    RustBytes(const_data_ptr_cast(&null_value), sizeof(T)), RustSel(*source_data.sel),
	    RustValidity(source_data.validity), RustSel(append_sel), append_count, RustAddresses(row_locations),
	    layout.GetRowWidth(), layout.GetOffsets()[col_idx], col_idx));
}

static void TupleDataStringScatter(const Vector &, const TupleDataVectorFormat &source_format,
                                   const SelectionVector &append_sel, const idx_t append_count,
                                   const TupleDataLayout &layout, Vector &row_locations, Vector &heap_locations,
                                   const idx_t col_idx, const UnifiedVectorFormat &,
                                   const vector<TupleDataScatterFunction> &) {
	const auto &source_data = source_format.unified;
	ThrowOnTupleDataError(rust::scatter_string(
	    RustFormatEntries(source_data, sizeof(string_t)), RustSel(*source_data.sel), RustValidity(source_data.validity),
	    RustSel(append_sel), append_count, RustAddresses(row_locations), RustAddressesMut(heap_locations),
	    layout.GetRowWidth(), layout.GetOffsets()[col_idx], col_idx));
}

static void TupleDataStructScatter(const Vector &source, const TupleDataVectorFormat &source_format,
                                   const SelectionVector &append_sel, const idx_t append_count,
                                   const TupleDataLayout &layout, Vector &row_locations, Vector &heap_locations,
                                   const idx_t col_idx, const UnifiedVectorFormat &dummy_arg,
                                   const vector<TupleDataScatterFunction> &child_functions) {
	// Source
	const auto &source_data = source_format.unified;

	// Create a Vector of pointers to the TupleDataLayout of the STRUCT
	Vector struct_row_locations(LogicalType::POINTER, append_count);
	ThrowOnTupleDataError(rust::scatter_struct(
	    RustSel(*source_data.sel), RustValidity(source_data.validity), RustSel(append_sel), append_count,
	    RustAddresses(row_locations), layout.GetRowWidth(), layout.GetOffsets()[col_idx], col_idx,
	    RustAddressesMut(struct_row_locations)));

	const auto &struct_layout = layout.GetStructLayout(col_idx);
	auto &struct_sources = StructVector::GetEntries(source);
	if (!struct_layout) {
		// Empty struct: no fields to scatter
		D_ASSERT(struct_sources.empty());
		return;
	}
	D_ASSERT(struct_layout->ColumnCount() == struct_sources.size());

	// Set the validity of the entries within the STRUCTs
	ThrowOnTupleDataError(rust::initialize_validity_mask(
	    RustAddresses(struct_row_locations), append_count,
	    ValidityBytes::SizeInBytes(struct_layout->ColumnCount())));

	// Recurse through the struct children
	for (idx_t struct_col_idx = 0; struct_col_idx < struct_layout->ColumnCount(); struct_col_idx++) {
		auto &struct_source = struct_sources[struct_col_idx];
		const auto &struct_source_format = source_format.children[struct_col_idx];
		const auto &struct_scatter_function = child_functions[struct_col_idx];
		struct_scatter_function.function(struct_source, struct_source_format, append_sel, append_count, *struct_layout,
		                                 struct_row_locations, heap_locations, struct_col_idx, dummy_arg,
		                                 struct_scatter_function.child_functions);
	}
}

static void TupleDataListScatter(const Vector &source, const TupleDataVectorFormat &source_format,
                                 const SelectionVector &append_sel, const idx_t append_count,
                                 const TupleDataLayout &layout, Vector &row_locations, Vector &heap_locations,
                                 const idx_t col_idx, const UnifiedVectorFormat &,
                                 const vector<TupleDataScatterFunction> &child_functions) {
	// The Array vector has fake list_entry_t's set by this point too, so both
	// collections scatter their header the same way.
	const auto &source_data = source_format.unified;
	ThrowOnTupleDataError(rust::scatter_collection(
	    RustFormatEntries(source_data, sizeof(list_entry_t)), RustSel(*source_data.sel),
	    RustValidity(source_data.validity), RustSel(append_sel), append_count, RustAddresses(row_locations),
	    RustAddressesMut(heap_locations), layout.GetRowWidth(), layout.GetOffsets()[col_idx], col_idx));

	// Recurse
	D_ASSERT(child_functions.size() == 1);
	auto &child_source = ListVector::GetChild(source);
	auto &child_format = source_format.children[0];
	const auto &child_function = child_functions[0];
	child_function.function(child_source, child_format, append_sel, append_count, layout, row_locations, heap_locations,
	                        col_idx, source_format.unified, child_function.child_functions);
}

static void TupleDataArrayScatter(const Vector &source, const TupleDataVectorFormat &source_format,
                                  const SelectionVector &append_sel, const idx_t append_count,
                                  const TupleDataLayout &layout, Vector &row_locations, Vector &heap_locations,
                                  const idx_t col_idx, const UnifiedVectorFormat &,
                                  const vector<TupleDataScatterFunction> &child_functions) {
	const auto &source_data = source_format.unified;
	ThrowOnTupleDataError(rust::scatter_collection(
	    RustFormatEntries(source_data, sizeof(list_entry_t)), RustSel(*source_data.sel),
	    RustValidity(source_data.validity), RustSel(append_sel), append_count, RustAddresses(row_locations),
	    RustAddressesMut(heap_locations), layout.GetRowWidth(), layout.GetOffsets()[col_idx], col_idx));

	// Recurse
	D_ASSERT(child_functions.size() == 1);
	auto &child_source = ArrayVector::GetChild(source);
	auto &child_format = source_format.children[0];
	const auto &child_function = child_functions[0];
	child_function.function(child_source, child_format, append_sel, append_count, layout, row_locations, heap_locations,
	                        col_idx, source_format.unified, child_function.child_functions);
}

template <class T>
static void TupleDataTemplatedWithinCollectionScatter(const Vector &, const TupleDataVectorFormat &source_format,
                                                      const SelectionVector &append_sel, const idx_t append_count,
                                                      const TupleDataLayout &, Vector &, Vector &heap_locations,
                                                      const idx_t, const UnifiedVectorFormat &list_data,
                                                      const vector<TupleDataScatterFunction> &) {
	const auto &source_data = source_format.unified;
	ThrowOnTupleDataError(rust::scatter_within_fixed(
	    RustFormatEntries(list_data, sizeof(list_entry_t)), RustSel(*list_data.sel),
	    RustValidity(list_data.validity), RustFormatEntries(source_data, sizeof(T)), sizeof(T),
	    RustSel(*source_data.sel), RustValidity(source_data.validity), RustSel(append_sel), append_count,
	    RustAddressesMut(heap_locations)));
}

static void TupleDataStringWithinCollectionScatter(const Vector &, const TupleDataVectorFormat &source_format,
                                                   const SelectionVector &append_sel, const idx_t append_count,
                                                   const TupleDataLayout &, Vector &, Vector &heap_locations,
                                                   const idx_t, const UnifiedVectorFormat &list_data,
                                                   const vector<TupleDataScatterFunction> &) {
	const auto &source_data = source_format.unified;
	ThrowOnTupleDataError(rust::scatter_within_string(
	    RustFormatEntries(list_data, sizeof(list_entry_t)), RustSel(*list_data.sel),
	    RustValidity(list_data.validity), RustFormatEntries(source_data, sizeof(string_t)),
	    RustSel(*source_data.sel), RustValidity(source_data.validity), RustSel(append_sel), append_count,
	    RustAddressesMut(heap_locations)));
}

static void TupleDataStructWithinCollectionScatter(const Vector &source, const TupleDataVectorFormat &source_format,
                                                   const SelectionVector &append_sel, const idx_t append_count,
                                                   const TupleDataLayout &layout, Vector &row_locations,
                                                   Vector &heap_locations, const idx_t,
                                                   const UnifiedVectorFormat &list_data,
                                                   const vector<TupleDataScatterFunction> &child_functions) {
	const auto &source_data = source_format.unified;
	ThrowOnTupleDataError(rust::scatter_within_struct(
	    RustFormatEntries(list_data, sizeof(list_entry_t)), RustSel(*list_data.sel),
	    RustValidity(list_data.validity), RustSel(*source_data.sel), RustValidity(source_data.validity),
	    RustSel(append_sel), append_count, RustAddressesMut(heap_locations)));

	// Recurse through the children
	auto &struct_sources = StructVector::GetEntries(source);
	for (idx_t struct_col_idx = 0; struct_col_idx < struct_sources.size(); struct_col_idx++) {
		auto &struct_source = struct_sources[struct_col_idx];
		auto &struct_format = source_format.children[struct_col_idx];
		const auto &struct_scatter_function = child_functions[struct_col_idx];
		struct_scatter_function.function(struct_source, struct_format, append_sel, append_count, layout, row_locations,
		                                 heap_locations, struct_col_idx, list_data,
		                                 struct_scatter_function.child_functions);
	}
}

template <class COLLECTION_VECTOR>
static void
TupleDataCollectionWithinCollectionScatter(const Vector &child_list, const TupleDataVectorFormat &child_list_format,
                                           const SelectionVector &append_sel, const idx_t append_count,
                                           const TupleDataLayout &layout, Vector &row_locations, Vector &heap_locations,
                                           const idx_t col_idx, const UnifiedVectorFormat &list_data,
                                           const vector<TupleDataScatterFunction> &child_functions) {
	const auto &child_list_data = child_list_format.unified;
	ThrowOnTupleDataError(rust::scatter_within_collection(
	    RustFormatEntries(list_data, sizeof(list_entry_t)), RustSel(*list_data.sel),
	    RustValidity(list_data.validity), RustFormatEntries(child_list_data, sizeof(list_entry_t)),
	    RustSel(*child_list_data.sel), RustValidity(child_list_data.validity), RustSel(append_sel), append_count,
	    RustAddressesMut(heap_locations)));

	// Recurse
	D_ASSERT(child_functions.size() == 1);
	auto &child_vec = COLLECTION_VECTOR::GetChild(child_list);
	auto &child_format = child_list_format.children[0];
	auto &combined_child_list_data = child_format.combined_list_data->combined_data;
	const auto &child_function = child_functions[0];
	child_function.function(child_vec, child_format, append_sel, append_count, layout, row_locations, heap_locations,
	                        col_idx, combined_child_list_data, child_function.child_functions);
}

//------------------------------------------------------------------------------
// Sort key scatter
//------------------------------------------------------------------------------

template <class T, SortKeyType SORT_KEY_TYPE>
void TupleDataSortKeyScatter(const Vector &, const TupleDataVectorFormat &source_format,
                             const SelectionVector &append_sel, const idx_t append_count, const TupleDataLayout &layout,
                             Vector &row_locations, Vector &heap_locations, const idx_t, const UnifiedVectorFormat &,
                             const vector<TupleDataScatterFunction> &) {
	D_ASSERT(layout.IsSortKeyLayout());
	D_ASSERT(layout.GetSortKeyType() == SORT_KEY_TYPE);
	using SORT_KEY = SortKey<SORT_KEY_TYPE>;

	// Source
	const auto &source_data = source_format.unified;
	const auto &source_sel = *source_data.sel;
	const auto data = UnifiedVectorFormat::GetData<T>(source_data);
	const auto &validity = source_data.validity;

	// Target
	const auto target_locations = FlatVector::GetDataMutable<SORT_KEY *>(row_locations);
	const auto target_heap_locations = FlatVector::GetDataMutable<data_ptr_t>(heap_locations);

	if (validity.CannotHaveNull()) {
		// Fast path
		if (!append_sel.IsSet() && !source_sel.IsSet()) {
			for (idx_t i = 0; i < append_count; i++) {
				target_locations[i]->Construct(data[i], target_heap_locations[i]);
			}
		} else {
			for (idx_t i = 0; i < append_count; i++) {
				const auto source_idx = source_sel.get_index(append_sel.get_index(i));
				target_locations[i]->Construct(data[source_idx], target_heap_locations[i]);
			}
		}
	} else {
		for (idx_t i = 0; i < append_count; i++) {
			const auto source_idx = source_sel.get_index(append_sel.get_index(i));
			// validity.CannotHaveNull() may not be true when doing aggressive vector verification
			// but the actual values should always all be valid
			D_ASSERT(validity.RowIsValid(source_idx));
			target_locations[i]->Construct(data[source_idx], target_heap_locations[i]);
		}
	}
}

//------------------------------------------------------------------------------
// Get Scatter Function
//------------------------------------------------------------------------------

template <class T>
tuple_data_scatter_function_t TupleDataGetScatterFunction(bool within_collection) {
	return within_collection ? TupleDataTemplatedWithinCollectionScatter<T> : TupleDataTemplatedScatter<T>;
}

TupleDataScatterFunction TupleDataCollection::GetScatterFunction(const LogicalType &type, bool within_collection) {
	TupleDataScatterFunction result;
	if (type.InternalType() == PhysicalType::VARCHAR) {
		result.function = within_collection ? TupleDataStringWithinCollectionScatter : TupleDataStringScatter;
		return result;
	}
	switch (type.InternalType()) {
	case PhysicalType::BOOL:
		result.function = TupleDataGetScatterFunction<bool>(within_collection);
		break;
	case PhysicalType::INT8:
		result.function = TupleDataGetScatterFunction<int8_t>(within_collection);
		break;
	case PhysicalType::INT16:
		result.function = TupleDataGetScatterFunction<int16_t>(within_collection);
		break;
	case PhysicalType::INT32:
		result.function = TupleDataGetScatterFunction<int32_t>(within_collection);
		break;
	case PhysicalType::INT64:
		result.function = TupleDataGetScatterFunction<int64_t>(within_collection);
		break;
	case PhysicalType::INT128:
		result.function = TupleDataGetScatterFunction<hugeint_t>(within_collection);
		break;
	case PhysicalType::UINT8:
		result.function = TupleDataGetScatterFunction<uint8_t>(within_collection);
		break;
	case PhysicalType::UINT16:
		result.function = TupleDataGetScatterFunction<uint16_t>(within_collection);
		break;
	case PhysicalType::UINT32:
		result.function = TupleDataGetScatterFunction<uint32_t>(within_collection);
		break;
	case PhysicalType::UINT64:
		result.function = TupleDataGetScatterFunction<uint64_t>(within_collection);
		break;
	case PhysicalType::UINT128:
		result.function = TupleDataGetScatterFunction<uhugeint_t>(within_collection);
		break;
	case PhysicalType::FLOAT:
		result.function = TupleDataGetScatterFunction<float>(within_collection);
		break;
	case PhysicalType::DOUBLE:
		result.function = TupleDataGetScatterFunction<double>(within_collection);
		break;
	case PhysicalType::INTERVAL:
		result.function = TupleDataGetScatterFunction<interval_t>(within_collection);
		break;
	case PhysicalType::STRUCT: {
		result.function = within_collection ? TupleDataStructWithinCollectionScatter : TupleDataStructScatter;
		for (const auto &child_type : StructType::GetChildTypes(type)) {
			result.child_functions.push_back(GetScatterFunction(child_type.second, within_collection));
		}
		break;
	}
	case PhysicalType::LIST:
		result.function =
		    within_collection ? TupleDataCollectionWithinCollectionScatter<ListVector> : TupleDataListScatter;
		result.child_functions.emplace_back(GetScatterFunction(ListType::GetChildType(type), true));
		break;
	case PhysicalType::ARRAY:
		result.function =
		    within_collection ? TupleDataCollectionWithinCollectionScatter<ArrayVector> : TupleDataArrayScatter;
		result.child_functions.emplace_back(GetScatterFunction(ArrayType::GetChildType(type), true));
		break;
	default:
		throw InternalException("Unsupported type for TupleDataCollection::GetScatterFunction");
	}
	return result;
}

template <class T>
TupleDataScatterFunction GetSortKeyScatterFunctionInternal(SortKeyType sort_key_type) {
	TupleDataScatterFunction result;
	switch (sort_key_type) {
#define DUCKDB_SORT_KEY_CASE(SORT_KEY_TYPE)                                                                            \
	case SortKeyType::SORT_KEY_TYPE:                                                                                   \
		result.function = TupleDataSortKeyScatter<T, SortKeyType::SORT_KEY_TYPE>;                                      \
		break;
		DUCKDB_FOR_EACH_SORT_KEY_TYPE(DUCKDB_SORT_KEY_CASE)
#undef DUCKDB_SORT_KEY_CASE
	default:
		throw NotImplementedException("GetSortKeyScatterFunction for %s", EnumUtil::ToString(sort_key_type));
	}
	return result;
}

TupleDataScatterFunction TupleDataCollection::GetSortKeyScatterFunction(const LogicalType &type,
                                                                        SortKeyType sort_key_type) {
	switch (type.id()) {
	case LogicalTypeId::BIGINT:
		return GetSortKeyScatterFunctionInternal<int64_t>(sort_key_type);
	case LogicalTypeId::BLOB:
		return GetSortKeyScatterFunctionInternal<string_t>(sort_key_type);
	default:
		throw NotImplementedException("TupleDataCollection::GetSortKeyScatterFunction for %s", type.ToString());
	}
}

//-------------------------------------------------------------------------------
// Gather
//-------------------------------------------------------------------------------

void TupleDataCollection::Gather(Vector &row_locations, const SelectionVector &scan_sel, const idx_t scan_count,
                                 DataChunk &result, const SelectionVector &target_sel,
                                 vector<unique_ptr<Vector>> &cached_cast_vectors) const {
	D_ASSERT(result.ColumnCount() == layout.ColumnCount());
	vector<column_t> column_ids;
	column_ids.reserve(layout.ColumnCount());
	for (idx_t col_idx = 0; col_idx < layout.ColumnCount(); col_idx++) {
		column_ids.emplace_back(col_idx);
	}
	Gather(row_locations, scan_sel, scan_count, column_ids, result, target_sel, cached_cast_vectors);
}

void TupleDataCollection::Gather(Vector &row_locations, const SelectionVector &scan_sel, const idx_t scan_count,
                                 const vector<column_t> &column_ids, DataChunk &result,
                                 const SelectionVector &target_sel,
                                 vector<unique_ptr<Vector>> &cached_cast_vectors) const {
	for (idx_t col_idx = 0; col_idx < column_ids.size(); col_idx++) {
		Gather(row_locations, scan_sel, scan_count, column_ids[col_idx], result.data[col_idx], target_sel,
		       cached_cast_vectors[col_idx].get());
	}
}

void TupleDataCollection::Gather(Vector &row_locations, const SelectionVector &scan_sel, const idx_t scan_count,
                                 const column_t column_id, Vector &result, const SelectionVector &target_sel,
                                 optional_ptr<Vector> cached_cast_vector) const {
	D_ASSERT(!cached_cast_vector ||
	         FlatVector::ValidityMutable(*cached_cast_vector).CannotHaveNull()); // ResetCachedCastVectors
	const auto &gather_function = gather_functions[column_id];
	gather_function.Gather(layout, row_locations, column_id, scan_sel, scan_count, result, target_sel,
	                       cached_cast_vector);
	result.Verify(target_sel, scan_count);
}

TupleDataGatherFunction::TupleDataGatherFunction(tuple_data_gather_function_t function_p) : function(function_p) {
}

TupleDataGatherFunction::TupleDataGatherFunction(tuple_data_gather_function_t function_p,
                                                 vector<TupleDataGatherFunction> child_functions_p)
    : function(function_p), child_functions(std::move(child_functions_p)) {
}

void TupleDataGatherFunction::Gather(const TupleDataLayout &layout, Vector &row_locations, const idx_t col_idx,
                                     const SelectionVector &scan_sel, const idx_t scan_count, Vector &target,
                                     const SelectionVector &target_sel, optional_ptr<Vector> list_vector) const {
	function(layout, row_locations, col_idx, scan_sel, scan_count, target, target_sel, list_vector, child_functions);
}

//------------------------------------------------------------------------------
// Gather kernels
//------------------------------------------------------------------------------

template <class T>
static void TupleDataTemplatedGather(const TupleDataLayout &layout, Vector &row_locations, const idx_t col_idx,
                                     const SelectionVector &scan_sel, const idx_t scan_count, Vector &target,
                                     const SelectionVector &target_sel, optional_ptr<Vector>,
                                     const vector<TupleDataGatherFunction> &) {
	ThrowOnTupleDataError(rust::gather_fixed(
	    RustAddresses(row_locations), RustSel(scan_sel), RustSel(target_sel), scan_count, RustTargetEntries(target, sizeof(T)),
	    RustTargetValidity(target), sizeof(T), layout.GetOffsets()[col_idx], col_idx, layout.CanHaveNull()));
}

static void TupleDataStringGather(const TupleDataLayout &layout, Vector &row_locations, const idx_t col_idx,
                                  const SelectionVector &scan_sel, const idx_t scan_count, Vector &target,
                                  const SelectionVector &target_sel, optional_ptr<Vector>,
                                  const vector<TupleDataGatherFunction> &) {
	ThrowOnTupleDataError(rust::gather_string(
	    RustAddresses(row_locations), RustSel(scan_sel), RustSel(target_sel), scan_count,
	    RustTargetEntries(target, sizeof(string_t)), RustTargetValidity(target), layout.GetOffsets()[col_idx], col_idx,
	    layout.CanHaveNull()));
}

template <class T, SortKeyType SORT_KEY_TYPE>
void TupleDataSortKeyGather(const TupleDataLayout &, Vector &, const idx_t, const SelectionVector &, const idx_t,
                            Vector &, const SelectionVector &, optional_ptr<Vector>,
                            const vector<TupleDataGatherFunction> &) {
	throw NotImplementedException("Unimplemented type for TupleDataSortKeyGather");
}

template <class T>
static void TupleDataTemplatedWithinCollectionGather(const TupleDataLayout &, Vector &heap_locations,
                                                     const idx_t list_size_before, const SelectionVector &scan_sel,
                                                     const idx_t scan_count, Vector &target,
                                                     const SelectionVector &target_sel, optional_ptr<Vector> list_vector,
                                                     const vector<TupleDataGatherFunction> &) {
	auto &list_vector_ref = *list_vector;
	ThrowOnTupleDataError(rust::gather_within_fixed(
	    RustAddresses(heap_locations), scan_count, RustSel(target_sel), RustTargetEntries(target, sizeof(T)),
	    RustTargetValidity(target), RustTargetEntries(list_vector_ref, sizeof(list_entry_t)),
	    RustValidity(FlatVector::Validity(list_vector_ref)), sizeof(T), list_size_before));
}

static void TupleDataStringWithinCollectionGather(const TupleDataLayout &, Vector &heap_locations,
                                                  const idx_t list_size_before, const SelectionVector &scan_sel,
                                                  const idx_t scan_count, Vector &target,
                                                  const SelectionVector &target_sel, optional_ptr<Vector> list_vector,
                                                  const vector<TupleDataGatherFunction> &) {
	auto &list_vector_ref = *list_vector;
	ThrowOnTupleDataError(rust::gather_within_string(
	    RustAddresses(heap_locations), scan_count, RustSel(target_sel), RustTargetEntries(target, sizeof(string_t)),
	    RustTargetValidity(target), RustTargetEntries(list_vector_ref, sizeof(list_entry_t)),
	    RustValidity(FlatVector::Validity(list_vector_ref)), list_size_before));
}

static void TupleDataStructGather(const TupleDataLayout &layout, Vector &row_locations, const idx_t col_idx,
                                  const SelectionVector &scan_sel, const idx_t scan_count, Vector &target,
                                  const SelectionVector &target_sel, optional_ptr<Vector> dummy_vector,
                                  const vector<TupleDataGatherFunction> &child_functions) {
	// Create a Vector of pointers to the start of the TupleDataLayout of the STRUCT
	Vector struct_row_locations(LogicalType::POINTER);
	ThrowOnTupleDataError(rust::gather_struct(
	    RustAddresses(row_locations), RustSel(scan_sel), RustSel(target_sel), scan_count, RustTargetValidity(target),
	    layout.GetOffsets()[col_idx], col_idx, layout.CanHaveNull(), RustAddressesMut(struct_row_locations)));

	// Get the struct layout and struct entries
	const auto &struct_layout = layout.GetStructLayout(col_idx);
	auto &struct_targets = StructVector::GetEntries(target);
	if (!struct_layout) {
		// Empty struct: no fields to gather
		D_ASSERT(struct_targets.empty());
		return;
	}
	D_ASSERT(struct_layout->ColumnCount() == struct_targets.size());

	// Recurse through the struct children
	for (idx_t struct_col_idx = 0; struct_col_idx < struct_layout->ColumnCount(); struct_col_idx++) {
		auto &struct_target = struct_targets[struct_col_idx];
		const auto &struct_gather_function = child_functions[struct_col_idx];
		struct_gather_function.Gather(*struct_layout, struct_row_locations, struct_col_idx, scan_sel, scan_count,
		                              struct_target, target_sel, dummy_vector);
	}
}

static void TupleDataListGather(const TupleDataLayout &layout, Vector &row_locations, const idx_t col_idx,
                                const SelectionVector &scan_sel, const idx_t scan_count, Vector &target,
                                const SelectionVector &target_sel, optional_ptr<Vector>,
                                const vector<TupleDataGatherFunction> &child_functions) {
	// Load pointers to the data from the row
	Vector heap_locations(LogicalType::POINTER);

	auto list_size_before = ListVector::GetListSize(target);
	auto target_list_offset = ThrowOnTupleDataError(rust::gather_collection(
	    RustAddresses(row_locations), RustSel(scan_sel), RustSel(target_sel), scan_count,
	    RustTargetEntries(target, sizeof(list_entry_t)), RustTargetValidity(target), RustAddressesMut(heap_locations),
	    layout.GetOffsets()[col_idx], col_idx, layout.CanHaveNull(), list_size_before));
	ListVector::Reserve(target, target_list_offset);
	ListVector::SetListSize(target, target_list_offset);

	// Recurse
	D_ASSERT(child_functions.size() == 1);
	const auto &child_function = child_functions[0];
	child_function.Gather(layout, heap_locations, list_size_before, scan_sel, scan_count,
	                      ListVector::GetChildMutable(target), target_sel, &target);
}

static void TupleDataStructWithinCollectionGather(const TupleDataLayout &layout, Vector &heap_locations,
                                                  const idx_t list_size_before, const SelectionVector &scan_sel,
                                                  const idx_t scan_count, Vector &target,
                                                  const SelectionVector &target_sel, optional_ptr<Vector> list_vector,
                                                  const vector<TupleDataGatherFunction> &child_functions) {
	auto &list_vector_ref = *list_vector;
	ThrowOnTupleDataError(rust::gather_within_struct(
	    RustAddresses(heap_locations), scan_count, RustSel(target_sel), RustTargetValidity(target),
	    RustTargetEntries(list_vector_ref, sizeof(list_entry_t)), RustValidity(FlatVector::Validity(list_vector_ref)),
	    list_size_before));

	// Recurse
	auto &struct_targets = StructVector::GetEntries(target);
	for (idx_t struct_col_idx = 0; struct_col_idx < struct_targets.size(); struct_col_idx++) {
		auto &struct_target = struct_targets[struct_col_idx];
		const auto &struct_gather_function = child_functions[struct_col_idx];
		struct_gather_function.Gather(layout, heap_locations, list_size_before, scan_sel, scan_count, struct_target,
		                              target_sel, list_vector);
	}
}

static void TupleDataCollectionWithinCollectionGather(const TupleDataLayout &layout, Vector &heap_locations,
                                                      const idx_t list_size_before, const SelectionVector &scan_sel,
                                                      const idx_t scan_count, Vector &target,
                                                      const SelectionVector &target_sel,
                                                      optional_ptr<Vector> list_vector,
                                                      const vector<TupleDataGatherFunction> &child_functions) {
	auto &list_vector_ref = *list_vector;
	const auto &list_validity = FlatVector::Validity(list_vector_ref);
	const auto child_list_size_before = ListVector::GetListSize(target);

	// We need to create a vector that has the combined list sizes (hugeint_t has same size as list_entry_t)
	Vector combined_list_vector(LogicalType::HUGEINT);
	FlatVector::SetValidity(combined_list_vector, list_validity); // Has same validity as list parent

	auto child_list_size = ThrowOnTupleDataError(rust::gather_within_collection(
	    RustAddresses(heap_locations), scan_count, RustSel(target_sel), RustTargetEntries(target, sizeof(list_entry_t)),
	    RustTargetEntries(combined_list_vector, sizeof(list_entry_t)), RustTargetValidity(target),
	    RustTargetEntries(list_vector_ref, sizeof(list_entry_t)), RustValidity(list_validity), list_size_before,
	    child_list_size_before));

	ListVector::Reserve(target, child_list_size);
	ListVector::SetListSize(target, child_list_size);

	// Recurse
	D_ASSERT(child_functions.size() == 1);
	const auto &child_function = child_functions[0];
	child_function.Gather(layout, heap_locations, child_list_size_before, scan_sel, scan_count,
	                      ListVector::GetChildMutable(target), target_sel, &combined_list_vector);
}

//------------------------------------------------------------------------------
// Special cases for arrays
//------------------------------------------------------------------------------
// A gather function that wraps another gather function and casts the result to the target array type
static void TupleDataCastToArrayListGather(const TupleDataLayout &layout, Vector &row_locations, const idx_t col_idx,
                                           const SelectionVector &scan_sel, const idx_t scan_count, Vector &target,
                                           const SelectionVector &target_sel, optional_ptr<Vector> cached_cast_vector,
                                           const vector<TupleDataGatherFunction> &child_functions) {
	if (cached_cast_vector) {
		// Reuse the cached cast vector
		TupleDataListGather(layout, row_locations, col_idx, scan_sel, scan_count, *cached_cast_vector, target_sel,
		                    cached_cast_vector, child_functions);
		VectorOperations::DefaultCast(*cached_cast_vector, target, scan_count);
	} else {
		// Otherwise, create a new temporary cast vector
		Vector cast_vector(ArrayType::ConvertToList(target.GetType()));
		TupleDataListGather(layout, row_locations, col_idx, scan_sel, scan_count, cast_vector, target_sel, &cast_vector,
		                    child_functions);
		VectorOperations::DefaultCast(cast_vector, target, scan_count);
	}
}

static void TupleDataCastToArrayStructGather(const TupleDataLayout &layout, Vector &row_locations, const idx_t col_idx,
                                             const SelectionVector &scan_sel, const idx_t scan_count, Vector &target,
                                             const SelectionVector &target_sel, optional_ptr<Vector> cached_cast_vector,
                                             const vector<TupleDataGatherFunction> &child_functions) {
	if (cached_cast_vector) {
		// Reuse the cached cast vector
		TupleDataStructGather(layout, row_locations, col_idx, scan_sel, scan_count, *cached_cast_vector, target_sel,
		                      cached_cast_vector, child_functions);
		VectorOperations::DefaultCast(*cached_cast_vector, target, scan_count);
	} else {
		// Otherwise, create a new temporary cast vector
		Vector cast_vector(ArrayType::ConvertToList(target.GetType()));
		TupleDataStructGather(layout, row_locations, col_idx, scan_sel, scan_count, cast_vector, target_sel,
		                      &cast_vector, child_functions);
		VectorOperations::DefaultCast(cast_vector, target, scan_count);
	}
}

//------------------------------------------------------------------------------
// Get Gather Function
//------------------------------------------------------------------------------

template <class T>
tuple_data_gather_function_t TupleDataGetGatherFunction(bool within_collection) {
	return within_collection ? TupleDataTemplatedWithinCollectionGather<T> : TupleDataTemplatedGather<T>;
}

static TupleDataGatherFunction TupleDataGetGatherFunctionInternal(const LogicalType &type, bool within_collection) {
	tuple_data_gather_function_t function = nullptr;
	vector<TupleDataGatherFunction> child_functions;
	if (type.InternalType() == PhysicalType::VARCHAR) {
		function = within_collection ? TupleDataStringWithinCollectionGather : TupleDataStringGather;
		return TupleDataGatherFunction(function);
	}
	switch (type.InternalType()) {
	case PhysicalType::BOOL:
		function = TupleDataGetGatherFunction<bool>(within_collection);
		break;
	case PhysicalType::INT8:
		function = TupleDataGetGatherFunction<int8_t>(within_collection);
		break;
	case PhysicalType::INT16:
		function = TupleDataGetGatherFunction<int16_t>(within_collection);
		break;
	case PhysicalType::INT32:
		function = TupleDataGetGatherFunction<int32_t>(within_collection);
		break;
	case PhysicalType::INT64:
		function = TupleDataGetGatherFunction<int64_t>(within_collection);
		break;
	case PhysicalType::INT128:
		function = TupleDataGetGatherFunction<hugeint_t>(within_collection);
		break;
	case PhysicalType::UINT8:
		function = TupleDataGetGatherFunction<uint8_t>(within_collection);
		break;
	case PhysicalType::UINT16:
		function = TupleDataGetGatherFunction<uint16_t>(within_collection);
		break;
	case PhysicalType::UINT32:
		function = TupleDataGetGatherFunction<uint32_t>(within_collection);
		break;
	case PhysicalType::UINT64:
		function = TupleDataGetGatherFunction<uint64_t>(within_collection);
		break;
	case PhysicalType::UINT128:
		function = TupleDataGetGatherFunction<uhugeint_t>(within_collection);
		break;
	case PhysicalType::FLOAT:
		function = TupleDataGetGatherFunction<float>(within_collection);
		break;
	case PhysicalType::DOUBLE:
		function = TupleDataGetGatherFunction<double>(within_collection);
		break;
	case PhysicalType::INTERVAL:
		function = TupleDataGetGatherFunction<interval_t>(within_collection);
		break;
	case PhysicalType::STRUCT: {
		function = within_collection ? TupleDataStructWithinCollectionGather : TupleDataStructGather;
		for (const auto &child_type : StructType::GetChildTypes(type)) {
			child_functions.push_back(TupleDataGetGatherFunctionInternal(child_type.second, within_collection));
		}
		break;
	}
	case PhysicalType::LIST:
		function = within_collection ? TupleDataCollectionWithinCollectionGather : TupleDataListGather;
		child_functions.push_back(TupleDataGetGatherFunctionInternal(ListType::GetChildType(type), true));
		break;
	case PhysicalType::ARRAY:
		function = within_collection ? TupleDataCollectionWithinCollectionGather : TupleDataListGather;
		child_functions.push_back(TupleDataGetGatherFunctionInternal(ArrayType::GetChildType(type), true));
		break;
	default:
		throw InternalException("Unsupported type for TupleDataCollection::GetGatherFunction");
	}
	return TupleDataGatherFunction(function, std::move(child_functions));
}

TupleDataGatherFunction TupleDataCollection::GetGatherFunction(const LogicalType &type) {
	if (!type.IsNested()) {
		return TupleDataGetGatherFunctionInternal(type, false);
	}

	if (TypeVisitor::Contains(type, LogicalTypeId::ARRAY)) {
		// Special case: we cant handle arrays yet, so we need to replace them with lists when gathering
		const auto new_type = ArrayType::ConvertToList(type);

		tuple_data_gather_function_t function = nullptr;
		vector<TupleDataGatherFunction> child_functions;
		// Theres only two cases: Either the array is within a struct, or it is within a list (or has now become a list)
		switch (new_type.InternalType()) {
		case PhysicalType::LIST:
			function = TupleDataCastToArrayListGather;
			child_functions.push_back(TupleDataGetGatherFunctionInternal(ListType::GetChildType(new_type), true));
			break;
		case PhysicalType::STRUCT:
			function = TupleDataCastToArrayStructGather;
			for (const auto &child_type : StructType::GetChildTypes(new_type)) {
				child_functions.push_back(TupleDataGetGatherFunctionInternal(child_type.second, false));
			}
			break;
		default:
			throw InternalException("Unsupported type for TupleDataCollection::GetGatherFunction");
		}
		return TupleDataGatherFunction(function, std::move(child_functions));
	}
	return TupleDataGetGatherFunctionInternal(type, false);
}

template <class T>
TupleDataGatherFunction GetSortKeyGatherFunctionInternal(SortKeyType sort_key_type) {
	tuple_data_gather_function_t function = nullptr;
	switch (sort_key_type) {
#define DUCKDB_SORT_KEY_CASE(SORT_KEY_TYPE)                                                                            \
	case SortKeyType::SORT_KEY_TYPE:                                                                                   \
		function = TupleDataSortKeyGather<T, SortKeyType::SORT_KEY_TYPE>;                                              \
		break;
		DUCKDB_FOR_EACH_SORT_KEY_TYPE(DUCKDB_SORT_KEY_CASE)
#undef DUCKDB_SORT_KEY_CASE
	default:
		throw NotImplementedException("GetSortKeyGatherFunction for %s", EnumUtil::ToString(sort_key_type));
	}
	return TupleDataGatherFunction(function);
}

TupleDataGatherFunction TupleDataCollection::GetSortKeyGatherFunction(const LogicalType &type,
                                                                      SortKeyType sort_key_type) {
	switch (type.id()) {
	case LogicalTypeId::BIGINT:
		return GetSortKeyGatherFunctionInternal<int64_t>(sort_key_type);
	case LogicalTypeId::BLOB:
		return GetSortKeyGatherFunctionInternal<string_t>(sort_key_type);
	default:
		throw NotImplementedException("TupleDataCollection::GetSortKeyGatherFunction for %s", type.ToString());
	}
}

} // namespace duckdb
