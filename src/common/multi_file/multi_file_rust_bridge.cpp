//===----------------------------------------------------------------------===//
//                         DuckDB
//
// common/multi_file/multi_file_rust_bridge.cpp
//
//===----------------------------------------------------------------------===//

#include "multi_file_rust_bridge.hpp"
#include "duckdb/common/exception.hpp"
#include "duckdb/common/helper.hpp"

namespace duckdb {

void ThrowMultiFileError(const ::rs::alloc::string::String &error) {
	throw InternalException("%s", string(std::string_view(error.as_str())).c_str());
}

//===--------------------------------------------------------------------===//
// Call scope bookkeeping
//===--------------------------------------------------------------------===//
namespace {

struct PendingMultiFileError {
	std::exception_ptr error;
	uint64_t call_scope = 0;
};

struct CallScopeState {
	PendingMultiFileError pending_error;
	uint64_t next_call_scope = 0;
	uint64_t active_call_scope = 0;
};

CallScopeState &CallScopeStates() {
	static thread_local CallScopeState state;
	return state;
}

PendingMultiFileError &PendingMultiFileErrorSlot() {
	return CallScopeStates().pending_error;
}

//! Runs `fn` on the C++ side, reporting success instead of letting an exception escape
template <class F>
bool CatchMultiFileException(F &&fn) {
	static_assert(std::is_void_v<decltype(fn())>, "CatchMultiFileException only reports whether the call threw");
	D_ASSERT(CallScopeStates().active_call_scope != 0);
	try {
		fn();
		return true;
	} catch (...) {
		PendingMultiFileErrorSlot() =
		    PendingMultiFileError {std::current_exception(), CallScopeStates().active_call_scope};
		return false;
	}
}

} // namespace

multi_file_bridge::CallScope::CallScope()
    : index(++CallScopeStates().next_call_scope), previous(CallScopeStates().active_call_scope) {
	CallScopeStates().active_call_scope = index;
}

multi_file_bridge::CallScope::~CallScope() {
	CallScopeStates().active_call_scope = previous;
}

void multi_file_bridge::RethrowPendingError(const CallScope &call_scope) {
	auto &slot = PendingMultiFileErrorSlot();
	if (!slot.error) {
		return;
	}
	if (slot.call_scope != call_scope.index) {
		slot = PendingMultiFileError();
		return;
	}
	auto error = slot.error;
	slot = PendingMultiFileError();
	std::rethrow_exception(error);
}

void multi_file_bridge::ClearPendingError() {
	PendingMultiFileErrorSlot() = PendingMultiFileError();
}

} // namespace duckdb
