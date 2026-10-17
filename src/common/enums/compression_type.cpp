#include "duckdb/common/enums/compression_type.hpp"

#include "duckdb/common/exception.hpp"
#include "duckdb/common/string_util.hpp"
#include "duckdb/storage/storage_info.hpp"
#include "duckdb/storage/storage_manager.hpp"

#include "enums_rust_bridge.hpp"

namespace duckdb {

vector<string> ListCompressionTypes(void) {
	vector<string> compression_types;
	uint8_t amount_of_compression_options = (uint8_t)CompressionType::COMPRESSION_COUNT;
	compression_types.reserve(amount_of_compression_options);
	for (uint8_t i = 0; i < amount_of_compression_options; i++) {
		compression_types.push_back(CompressionTypeToString((CompressionType)i));
	}
	return compression_types;
}

namespace {
struct CompressionMethodRequirements {
	CompressionType type;
	StorageVersion minimum_storage_version;
	StorageVersion maximum_storage_version;
};
} // namespace

CompressionAvailabilityResult CompressionTypeIsAvailable(CompressionType compression_type,
                                                         optional_ptr<StorageManager> storage_manager) {
	// version-gated availability kept in C++
	vector<CompressionMethodRequirements> candidates(
	    {{CompressionType::COMPRESSION_PATAS, StorageVersion::INVALID, StorageVersion::V0_10_2},
	     {CompressionType::COMPRESSION_CHIMP, StorageVersion::INVALID, StorageVersion::V0_10_2},
	     {CompressionType::COMPRESSION_DICTIONARY, StorageVersion::V0_10_2, StorageVersion::V1_2_0},
	     {CompressionType::COMPRESSION_FSST, StorageVersion::V0_10_2, StorageVersion::V1_2_0},
	     {CompressionType::COMPRESSION_ROARING, StorageVersion::V1_2_0, StorageVersion::LATEST},
	     {CompressionType::COMPRESSION_ZSTD, StorageVersion::V1_2_0, StorageVersion::LATEST},
	     {CompressionType::COMPRESSION_DICT_FSST, StorageVersion::V1_3_0, StorageVersion::LATEST},
	     {CompressionType::COMPRESSION_PFOR_DELTA, (StorageVersion)((int)StorageVersion::LATEST + 1),
	      StorageVersion::INVALID}});

	StorageVersion current_storage_version = StorageVersion::INVALID;
	if (storage_manager && storage_manager->HasStorageVersion()) {
		current_storage_version = storage_manager->GetStorageVersion();
	}
	for (auto &candidate : candidates) {
		auto &type = candidate.type;
		if (type != compression_type) {
			continue;
		}
		auto &min = candidate.minimum_storage_version;
		auto &max = candidate.maximum_storage_version;

		if (min == StorageVersion::INVALID) {
			return CompressionAvailabilityResult::Deprecated();
		}

		if (current_storage_version == StorageVersion::INVALID) {
			return CompressionAvailabilityResult();
		}

		auto current_version = current_storage_version;
		D_ASSERT(min != StorageVersion::INVALID);
		if (min > current_version) {
			return CompressionAvailabilityResult::NotAvailableYet();
		}
		if (max != StorageVersion::INVALID && max < current_version) {
			return CompressionAvailabilityResult::Deprecated();
		}
		return CompressionAvailabilityResult();
	}
	return CompressionAvailabilityResult();
}

string CompressionTypeToString(CompressionType type) {
	return EnumsRustBridge::CompressionTypeToString(type);
}

} // namespace duckdb
