// Proves the C++ side of the ahirudb crate: that Crubit's generated bindings
// compile against this toolchain, that calling into the Rust code works, and
// that the staticlib the generator produced links.
//
// `LogLevel` is the smallest thing in the crate that still runs Rust rather than
// only carrying a byte: the `From<u8>` conversion decodes a discriminant in Rust
// and the comparison orders it there too, so a wrong bridging shows up as a wrong
// answer rather than as a link error.
//
// The vector kernels below are the other half: `copy_vector_buffer`,
// `array_child_sel` and the list-child gather are functions `src/common/vector`
// now calls into Rust, and they read and write through `SliceRef`s that C++
// placed over its own buffers. A kernel that read the wrong rows, or that wrote
// through the bridge in a way C++ could not see, shows up here as a wrong
// answer.

#include "crubit/ahirudb.h"

#include <cstdint>
#include <cstdio>
#include <vector>

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

	// --- the vector kernels -------------------------------------------------
	// Three 4-byte rows, gathered out of order into a target C++ owns.
	std::vector<std::uint32_t> source = {10, 20, 30};
	std::vector<std::uint32_t> target(3, 0);
	const std::uint32_t selection[] = {2, 0, 1};

	const auto gathered = ahirudb::vector::flat_vector::copy_vector_buffer(
	    rs_std::SliceRef<std::uint8_t>(reinterpret_cast<std::uint8_t *>(target.data()),
	                                   target.size() * sizeof(std::uint32_t)),
	    rs_std::SliceRef<const std::uint8_t>(reinterpret_cast<const std::uint8_t *>(source.data()),
	                                         source.size() * sizeof(std::uint32_t)),
	    rs_std::SliceRef<const std::uint32_t>(selection, 3), 0, 0, 3, sizeof(std::uint32_t));

	Check(gathered.has_value(), "copy_vector_buffer accepts an in-range selection");
	Check(target[0] == 30 && target[1] == 10 && target[2] == 20, "copy_vector_buffer gathers through a selection");

	// A selection vector that reaches past the end of the source is reported,
	// not read: the same call in C++ used to read out of bounds.
	std::vector<std::uint32_t> short_target(1, 0);
	const std::uint32_t bad_selection[] = {7};
	auto refused = ahirudb::vector::flat_vector::copy_vector_buffer(
	    rs_std::SliceRef<std::uint8_t>(reinterpret_cast<std::uint8_t *>(short_target.data()), sizeof(std::uint32_t)),
	    rs_std::SliceRef<const std::uint8_t>(reinterpret_cast<const std::uint8_t *>(source.data()),
	                                         source.size() * sizeof(std::uint32_t)),
	    rs_std::SliceRef<const std::uint32_t>(bad_selection, 1), 0, 0, 1, sizeof(std::uint32_t));
	Check(!refused.has_value(), "copy_vector_buffer refuses a source row that is out of range");
	Check(short_target[0] == 0, "a refused copy leaves the target alone");

	// --- the list kernels ----------------------------------------------------
	// Two list entries over a three-row child, and the gather they expand into.
	// `total_entry_count` and `consecutive_child_sel_vector` read `list_entry_t`
	// as raw bytes, which is how DuckDB stores them.
	struct ListEntry {
		std::uint64_t offset;
		std::uint64_t length;
	};
	const ListEntry list_entries[] = {{0, 2}, {1, 1}};
	const auto entry_bytes = reinterpret_cast<const std::uint8_t *>(list_entries);
	const auto entry_byte_count = sizeof(list_entries);

	// A kernel that starts reporting an error must print FAILED, not abort:
	// `has_value()` is checked before every `.value()` below, because
	// `rs_std::Result::value()` is a CRUBIT_CHECK and ends the process.
	const auto entry_total = ahirudb::vector::list_vector::total_entry_count(
	    rs_std::SliceRef<const std::uint8_t>(entry_bytes, entry_byte_count), rs_std::SliceRef<const std::uint32_t>(),
	    rs_std::SliceRef<const std::uint64_t>(), 2);
	Check(entry_total.has_value(), "total_entry_count accepts the entries it was given");
	Check(entry_total.has_value() && entry_total.value() == 3, "total_entry_count sums the child lengths");

	std::uint32_t child_rows[3] = {0, 0, 0};
	const auto written = ahirudb::vector::list_vector::consecutive_child_sel_vector(
	    rs_std::SliceRef<const std::uint8_t>(entry_bytes, entry_byte_count), rs_std::SliceRef<const std::uint32_t>(),
	    rs_std::SliceRef<const std::uint64_t>(), 0, 2, rs_std::SliceRef<std::uint32_t>(child_rows, 3));
	Check(written.has_value(), "consecutive_child_sel_vector accepts a selection vector with room");
	Check(written.has_value() && written.value() == 3 && child_rows[0] == 0 && child_rows[1] == 1 && child_rows[2] == 1,
	      "consecutive_child_sel_vector gathers the child rows");

	// A selection vector with no room for the rows the entries name is reported
	// rather than written past.
	std::uint32_t short_child_rows[2] = {0, 0};
	const auto refused_write = ahirudb::vector::list_vector::consecutive_child_sel_vector(
	    rs_std::SliceRef<const std::uint8_t>(entry_bytes, entry_byte_count), rs_std::SliceRef<const std::uint32_t>(),
	    rs_std::SliceRef<const std::uint64_t>(), 0, 2, rs_std::SliceRef<std::uint32_t>(short_child_rows, 2));
	Check(!refused_write.has_value(), "consecutive_child_sel_vector refuses a selection vector that is too small");

	// --- the array kernel ----------------------------------------------------
	// Two array rows of three elements each, read through a selection that takes
	// array row 2 before array row 0.
	const std::uint32_t array_rows[] = {2, 0};
	const auto child_sel = ahirudb::vector::array_vector::array_child_sel(
	    rs_std::SliceRef<const std::uint32_t>(array_rows, 2), rs_std::SliceRef<const std::uint64_t>(), 0, 2, 3);
	Check(child_sel.has_value(), "array_child_sel accepts the rows it was given");
	Check(child_sel.has_value() && child_sel.value().size() == 6, "array_child_sel expands two array rows");
	Check(child_sel.has_value() && child_sel.value()[0] == 6 && child_sel.value()[1] == 7 && child_sel.value()[2] == 8,
	      "array_child_sel gathers the first selected row's elements");
	Check(child_sel.has_value() && child_sel.value()[3] == 0 && child_sel.value()[4] == 1 && child_sel.value()[5] == 2,
	      "array_child_sel gathers the second selected row's elements");

	// A selection vector too short to name the rows asked for is reported rather
	// than read past.
	const auto refused_array = ahirudb::vector::array_vector::array_child_sel(
	    rs_std::SliceRef<const std::uint32_t>(array_rows, 1), rs_std::SliceRef<const std::uint64_t>(), 0, 2, 3);
	Check(!refused_array.has_value(), "array_child_sel refuses a selection vector that is too short");

	if (failures == 0) {
		std::printf("ahirudb bindings OK\n");
	}
	return failures == 0 ? 0 : 1;
}
