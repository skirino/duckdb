#include "duckdb/common/serializer/binary_serializer.hpp"

#include "crubit/ahirudb.h"

#include "duckdb/common/exception.hpp"
#include "duckdb/common/string_util.hpp"

namespace duckdb {

namespace {
thread_local std::exception_ptr pending_stream_error;
}

extern "C" void WriteDataThunk(void *ctx, const uint8_t *buffer, uint64_t size) {
	if (pending_stream_error) {
		return;
	}
	try {
		reinterpret_cast<WriteStream *>(ctx)->WriteData(const_data_ptr_cast(buffer), size);
	} catch (...) {
		pending_stream_error = std::current_exception();
	}
}

static void ClearPendingErrors() {
	pending_stream_error = nullptr;
	ahirudb::serializer::cpp_bridge::clear_pending_error();
}

BinarySerializer::BinarySerializer(WriteStream &stream, SerializationOptions options_p)
    : stream(stream), rust_(reinterpret_cast<void *>(ahirudb::serializer::cpp_bridge::cpp_binary_serializer_new(
                          reinterpret_cast<uintptr_t>(&stream),
                          reinterpret_cast<uintptr_t>(&WriteDataThunk)))) {
	ClearPendingErrors();
	options = std::move(options_p);
	options.serialize_enum_as_string = false;
}

BinarySerializer::~BinarySerializer() {
	ClearPendingErrors();
	delete reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_);
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

void BinarySerializer::OnPropertyBegin(const field_id_t field_id, const char *tag) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->on_property_begin(field_id,
	                                                           reinterpret_cast<const uint8_t *>(tag));
	CheckPendingError();
}

void BinarySerializer::OnPropertyEnd() {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->on_property_end();
	CheckPendingError();
}

void BinarySerializer::OnOptionalPropertyBegin(const field_id_t field_id, const char *tag, bool present) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->on_optional_property_begin(field_id,
	                                                                     reinterpret_cast<const uint8_t *>(tag), present);
	CheckPendingError();
}

void BinarySerializer::OnOptionalPropertyEnd(bool present) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->on_optional_property_end(present);
	CheckPendingError();
}

void BinarySerializer::OnListBegin(idx_t count) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->on_list_begin(count);
	CheckPendingError();
}

void BinarySerializer::OnListEnd() {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->on_list_end();
	CheckPendingError();
}

void BinarySerializer::OnObjectBegin() {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->on_object_begin();
	CheckPendingError();
}

void BinarySerializer::OnObjectEnd() {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->on_object_end();
	CheckPendingError();
}

void BinarySerializer::OnNullableBegin(bool present) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->on_nullable_begin(present);
	CheckPendingError();
}

void BinarySerializer::OnNullableEnd() {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->on_nullable_end();
	CheckPendingError();
}

void BinarySerializer::WriteNull() {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_null();
	CheckPendingError();
}

void BinarySerializer::WriteValue(char value) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_char(static_cast<int8_t>(value));
	CheckPendingError();
}

void BinarySerializer::WriteValue(uint8_t value) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_uint8(value);
	CheckPendingError();
}

void BinarySerializer::WriteValue(int8_t value) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_int8(value);
	CheckPendingError();
}

void BinarySerializer::WriteValue(uint16_t value) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_uint16(value);
	CheckPendingError();
}

void BinarySerializer::WriteValue(int16_t value) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_int16(value);
	CheckPendingError();
}

void BinarySerializer::WriteValue(uint32_t value) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_uint32(value);
	CheckPendingError();
}

void BinarySerializer::WriteValue(int32_t value) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_int32(value);
	CheckPendingError();
}

void BinarySerializer::WriteValue(uint64_t value) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_uint64(value);
	CheckPendingError();
}

void BinarySerializer::WriteValue(int64_t value) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_int64(value);
	CheckPendingError();
}

void BinarySerializer::WriteValue(hugeint_t value) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_hugeint(value.upper, value.lower);
	CheckPendingError();
}

void BinarySerializer::WriteValue(uhugeint_t value) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_uhugeint(value.upper, value.lower);
	CheckPendingError();
}

void BinarySerializer::WriteValue(float value) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_float(value);
	CheckPendingError();
}

void BinarySerializer::WriteValue(double value) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_double(value);
	CheckPendingError();
}

void BinarySerializer::WriteValue(const string_t value) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_string_t(reinterpret_cast<const uint8_t *>(value.GetDataUnsafe()), value.GetSize());
	CheckPendingError();
}

void BinarySerializer::WriteValue(const string &value) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_string(reinterpret_cast<const uint8_t *>(value.c_str()), value.length());
	CheckPendingError();
}

void BinarySerializer::WriteValue(const char *value) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_cstr(reinterpret_cast<const uint8_t *>(value));
	CheckPendingError();
}

void BinarySerializer::WriteValue(bool value) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_bool(value);
	CheckPendingError();
}

void BinarySerializer::WriteDataPtr(const_data_ptr_t ptr, idx_t count) {
	ClearPendingErrors();
	reinterpret_cast<ahirudb::serializer::cpp_bridge::CppBinarySerializer *>(rust_)->write_data_ptr(reinterpret_cast<const uint8_t *>(ptr), count);
	CheckPendingError();
}

} // namespace duckdb
