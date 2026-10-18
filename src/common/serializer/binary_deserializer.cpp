#include "duckdb/common/serializer/binary_deserializer.hpp"

#include "crubit/ahirudb.h"

#include "duckdb/common/exception.hpp"

namespace duckdb {

namespace {
thread_local std::exception_ptr pending_stream_error;
}

extern "C" void BinaryDeserializerReadDataThunk(void *ctx, uint8_t *buffer, uint64_t size) {
	if (pending_stream_error) {
		return;
	}
	try {
		reinterpret_cast<ReadStream *>(ctx)->ReadData(buffer, size);
	} catch (...) {
		pending_stream_error = std::current_exception();
	}
}

static void ClearPendingErrors() {
	pending_stream_error = nullptr;
	ahirudb::serializer::cpp_bridge::clear_pending_error();
}

BinaryDeserializer::BinaryDeserializer(ReadStream &stream)
    : stream(stream), rust_(reinterpret_cast<void *>(ahirudb::serializer::cpp_bridge::cpp_binary_deserializer_new(
                          reinterpret_cast<uintptr_t>(&stream),
                          reinterpret_cast<uintptr_t>(&BinaryDeserializerReadDataThunk)))) {
	ClearPendingErrors();
	deserialize_enum_from_string = false;
}

BinaryDeserializer::~BinaryDeserializer() {
	ClearPendingErrors();
	delete reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_);
	rust_ = nullptr;
}

namespace {
void CheckPendingError() {
	if (pending_stream_error) {
		auto e = pending_stream_error;
		pending_stream_error = nullptr;
		std::rethrow_exception(e);
	}
	if (ahirudb::serializer::cpp_bridge::has_pending_error()) {
		auto len = ahirudb::serializer::cpp_bridge::pending_error_len();
		string msg(len, '\0');
		if (len > 0) {
			std::memcpy(msg.data(), ahirudb::serializer::cpp_bridge::pending_error_ptr(), len);
		}
		ahirudb::serializer::cpp_bridge::clear_pending_error();
		throw SerializationException("%s", msg.c_str());
	}
}
} // namespace

void BinaryDeserializer::OnPropertyBegin(const field_id_t field_id, const char *) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->on_property_begin(field_id,
	                                                             nullptr);
	CheckPendingError();
}

void BinaryDeserializer::OnPropertyEnd() {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->on_property_end();
	CheckPendingError();
}

bool BinaryDeserializer::OnOptionalPropertyBegin(const field_id_t field_id, const char *) {
	ClearPendingErrors();
	bool present = reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->on_optional_property_begin(field_id, nullptr);
	CheckPendingError();
	return present;
}

void BinaryDeserializer::OnOptionalPropertyEnd(bool present) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->on_optional_property_end(present);
	CheckPendingError();
}

void BinaryDeserializer::OnObjectBegin() {
	ClearPendingErrors();
	nesting_level++;
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->on_object_begin();
	CheckPendingError();
}

void BinaryDeserializer::OnObjectEnd() {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->on_object_end();
	CheckPendingError();
	nesting_level--;
}

idx_t BinaryDeserializer::OnListBegin() {
	ClearPendingErrors();
	idx_t count = reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->on_list_begin();
	CheckPendingError();
	return count;
}

void BinaryDeserializer::OnListEnd() {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->on_list_end();
	CheckPendingError();
}

bool BinaryDeserializer::OnNullableBegin() {
	ClearPendingErrors();
	bool present = reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->on_nullable_begin();
	CheckPendingError();
	return present;
}

void BinaryDeserializer::OnNullableEnd() {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->on_nullable_end();
	CheckPendingError();
}

bool BinaryDeserializer::CanDeserializeProperty(const field_id_t field_id, const char *) {
	ClearPendingErrors();
	auto next = reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->peek_field();
	CheckPendingError();
	return next == field_id;
}

bool BinaryDeserializer::ReadBool() {
	ClearPendingErrors();
	bool v = reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->read_bool();
	CheckPendingError();
	return v;
}

char BinaryDeserializer::ReadChar() {
	ClearPendingErrors();
	char v = static_cast<char>(reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->read_char());
	CheckPendingError();
	return v;
}

int8_t BinaryDeserializer::ReadSignedInt8() {
	ClearPendingErrors();
	int8_t v = reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->read_int8();
	CheckPendingError();
	return v;
}

uint8_t BinaryDeserializer::ReadUnsignedInt8() {
	ClearPendingErrors();
	uint8_t v = reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->read_uint8();
	CheckPendingError();
	return v;
}

int16_t BinaryDeserializer::ReadSignedInt16() {
	ClearPendingErrors();
	int16_t v = reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->read_int16();
	CheckPendingError();
	return v;
}

uint16_t BinaryDeserializer::ReadUnsignedInt16() {
	ClearPendingErrors();
	uint16_t v = reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->read_uint16();
	CheckPendingError();
	return v;
}

int32_t BinaryDeserializer::ReadSignedInt32() {
	ClearPendingErrors();
	int32_t v = reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->read_int32();
	CheckPendingError();
	return v;
}

uint32_t BinaryDeserializer::ReadUnsignedInt32() {
	ClearPendingErrors();
	uint32_t v = reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->read_uint32();
	CheckPendingError();
	return v;
}

int64_t BinaryDeserializer::ReadSignedInt64() {
	ClearPendingErrors();
	int64_t v = reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->read_int64();
	CheckPendingError();
	return v;
}

uint64_t BinaryDeserializer::ReadUnsignedInt64() {
	ClearPendingErrors();
	uint64_t v = reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->read_uint64();
	CheckPendingError();
	return v;
}

float BinaryDeserializer::ReadFloat() {
	ClearPendingErrors();
	float v = reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->read_float();
	CheckPendingError();
	return v;
}

double BinaryDeserializer::ReadDouble() {
	ClearPendingErrors();
	double v = reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->read_double();
	CheckPendingError();
	return v;
}

string BinaryDeserializer::ReadString() {
	ClearPendingErrors();
	auto len = reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->read_string();
	CheckPendingError();
	if (len == 0) {
		return string();
	}
	auto ptr = ahirudb::serializer::cpp_bridge::CppBinaryDeserializer::last_string_ptr();
	return string(const_char_ptr_cast(ptr), len);
}

hugeint_t BinaryDeserializer::ReadHugeInt() {
	ClearPendingErrors();
	int64_t upper = 0;
	uint64_t lower = 0;
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->read_hugeint(&upper, &lower);
	CheckPendingError();
	return hugeint_t(upper, lower);
}

uhugeint_t BinaryDeserializer::ReadUhugeInt() {
	ClearPendingErrors();
	uint64_t upper = 0;
	uint64_t lower = 0;
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->read_uhugeint(&upper, &lower);
	CheckPendingError();
	return uhugeint_t(upper, lower);
}

void BinaryDeserializer::ReadDataPtr(data_ptr_t &ptr, idx_t count) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinaryDeserializer *>(rust_)->read_data_into(ptr, count);
	CheckPendingError();
}

} // namespace duckdb
