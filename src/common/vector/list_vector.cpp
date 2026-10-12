#include "duckdb/common/vector/list_vector.hpp"
#include "duckdb/common/vector_operations/vector_operations.hpp"
#include "vector_rust_bridge.hpp"

namespace duckdb {

VectorListBuffer::VectorListBuffer(Allocator &allocator, capacity_t capacity, unique_ptr<Vector> vector)
    : StandardVectorBuffer(allocator, capacity, sizeof(list_entry_t)), child(std::move(vector)) {
	buffer_type = VectorBufferType::LIST_BUFFER;
	FlatVector::SetSize(*child, 0ULL);
}
VectorListBuffer::VectorListBuffer(Allocator &allocator, capacity_t capacity, const LogicalType &list_type,
                                   capacity_t child_capacity)
    : VectorListBuffer(allocator, capacity, make_uniq<Vector>(ListType::GetChildType(list_type), child_capacity)) {
}

VectorListBuffer::VectorListBuffer(capacity_t capacity, const LogicalType &list_type, capacity_t child_capacity)
    : VectorListBuffer(Allocator::DefaultAllocator(), capacity, list_type, child_capacity) {
}

VectorListBuffer::VectorListBuffer(data_ptr_t data, count_t count, const Vector &vector)
    : StandardVectorBuffer(data, count, sizeof(list_entry_t)) {
	buffer_type = VectorBufferType::LIST_BUFFER;
	child = make_uniq<Vector>(Vector::Ref(vector));
}

VectorListBuffer::VectorListBuffer(data_ptr_t data, count_t count, const VectorListBuffer &parent)
    : StandardVectorBuffer(data, count, sizeof(list_entry_t)) {
	buffer_type = VectorBufferType::LIST_BUFFER;
	child = make_uniq<Vector>(Vector::Ref(parent.GetChild()));
}

VectorListBuffer::VectorListBuffer(AllocatedData allocated_data_p, count_t count, const VectorListBuffer &parent)
    : StandardVectorBuffer(std::move(allocated_data_p), count, sizeof(list_entry_t)) {
	buffer_type = VectorBufferType::LIST_BUFFER;
	child = make_uniq<Vector>(Vector::Ref(parent.GetChild()));
}

VectorListBuffer::VectorListBuffer(AllocatedData allocated_data_p, count_t count, VectorListBuffer &parent)
    : StandardVectorBuffer(std::move(allocated_data_p), count, sizeof(list_entry_t)) {
	buffer_type = VectorBufferType::LIST_BUFFER;
	auto &parent_child = parent.GetChildMutable();
	child = std::move(parent_child);
	parent_child = make_uniq<Vector>(Vector::Ref(*child));
}

void VectorListBuffer::Reserve(idx_t to_reserve) {
	child->Reserve(to_reserve);
}

void VectorListBuffer::AppendToChild(const Vector &to_append, idx_t to_append_size) {
	child->Append(to_append, to_append_size, VectorAppendMode::ALLOW_RESIZE);
}

void VectorListBuffer::AppendToChild(const Vector &to_append, const SelectionVector &sel, idx_t to_append_size) {
	child->Append(to_append, sel, to_append_size, VectorAppendMode::ALLOW_RESIZE);
}

void VectorListBuffer::PushBack(const Value &insert) {
	child->Append(insert, VectorAppendMode::ALLOW_RESIZE);
}

idx_t VectorListBuffer::GetChildCapacity() const {
	return FlatVector::GetCapacity(*child);
}

void VectorListBuffer::SetChildSize(idx_t new_size) {
	FlatVector::SetSize(*child, new_size);
}

VectorListBuffer::~VectorListBuffer() {
}

idx_t VectorListBuffer::GetDataSize(const LogicalType &type, idx_t count) const {
	idx_t size = StandardVectorBuffer::GetDataSize(type, count);
	size += child->GetDataSize(child->size());
	return size;
}

idx_t VectorListBuffer::GetAllocationSize() const {
	idx_t size = StandardVectorBuffer::GetAllocationSize();
	size += GetChild().GetAllocationSize();
	return size;
}

void VectorListBuffer::VerifyInternal(const LogicalType &type, const SelectionVector &sel, idx_t count) const {
	StandardVectorBuffer::VerifyInternal(type, sel, count);

	D_ASSERT(type.InternalType() == PhysicalType::LIST);
	if (type.id() == LogicalTypeId::MAP) {
		// FIXME: verify map
		// auto &child = ListType::GetChildType(vector_p.GetType());
		// D_ASSERT(StructType::GetChildCount(child) == 2);
		// D_ASSERT(StructType::GetChildName(child, 0) == "key");
		// D_ASSERT(StructType::GetChildName(child, 1) == "value");
		//
		// auto valid_check = MapVector::CheckMapValidity(vector_p, count, sel_p);
		// D_ASSERT(valid_check == MapInvalidReason::VALID);
	}

	// NOTE: size > capacity can occur in valid intermediate states (e.g. after SetListSize before Reserve)
	// D_ASSERT(size <= capacity);
	idx_t total_size = 0;
	auto list_data = reinterpret_cast<list_entry_t *>(data_ptr);
	for (idx_t i = 0; i < count; i++) {
		auto idx = sel.get_index(i);
		idx = vector_type == VectorType::CONSTANT_VECTOR ? 0 : idx;
		auto &le = list_data[idx];
		if (validity.RowIsValid(idx)) {
			if (le.offset + le.length > child->size()) {
				throw InternalException("List entry offset + length out of range (offset %d, size %d, child size %d)",
				                        le.offset, le.length, child->size());
			}
			total_size += le.length;
		}
	}
	if (!sel.IsSet() && count == Size()) {
		child->Verify();
		return;
	}
	SelectionVector child_sel(total_size);
	idx_t child_count = 0;
	for (idx_t i = 0; i < count; i++) {
		auto idx = sel.get_index(i);
		idx = vector_type == VectorType::CONSTANT_VECTOR ? 0 : idx;
		auto &le = list_data[idx];
		if (validity.RowIsValid(idx)) {
			D_ASSERT(le.offset + le.length <= child->size());
			for (idx_t k = 0; k < le.length; k++) {
				child_sel.set_index(child_count++, le.offset + k);
			}
		}
	}
	child->Verify(child_sel, child_count);
}

buffer_ptr<VectorBuffer> VectorListBuffer::SliceInternal(const LogicalType &type, idx_t offset, idx_t end) {
	auto type_size = GetTypeIdSize(type.InternalType());
	auto offset_ptr = data_ptr + type_size * offset;
	auto count = count_t(end - offset);
	auto result = make_buffer<VectorListBuffer>(offset_ptr, count, *this);
	result->GetValidityMask().Slice(validity, offset, count);
	result->AddAuxiliaryData(make_uniq<VectorBufferHolder>(shared_from_this()));
	return result;
}

buffer_ptr<VectorBuffer> VectorListBuffer::ConstantSliceInternal(const LogicalType &type, count_t count) {
	auto result = make_buffer<VectorListBuffer>(data_ptr, count, *child);
	result->GetValidityMask().Set(0, validity.RowIsValid(0));
	result->SetVectorType(VectorType::CONSTANT_VECTOR);
	result->AddAuxiliaryData(make_uniq<VectorBufferHolder>(shared_from_this()));
	return result;
}

void VectorListBuffer::ToUnifiedFormat(UnifiedVectorFormat &format) const {
	if (vector_type == VectorType::CONSTANT_VECTOR) {
		format.sel = ConstantVector::ZeroSelectionVector(Size(), format.owned_sel);
	} else {
		format.sel = FlatVector::IncrementalSelectionVector();
	}
	format.data = data_ptr;
	format.validity = validity;
}

buffer_ptr<VectorBuffer> VectorListBuffer::CreateBuffer(AllocatedData &&new_data, count_t count) const {
	return make_buffer<VectorListBuffer>(std::move(new_data), count, *this);
}

void VectorListBuffer::CopyInternal(const Vector &source, const SelectionVector &source_sel, idx_t source_count,
                                    idx_t source_offset, idx_t target_offset, idx_t copy_count) {
	D_ASSERT(source.GetType().InternalType() == PhysicalType::LIST);

	auto &source_child = ListVector::GetChild(source);
	auto tdata = reinterpret_cast<list_entry_t *>(data_ptr);

	//! we need to append the child elements to the target
	auto source_entries = RustEntryBuffer(source, sizeof(list_entry_t));
	auto target_entries = RustBytes(reinterpret_cast<data_ptr_t>(tdata), capacity * sizeof(list_entry_t));
	// Unlike a flat copy, this one is not a no-op onto itself — it re-points the
	// target entries and appends the child rows — so there is nothing to
	// short-circuit, and a copy that shares its rows with its source is refused.
	RustRejectOverlappingCopy(target_entries, source_entries);
	//! re-pointing the target entries and gathering the child rows to append is
	//! the Rust kernel's job; the append itself needs the child vector, so it
	//! stays here
	rust::CopyRange range {source_offset, target_offset, copy_count};
	auto child_rows = ThrowOnVectorError(
	    rust::copy_list_entries(source_entries, RustSel(source_sel, source_offset + copy_count), target_entries,
	                            RustValidity(validity, target_offset + copy_count), range, child->size()));
	if (child_rows.empty()) {
		// nothing to copy
		return;
	}
	// now append the child elements
	SelectionVector child_sel(child_rows.data(), child_rows.size());
	AppendToChild(source_child, child_sel, child_rows.size());
}

buffer_ptr<VectorBuffer> VectorListBuffer::Flatten(const LogicalType &type) const {
	if (vector_type == VectorType::FLAT_VECTOR) {
		// already flat - flatten the child
		child->Flatten();
		return nullptr;
	}
	return FlattenSlice(type, *FlatVector::IncrementalSelectionVector(), Size());
}

buffer_ptr<VectorBuffer> VectorListBuffer::FlattenSliceInternal(const LogicalType &type, const SelectionVector &sel,
                                                                idx_t count) const {
	// flatten the list offsets
	auto result = StandardVectorBuffer::FlattenSliceInternal(type, sel, count);
	// now flatten the child
	auto &list_result = result->Cast<VectorListBuffer>();
	auto &list_child = list_result.GetChild();
	list_child.Flatten();
	return result;
}

void VectorListBuffer::SetValue(const LogicalType &type, idx_t index, const Value &val) {
	if (!val.IsNull() && val.type() != type) {
		SetValue(type, index, val.DefaultCastAs(type));
		return;
	}
	validity.Set(index, !val.IsNull());
	auto offset = GetChildSize();
	if (val.IsNull()) {
		PushBack(Value());
		auto &entry = reinterpret_cast<list_entry_t *>(data_ptr)[index];
		entry.length = 1;
		entry.offset = offset;
	} else {
		auto &val_children = ListValue::GetChildren(val);
		for (idx_t i = 0; i < val_children.size(); i++) {
			PushBack(val_children[i]);
		}
		auto &entry = reinterpret_cast<list_entry_t *>(data_ptr)[index];
		entry.length = val_children.size();
		entry.offset = offset;
	}
}

Value VectorListBuffer::GetValue(const LogicalType &type, idx_t index) const {
	if (vector_type == VectorType::CONSTANT_VECTOR) {
		index = 0;
	}
	if (!validity.RowIsValid(index)) {
		return Value(type);
	}
	auto offlen = reinterpret_cast<const list_entry_t *>(data_ptr)[index];
	duckdb::vector<Value> children;
	for (idx_t i = offlen.offset; i < offlen.offset + offlen.length; i++) {
		children.push_back(child->GetValue(i));
	}
	if (type.id() == LogicalTypeId::MAP) {
		return Value::MAP(ListType::GetChildType(type), std::move(children));
	}
	return Value::LIST(ListType::GetChildType(type), std::move(children));
}

template <class T>
T &ListVector::GetEntryInternal(T &vector) {
	D_ASSERT(vector.GetType().id() == LogicalTypeId::LIST || vector.GetType().id() == LogicalTypeId::MAP);
	if (vector.GetVectorType() == VectorType::DICTIONARY_VECTOR) {
		auto &child = DictionaryVector::Child(vector);
		return GetEntryInternal<T>(child);
	}
	D_ASSERT(vector.GetVectorType() == VectorType::FLAT_VECTOR ||
	         vector.GetVectorType() == VectorType::CONSTANT_VECTOR);
	D_ASSERT(vector.GetBufferRef());
	D_ASSERT(vector.Buffer().GetBufferType() == VectorBufferType::LIST_BUFFER);
	return vector.GetBufferRef()->template Cast<VectorListBuffer>().GetChild();
}

const Vector &ListVector::GetChild(const Vector &vector) {
	return GetEntryInternal<const Vector>(vector);
}

Vector &ListVector::GetChildMutable(Vector &vector) {
	return GetEntryInternal<Vector>(vector);
}

void ListVector::Reserve(Vector &vector, idx_t required_capacity) {
	D_ASSERT(vector.GetType().id() == LogicalTypeId::LIST || vector.GetType().id() == LogicalTypeId::MAP);
	D_ASSERT(vector.GetVectorType() == VectorType::FLAT_VECTOR ||
	         vector.GetVectorType() == VectorType::CONSTANT_VECTOR);
	D_ASSERT(vector.GetBufferRef());
	D_ASSERT(vector.Buffer().GetBufferType() == VectorBufferType::LIST_BUFFER);
	auto &child_buffer = vector.BufferMutable().Cast<VectorListBuffer>();
	child_buffer.Reserve(required_capacity);
}

idx_t ListVector::GetListSize(const Vector &vec) {
	if (vec.GetVectorType() == VectorType::DICTIONARY_VECTOR) {
		auto &child = DictionaryVector::Child(vec);
		return ListVector::GetListSize(child);
	}
	D_ASSERT(vec.GetBufferRef());
	return vec.Buffer().Cast<VectorListBuffer>().GetChildSize();
}

idx_t ListVector::GetListCapacity(const Vector &vec) {
	if (vec.GetVectorType() == VectorType::DICTIONARY_VECTOR) {
		throw InternalException("ListVector::GetListCapacity called on dictionary vector");
	}
	D_ASSERT(vec.GetBufferRef());
	return vec.Buffer().Cast<VectorListBuffer>().GetChildCapacity();
}

void ListVector::SetListSize(Vector &vec, idx_t size) {
	if (vec.GetVectorType() == VectorType::DICTIONARY_VECTOR) {
		throw InternalException("ListVector::SetListSize called on dictionary vector");
	}
	auto &list_buffer = vec.BufferMutable().Cast<VectorListBuffer>();
	// ensure the child has enough capacity before setting its size (if the child is a flat/constant buffer)
	auto &child_buffer = list_buffer.GetChild().GetBufferRef();
	if (child_buffer) {
		auto vtype = child_buffer->GetVectorType();
		if ((vtype == VectorType::FLAT_VECTOR || vtype == VectorType::CONSTANT_VECTOR) &&
		    size > child_buffer->Capacity()) {
			list_buffer.Reserve(size);
		}
	}
	list_buffer.SetChildSize(size);
}

void ListVector::Append(Vector &target, const Vector &source, idx_t source_size) {
	if (source_size == 0) {
		//! Nothing to add
		return;
	}
	auto &target_buffer = target.BufferMutable().Cast<VectorListBuffer>();
	target_buffer.AppendToChild(source, source_size);
}

void ListVector::Append(Vector &target, const Vector &source, const SelectionVector &sel, idx_t source_size) {
	if (source_size == 0) {
		//! Nothing to add
		return;
	}
	auto &target_buffer = target.BufferMutable().Cast<VectorListBuffer>();
	target_buffer.AppendToChild(source, sel, source_size);
}

void ListVector::PushBack(Vector &target, const Value &insert) {
	auto &target_buffer = target.BufferMutable().Cast<VectorListBuffer>();
	target_buffer.PushBack(insert);
}

idx_t ListVector::GetConsecutiveChildList(const Vector &list, Vector &result, idx_t offset, idx_t count) {
	auto info = ListVector::GetConsecutiveChildListInfo(list, offset, count);
	if (info.needs_slicing) {
		SelectionVector sel(info.child_list_info.length);
		ListVector::GetConsecutiveChildSelVector(list, sel, offset, count);

		result.Slice(sel, info.child_list_info.length);
		result.Flatten();
	}
	return info.child_list_info.length;
}

idx_t ListVector::GetTotalEntryCount(const Vector &list) {
	RustVectorView view(list, sizeof(list_entry_t));
	return ThrowOnVectorError(rust::total_entry_count(view.entries, view.selection, view.validity, list.size()));
}

ConsecutiveChildListInfo ListVector::GetConsecutiveChildListInfo(const Vector &list, idx_t offset, idx_t count) {
	RustVectorView view(list, sizeof(list_entry_t));
	auto rust_info =
	    ThrowOnVectorError(rust::consecutive_child_list_info(view.entries, view.selection, view.validity, offset, count,
	                                                         list.GetVectorType() == VectorType::CONSTANT_VECTOR));

	ConsecutiveChildListInfo info;
	info.child_list_info.offset = rust_info.child_offset;
	info.child_list_info.length = rust_info.child_length;
	info.is_constant = rust_info.is_constant;
	info.needs_slicing = rust_info.needs_slicing;
	return info;
}

void ListVector::GetConsecutiveChildSelVector(const Vector &list, SelectionVector &sel, idx_t offset, idx_t count) {
	RustVectorView view(list, sizeof(list_entry_t));
	// the kernel reports a selection vector too small to gather into rather than
	// writing past its end, so the count it wrote can be dropped: the caller
	// sized the vector to that count
	ThrowOnVectorError(rust::consecutive_child_sel_vector(view.entries, view.selection, view.validity, offset, count,
	                                                      RustSelMut(sel)));
}

const Vector &VectorIteratorGetListChild(const Vector &vector) {
	return ListVector::GetChild(vector);
}

idx_t VectorIteratorGetListSize(const Vector &vector) {
	return ListVector::GetListSize(vector);
}

Vector &VectorWriterGetListChild(Vector &vector) {
	return ListVector::GetChildMutable(vector);
}

idx_t VectorWriterGetListSize(const Vector &vector) {
	return ListVector::GetListSize(vector);
}

void VectorWriterReserveList(Vector &vector, idx_t required_capacity) {
	ListVector::Reserve(vector, required_capacity);
}

void VectorWriterSetListSize(Vector &vector, idx_t size) {
	ListVector::SetListSize(vector, size);
}

} // namespace duckdb
