#include "core_functions_extension.hpp"
#include "core_functions/function_list.hpp"

namespace duckdb {

static void LoadInternal(ExtensionLoader &loader) {
	FunctionList::RegisterExtensionFunctions(loader, CoreFunctionList::GetFunctionList());
}

void CoreFunctionsExtension::Load(ExtensionLoader &loader) {
	LoadInternal(loader);
}

std::string CoreFunctionsExtension::Name() {
	return "core_functions";
}

std::string CoreFunctionsExtension::Version() const {
	return DefaultVersion();
}

} // namespace duckdb

extern "C" {

DUCKDB_CPP_EXTENSION_ENTRY(core_functions, loader) {
	duckdb::LoadInternal(loader);
}
}
