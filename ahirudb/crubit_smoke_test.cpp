// Proves the C++ side of the ahirudb crate: that Crubit's generated bindings
// compile against this toolchain, that calling into the Rust code works, and
// that the staticlib the generator produced links.
//
// `LogLevel` is the smallest thing in the crate that still runs Rust rather than
// only carrying a byte: the `From<u8>` conversion decodes a discriminant in Rust
// and the comparison orders it there too, so a wrong bridging shows up as a wrong
// answer rather than as a link error.

#include "crubit/ahirudb.h"

#include <cstdint>
#include <cstdio>

namespace {

int failures = 0;

void Check(bool condition, const char *what) {
	if (!condition) {
		std::printf("FAILED: %s\n", what);
		failures++;
	}
}

} // namespace

int main() {
	using ahirudb::logging::LogLevel;

	// The discriminants are the ones src/logging/logging.rs assigns.
	Check(LogLevel(10) == LogLevel::MakeLOG_TRACE(), "10 decodes to LOG_TRACE");
	Check(LogLevel(60) == LogLevel::MakeLOG_FATAL(), "60 decodes to LOG_FATAL");

	// An unknown discriminant falls back to LOG_INFO, which is decided in Rust.
	Check(LogLevel(0) == LogLevel::MakeLOG_INFO(), "an unknown discriminant is LOG_INFO");
	Check(LogLevel(255) == LogLevel::MakeLOG_INFO(), "255 is not a log level");

	Check(LogLevel::MakeLOG_ERROR() > LogLevel::MakeLOG_WARNING(), "ERROR outranks WARNING");
	Check(LogLevel::MakeLOG_TRACE() < LogLevel::MakeLOG_DEBUG(), "TRACE is below DEBUG");

	if (failures == 0) {
		std::printf("ahirudb bindings OK\n");
	}
	return failures == 0 ? 0 : 1;
}
