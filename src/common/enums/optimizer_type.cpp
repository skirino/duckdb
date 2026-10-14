#include "duckdb/common/enums/optimizer_type.hpp"

#include "duckdb/common/exception.hpp"
#include "duckdb/common/exception/parser_exception.hpp"
#include "duckdb/common/string_util.hpp"
#include "duckdb/optimizer/optimizer.hpp"

#include "enums_rust_bridge.hpp"

namespace duckdb {

string OptimizerTypeToString(OptimizerType type) {
	auto result = EnumsRustBridge::OptimizerNameForDiscriminant(static_cast<uint32_t>(type));
	if (result.empty()) {
		throw InternalException("Invalid optimizer type");
	}
	return result;
}

vector<string> ListAllOptimizers() {
	vector<string> result;
	for (idx_t i = 0; i < EnumsRustBridge::OptimizerCount(); i++) {
		result.push_back(EnumsRustBridge::OptimizerName(i));
	}
	return result;
}

OptimizerType OptimizerTypeFromString(const string &str) {
	auto idx = EnumsRustBridge::OptimizerIndexOfName(str);
	if (idx < 0) {
		vector<string> optimizer_names;
		for (idx_t i = 0; i < EnumsRustBridge::OptimizerCount(); i++) {
			optimizer_names.emplace_back(EnumsRustBridge::OptimizerName(i));
		}
		throw ParserException("Optimizer type \"%s\" not recognized\n%s", str,
		                      StringUtil::CandidatesErrorMessage(optimizer_names, str, "Candidate optimizers"));
	}
	return static_cast<OptimizerType>(EnumsRustBridge::OptimizerDiscriminant(static_cast<idx_t>(idx)));
}

} // namespace duckdb