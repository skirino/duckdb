#include "duckdb/common/types/row/tuple_data_layout.hpp"

#include "tuple_data_layout_rust_bridge.hpp"

namespace duckdb {

TupleDataLayout::TupleDataLayout()
    : sort_key_type(SortKeyType::INVALID), flag_width(0), data_width(0), aggr_width(0), row_width(0),
      all_constant(true), heap_size_offset(0), validity_type(TupleDataValidityType::CAN_HAVE_NULL_VALUES) {
}

TupleDataLayout TupleDataLayout::Copy() const {
	TupleDataLayout result;
	result.types = this->types;
	result.aggregates = this->aggregates;
	result.sort_key_type = this->sort_key_type;
	result.flag_width = this->flag_width;
	result.data_width = this->data_width;
	result.aggr_width = this->aggr_width;
	result.sort_width = this->sort_width;
	result.sort_skippable_bytes = this->sort_skippable_bytes;
	result.row_width = this->row_width;
	result.offsets = this->offsets;
	result.all_constant = this->all_constant;
	result.heap_size_offset = this->heap_size_offset;
	result.aggr_destructor_idxs = this->aggr_destructor_idxs;
	result.validity_type = this->validity_type;
	// Copy struct layouts (they are lazily computed, so we need to copy the cached layouts)
	result.struct_layouts.resize(struct_layouts.size());
	for (idx_t i = 0; i < struct_layouts.size(); i++) {
		if (struct_layouts[i]) {
			result.struct_layouts[i] = make_shared_ptr<TupleDataLayout>(struct_layouts[i]->Copy());
		}
	}
	return result;
}

void TupleDataLayout::Initialize(vector<LogicalType> types_p, Aggregates aggregates_p,
                                 TupleDataValidityType validity_type_p, TupleDataNestednessType nestedness_type) {
	sort_key_type = SortKeyType::INVALID;
	offsets.clear();
	aggr_destructor_idxs.clear();
	types = std::move(types_p);
	all_constant = true;
	sort_skippable_bytes.clear();
	variable_columns.clear();

	validity_type = validity_type_p;

	// Resize struct_layouts to match number of columns (for vector-based access)
	struct_layouts.resize(types.size());

	// Pre-compute all struct layouts recursively before calling Rust bridge
	// This ensures struct_size values are available for compute_layout()
	for (idx_t col_idx = 0; col_idx < types.size(); col_idx++) {
		if (types[col_idx].InternalType() == PhysicalType::STRUCT) {
			GetStructLayout(col_idx);
		}
	}

	// Aggregates.
	std::vector<idx_t> aggr_payload_sizes(aggregates_p.size());
	std::vector<uint8_t> aggr_has_destructor(aggregates_p.size());
	for (idx_t i = 0; i < aggregates_p.size(); i++) {
		aggr_payload_sizes[i] = aggregates_p[i].payload_size;
		aggr_has_destructor[i] = aggregates_p[i].function.HasStateDestructorCallback() ? 1 : 0;
	}

	bool is_top_level = (nestedness_type == TupleDataNestednessType::TOP_LEVEL_LAYOUT);

	idx_t total_columns = types.size() + aggregates_p.size();
	std::vector<idx_t> offsets_buf(total_columns);
	std::vector<idx_t> variable_columns_buf(types.size());
	std::vector<idx_t> destructor_idxs_buf(aggregates_p.size());

	// Validate buffer sizes before calling Rust bridge
	D_ASSERT(offsets_buf.size() >= total_columns);
	D_ASSERT(variable_columns_buf.size() >= types.size());
	D_ASSERT(destructor_idxs_buf.size() >= aggregates_p.size());

	// Ensure buffers are properly initialized - empty vectors have valid data() but values may be uninitialized
	// Size is already set above; values will be overwritten by Rust compute_layout
	(void)offsets_buf.data();
	(void)variable_columns_buf.data();
	(void)destructor_idxs_buf.data();

	std::vector<rust::ColumnDesc> columns(types.size());
	for (idx_t col_idx = 0; col_idx < types.size(); col_idx++) {
		const auto &type = types[col_idx];
		bool is_struct = (type.InternalType() == PhysicalType::STRUCT);
		bool is_const = true;
		if (is_struct) {
			// struct_size was computed by GetStructLayout() pre-compute loop above
			auto struct_layout = GetStructLayout(col_idx);
			columns[col_idx].struct_size = struct_layout ? struct_layout->GetRowWidth() : 0;
			is_const = struct_layout && struct_layout->AllConstant();
		} else if (!TypeIsConstantSize(type.InternalType())) {
			is_const = false;
			columns[col_idx].struct_size = 0;
		} else {
			columns[col_idx].struct_size = 0;
		}
		columns[col_idx].physical_type = static_cast<uint8_t>(type.InternalType());
		columns[col_idx].is_struct = is_struct ? 1 : 0;
		// Note: is_constant=1 means constant size, is_constant=0 means variable
		// This naming is inverted from the Rust side where is_constant==0 means variable
		columns[col_idx].is_constant = is_const ? 1 : 0;
	}

	rust::LayoutResult result = rust::compute_layout(
	    rs_std::SliceRef<const rust::ColumnDesc>(columns.data(), columns.size()),
	    rs_std::SliceRef<const uintptr_t>(reinterpret_cast<const uintptr_t *>(aggr_payload_sizes.data()), aggr_payload_sizes.size()),
	    rs_std::SliceRef<const uint8_t>(aggr_has_destructor.data(), aggr_has_destructor.size()),
	    validity_type == TupleDataValidityType::CANNOT_HAVE_NULL_VALUES ? 1 : 0,
	    is_top_level ? 1 : 0,
	    rs_std::SliceRef<uintptr_t>(reinterpret_cast<uintptr_t *>(offsets_buf.data()), offsets_buf.size()),
	    rs_std::SliceRef<uintptr_t>(reinterpret_cast<uintptr_t *>(variable_columns_buf.data()), variable_columns_buf.size()),
	    rs_std::SliceRef<uintptr_t>(reinterpret_cast<uintptr_t *>(destructor_idxs_buf.data()), destructor_idxs_buf.size()));

	flag_width = result.flag_width;
	row_width = result.row_width;
	data_width = result.data_width;
	aggr_width = result.aggr_width;
	heap_size_offset = result.heap_size_offset;
	all_constant = (result.all_constant != 0);

	offsets.assign(offsets_buf.begin(), offsets_buf.begin() + result.num_offsets);
	variable_columns.assign(variable_columns_buf.begin(), variable_columns_buf.begin() + result.num_variable_columns);
	aggr_destructor_idxs.assign(destructor_idxs_buf.begin(), destructor_idxs_buf.begin() + result.num_aggr_destructor_idxs);

	aggregates = std::move(aggregates_p);
}

void TupleDataLayout::Initialize(vector<LogicalType> types_p, TupleDataValidityType validity_type,
                                 TupleDataNestednessType nestedness_type) {
	Initialize(std::move(types_p), Aggregates(), validity_type, nestedness_type);
}

void TupleDataLayout::Initialize(Aggregates aggregates_p) {
	Initialize(vector<LogicalType>(), std::move(aggregates_p), TupleDataValidityType::CANNOT_HAVE_NULL_VALUES,
	           TupleDataNestednessType::TOP_LEVEL_LAYOUT);
}

void TupleDataLayout::Initialize(const vector<BoundOrderByNode> &orders, const LogicalType &type, bool has_payload) {
	// Reset state
	types.clear();
	aggregates.clear();
	flag_width = DConstants::INVALID_INDEX;
	data_width = DConstants::INVALID_INDEX;
	aggr_width = DConstants::INVALID_INDEX;
	offsets.clear();
	sort_skippable_bytes.clear();
	all_constant = true;
	heap_size_offset = DConstants::INVALID_INDEX;
	aggr_destructor_idxs.clear();
	validity_type = TupleDataValidityType::CAN_HAVE_NULL_VALUES;

	// Type is determined by "create_sort_key", if it is <= 8 we get a bigint
	types.push_back(type);

	// Compute row width
	sort_width = 0;
	bool all_valid_and_truly_constant = true;
	for (idx_t order_idx = 0; order_idx < orders.size(); order_idx++) {
		const auto &order = orders[order_idx];
		const auto &logical_type = order.expression->GetReturnType();
		const auto physical_type = logical_type.InternalType();

		if (all_valid_and_truly_constant && order.stats && !order.stats->CanHaveNull() &&
		    TypeIsConstantSize(physical_type) && sort_width < 7) {
			// We don't have to sort by this byte, all values are valid
			sort_skippable_bytes.emplace_back(sort_width);
		} else {
			all_valid_and_truly_constant = false;
		}

		if (TypeIsConstantSize(physical_type)) {
			// NULL byte + fixed-width type
			sort_width += 1 + GetTypeIdSize(physical_type);
		} else if (logical_type == LogicalType::VARCHAR && order.stats &&
		           StringStats::HasMaxStringLength(*order.stats)) {
			// NULL byte + maximum string length + string delimiter
			sort_width += 1 + StringStats::MaxStringLength(*order.stats) + 1;
		} else {
			// We don't know how long the key will be
			sort_width = DConstants::INVALID_INDEX;
			break;
		}
	}

	// Set row width and sort key type accordingly
	idx_t temp_row_width = type.id() == LogicalTypeId::BIGINT ? 8 : sort_width;
	if (sort_width != DConstants::INVALID_INDEX && has_payload) {
		temp_row_width += 8;
	}
#if DUCKDB_SMALLER_BINARY(sort_key_layouts)
	// Round the key up to the nearest layout that DUCKDB_FOR_EACH_SORT_KEY_TYPE still keeps. A key
	// without payload widens to the 32-byte one rather than falling back to the variable-size key,
	// which would cost far more than the wider row: a variable-size key compares through the heap.
	if (has_payload) {
		if (temp_row_width > 24) {
			temp_row_width = DConstants::INVALID_INDEX;
		}
	} else if (temp_row_width > 8 && temp_row_width <= 32) {
		temp_row_width = 32;
	}
#endif
	if (temp_row_width <= 8) {
		D_ASSERT(!has_payload);
		row_width = 8;
		sort_key_type = SortKeyType::NO_PAYLOAD_FIXED_8;
	} else if (temp_row_width <= 16) {
		row_width = 16;
		sort_key_type = has_payload ? SortKeyType::PAYLOAD_FIXED_16 : SortKeyType::NO_PAYLOAD_FIXED_16;
	} else if (temp_row_width <= 24) {
		row_width = 24;
		sort_key_type = has_payload ? SortKeyType::PAYLOAD_FIXED_24 : SortKeyType::NO_PAYLOAD_FIXED_24;
	} else if (temp_row_width <= 32) {
		row_width = 32;
		sort_key_type = has_payload ? SortKeyType::PAYLOAD_FIXED_32 : SortKeyType::NO_PAYLOAD_FIXED_32;
	} else {
		row_width = 32;
		sort_key_type = has_payload ? SortKeyType::PAYLOAD_VARIABLE_32 : SortKeyType::NO_PAYLOAD_VARIABLE_32;

		// Variable-size sort key, also set these properties
		all_constant = false;
		heap_size_offset = has_payload ? SortKey<SortKeyType::PAYLOAD_VARIABLE_32>::HEAP_SIZE_OFFSET
		                               : SortKey<SortKeyType::NO_PAYLOAD_VARIABLE_32>::HEAP_SIZE_OFFSET;
	}
}

bool TupleDataLayout::IsSortKeyLayout() const {
	return sort_key_type != SortKeyType::INVALID;
}

bool TupleDataLayout::HasNestedTypes() const {
	for (const auto &type : types) {
		if (type.IsNested()) {
			return true;
		}
	}
	return false;
}

const TupleDataLayout *TupleDataLayout::GetStructLayout(idx_t col_idx) const {
	if (col_idx >= types.size()) {
		return nullptr;
	}
	if (types[col_idx].InternalType() != PhysicalType::STRUCT) {
		return nullptr;
	}
	// Check if struct layout is already computed
	if (col_idx < struct_layouts.size() && struct_layouts[col_idx]) {
		return struct_layouts[col_idx].get();
	}
	// lazily build struct layout
	auto child_types = StructType::GetChildTypes(types[col_idx]);
	if (child_types.empty()) {
		return nullptr;
	}
	// Create nested struct layout
	auto nested_layout = make_shared_ptr<TupleDataLayout>();
	vector<LogicalType> child_type_list;
	child_type_list.reserve(child_types.size());
	for (const auto &child : child_types) {
		child_type_list.push_back(child.second);
	}
	// Struct fields can always be NULL, regardless of the parent's validity
	nested_layout->Initialize(child_type_list, TupleDataValidityType::CAN_HAVE_NULL_VALUES,
	                          TupleDataNestednessType::NESTED_STRUCT_LAYOUT);
	struct_layouts[col_idx] = nested_layout;
	return nested_layout.get();
}

} // namespace duckdb
