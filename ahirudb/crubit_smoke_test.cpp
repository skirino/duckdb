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
#include <string>
#include <string_view>
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

	// --- the exception module -----------------------------------------------
	// Everything below is what `src/common/exception` reaches into Rust for. The
	// interesting part is the shape: a factory hands C++ a payload of owned bytes and
	// C++ hands it straight back, so a wrong free or a wrong copy shows up here.
	namespace bridge = ahirudb::exception::bridge;
	using ahirudb::exception::types::ByteSlice;
	using ahirudb::exception::types::ExceptionPayload;
	using ahirudb::exception::types::OwnedBytes;
	using ahirudb::exception::types::QueryLocationLayout;

	Check(std::string_view(bridge::exception_type_to_string(24)) == "Binder",
	      "the binder type is named Binder, as the error text spells it");
	// NETWORK has no entry in the C++ EXCEPTION_MAP, so it has no name there either.
	Check(std::string_view(bridge::exception_type_to_string(25)) == "Unknown", "NETWORK has no name");
	Check(bridge::string_to_exception_type(std::string_view("Catalog")) == 13, "Catalog names the catalog type");
	Check(bridge::string_to_exception_type(std::string_view("nonsense")) == 0, "an unknown name is INVALID");

	Check(!bridge::invalidates_transaction(24), "a binder error leaves the transaction alone");
	Check(bridge::invalidates_database(30), "a fatal error invalidates the database");
	Check(bridge::is_execution_error(2), "a conversion error happens during execution");

	// Reads a payload the way the C++ bridge does: copy the bytes out, then free it.
	// An empty run is not read from at all - its pointer is a dangling sentinel.
	auto payload_text = [](const OwnedBytes &bytes) {
		if (bytes.len == 0) {
			return std::string();
		}
		return std::string(reinterpret_cast<const char *>(bytes.ptr), bytes.len);
	};
	auto check_payload = [&](const ExceptionPayload &payload, const char *message, std::size_t entry_count) {
		Check(payload_text(payload.message) == message, message);
		Check(payload.entry_count == entry_count, "the payload has the expected number of extra_info entries");
	};
	auto as_slice = [](std::string_view value) {
		return rs_std::SliceRef<const std::uint8_t>(reinterpret_cast<const std::uint8_t *>(value.data()), value.size());
	};
	auto as_views = [](std::string_view value) {
		return ByteSlice {reinterpret_cast<const std::uint8_t *>(value.data()), value.size()};
	};

	QueryLocationLayout location;
	location.offset = 10;
	location.length = 2;
	const auto known_location = bridge::set_query_location(location);
	Check(known_location.entry_count == 2, "a known location contributes position and location");
	Check(known_location.entry_count >= 2 && payload_text(known_location.entries[0].value) == "10",
	      "position is the start offset");
	Check(known_location.entry_count >= 2 && payload_text(known_location.entries[1].value) == "[10,2]",
	      "location is the [start, length] range");
	Check(payload_text(known_location.message).empty(), "set_query_location builds no message");
	bridge::free_payload(known_location);

	QueryLocationLayout unknown_location;
	unknown_location.offset = QueryLocationLayout::INVALID_OFFSET;
	unknown_location.length = 0;
	const auto no_location = bridge::set_query_location(unknown_location);
	Check(no_location.entry_count == 0, "an unknown location contributes nothing");
	bridge::free_payload(no_location);

	const auto already_exists = bridge::catalog_entry_already_exists(as_slice("Table"), as_slice("tbl"));
	check_payload(already_exists, "Table with name \"tbl\" already exists!", 3);
	Check(payload_text(already_exists.entries[0].key) == "error_subtype", "the subtype comes first");
	Check(payload_text(already_exists.entries[0].value) == "ENTRY_ALREADY_EXISTS", "the subtype is named");
	bridge::free_payload(already_exists);

	// A list of borrowed strings crosses as a list of views, and the message is built
	// around the candidate message C++ rendered for them.
	const std::vector<std::string_view> similar = {"id", "idx"};
	std::vector<ByteSlice> similar_views;
	for (const auto &value : similar) {
		similar_views.push_back(as_views(value));
	}
	const auto column_not_found = bridge::binder_column_not_found(
	    as_slice("i"), rs_std::SliceRef<const ByteSlice>(similar_views.data(), similar_views.size()),
	    as_slice("\nCandidate bindings: \"id\", \"idx\""), location);
	check_payload(column_not_found,
	              "Referenced column \"i\" not found in FROM clause!\nCandidate bindings: \"id\", "
	              "\"idx\"",
	              5);
	Check(payload_text(column_not_found.entries[4].value) == "id,idx", "the candidate names are joined with a comma");
	bridge::free_payload(column_not_found);

	// --- the allocator module ----------------------------------------------
	// One tracker per PrivateAllocatorData, owned through an opaque pointer
	// the way the C++ shell owns a LogManager: created in Rust, freed in Rust,
	// called into from C++.
	{
		using namespace ahirudb::allocator;
		namespace bridge = ahirudb::allocator::bridge;
		debug::DebugTracker *tracker = bridge::new_debug_tracker(true);
		tracker->track_allocate(
		    0x1000, 64, rs_std::SliceRef<const std::uint8_t>(reinterpret_cast<const std::uint8_t *>("stack"), 5));
		Check(tracker->outstanding_bytes() == 64, "the allocator tracker counts allocated bytes");
		Check(!tracker->is_empty(), "a tracker with an outstanding allocation is not empty");

		// A free that never happened, or one with the wrong size, is refused.
		Check(!tracker->track_free(0x2000, 64), "an untracked free is refused");
		Check(!tracker->track_free(0x1000, 32), "a free with the wrong size is refused");
		Check(tracker->outstanding_bytes() == 64, "a refused free leaves the accounting alone");

		Check(tracker->track_free(0x1000, 64), "the matching free is accepted");
		Check(tracker->is_empty(), "the tracker is empty again");

		// A reallocate moves the tracking from the old pointer to the new.
		tracker->track_allocate(0x3000, 32,
		                        rs_std::SliceRef<const std::uint8_t>(reinterpret_cast<const std::uint8_t *>("old"), 3));
		Check(tracker->track_reallocate(
		          0x3000, 0x4000, 32, 48,
		          rs_std::SliceRef<const std::uint8_t>(reinterpret_cast<const std::uint8_t *>("new"), 3)),
		      "a reallocate moves the tracking to the new pointer");
		Check(tracker->outstanding_bytes() == 48, "the tracker holds the reallocated size");
		Check(!tracker->track_free(0x3000, 32), "the old pointer no longer has tracking");

		// A leak report names every outstanding pointer.
		auto bytes_text = [](const debug::OwnedBytes &bytes) {
			if (bytes.len == 0) {
				return std::string();
			}
			return std::string(reinterpret_cast<const char *>(bytes.ptr), bytes.len);
		};
		debug::OwnedBytes leaked = tracker->leak_report();
		std::string report = bytes_text(leaked);
		Check(report.find("Outstanding allocations found for Allocator") != std::string::npos,
		      "the leak report has the expected header");
		Check(report.find("Allocation of size 48 at address 0x4000") != std::string::npos,
		      "the leak report names the outstanding pointer");
		Check(report.find("Stack trace:\nnew\n\n") != std::string::npos, "the leak report carries the stack trace");

		// The free that ends the leaked allocation works.
		Check(tracker->track_free(0x4000, 48), "the outstanding allocation frees normally");
		Check(bytes_text(tracker->leak_report()).empty(), "a tracker with nothing outstanding has an empty report");

		bridge::free_leak_report(leaked);
		bridge::delete_debug_tracker(tracker);

		// A bytes-only tracker never inspects pointers.
		debug::DebugTracker *counting = bridge::new_debug_tracker(false);
		counting->track_allocate(0x1000, 64, rs_std::SliceRef<const std::uint8_t>(nullptr, 0));
		Check(counting->track_free(0x9999, 64), "a bytes-only tracker accepts any pointer");
		bridge::delete_debug_tracker(counting);

		// The trim gate trims once per interval against a fresh call sequence.
		Check(bridge::should_trim(1000), "the first trim tick is due");
		Check(!bridge::should_trim(1000), "a second trim within the interval is not due");
	}

	// --- the enums module -----------------------------------------------
	{
		namespace bridge = ahirudb::enums::bridge;
		Check(std::string_view(bridge::expression_type_to_operator(25)) == "=",
		      "expression_type_to_operator maps EQUAL to =");
		Check(bridge::is_left_outer_join(1), "JOIN LEFT is a left outer join");
		Check(bridge::inverse_join_type(1) == 2, "LEFT inverts to RIGHT in enums bridge");
		const auto status = bridge::http_status_message(500);
		Check(std::string_view(status.data(), status.size()) == "Internal Server Error",
		      "http_status_message names the 500 code");
		Check(bridge::catalog_type_from_string(
		          rs_std::SliceRef<const std::uint8_t>(reinterpret_cast<const std::uint8_t *>("nonsense"), 8)) == -1,
		      "an unknown catalog name yields the -1 sentinel");
	}

	if (failures == 0) {
		std::printf("ahirudb bindings OK\n");
	}
	return failures == 0 ? 0 : 1;
}
