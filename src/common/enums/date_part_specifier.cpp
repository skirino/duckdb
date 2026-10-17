#include "duckdb/common/enums/date_part_specifier.hpp"
#include "duckdb/common/string_util.hpp"
#include "duckdb/common/exception/conversion_exception.hpp"

#include "enums_rust_bridge.hpp"

namespace duckdb {

bool IsBigintDatepart(DatePartSpecifier part_code) {
	return EnumsRustBridge::IsBigintDatepart(part_code);
}

bool TryGetDatePartSpecifier(const string &specifier, DatePartSpecifier &result) {
	return EnumsRustBridge::TryGetDatePartSpecifier(specifier, result);
}

DatePartSpecifier GetDatePartSpecifier(const string &specifier) {
	return EnumsRustBridge::GetDatePartSpecifier(specifier);
}

} // namespace duckdb
