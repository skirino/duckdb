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
#include <optional>
#include <string>
#include <string_view>
#include <tuple>
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
	// --- the multi_file module -------------------------------------------
	// The kernels src/common/multi_file calls into Rust for: which index is
	// valid, what a filter entry means, and how many files a list holds. Each
	// of them used to be an inline C++ rule, so a bridging that crossed the
	// values wrongly shows up here as a wrong answer.
	//
	// This block runs before the logging checks below on purpose: those abort
	// on an unknown discriminant until task 007 lands the logging fix, and a
	// block after them would never run in CI.
	{
		namespace data = ahirudb::multi_file::data;
		namespace list = ahirudb::multi_file::list;

		Check(data::local_index_is_valid(0), "0 is a valid local index");
		Check(!data::local_index_is_valid(data::INVALID_INDEX), "INVALID_INDEX is not a valid local index");
		Check(data::local_index_get_index(3).has_value() && data::local_index_get_index(3).value() == 3,
		      "a valid local index unwraps to itself");
		Check(!data::local_index_get_index(data::INVALID_INDEX).has_value(),
		      "an invalid local index is refused rather than unwrapped");

		Check(data::local_column_ids_get(2, 1).has_value(), "a position inside the list resolves");
		Check(!data::local_column_ids_get(2, 2).has_value(), "a position past the list is refused");

		const auto unset = data::MultiFileFilterEntryLayout();
		const auto local = data::filter_entry_set_local(1);
		const auto constant = data::filter_entry_set_constant(2);
		Check(!unset.is_set, "an entry that was never set is unset");
		Check(unset.index == data::INVALID_INDEX, "an entry that was never set holds no index");
		Check(local.is_set && !local.is_constant && local.index == 1, "a local entry holds its local index");
		Check(constant.is_set && constant.is_constant && constant.index == 2, "a constant entry holds its index");
		Check(data::filter_entry_is_constant(local).has_value() && !data::filter_entry_is_constant(local).value(),
		      "a local entry is not a constant one");
		Check(!data::filter_entry_is_constant(unset).has_value(), "an unset entry is neither kind");
		Check(data::filter_entry_get_local_index(local).has_value() &&
		          data::filter_entry_get_local_index(local).value() == 1,
		      "a local entry reads back its local index");
		Check(!data::filter_entry_get_constant_index(local).has_value(), "a local entry has no constant index");
		Check(data::filter_entry_get_constant_index(constant).has_value() &&
		          data::filter_entry_get_constant_index(constant).value() == 2,
		      "a constant entry reads back its constant index");

		Check(!data::filter_map_get(1, 1).has_value(), "a global index the filter map does not cover is refused");
		Check(data::constant_map_get(2, 1).has_value(), "a constant-map index inside the map resolves");
		Check(!data::constant_map_get(2, 2).has_value(), "a constant-map index past the map is refused");

		Check(list::expand_result(0) == list::FileExpandResult::MakeNO_FILES(), "0 files is NO_FILES");
		Check(list::expand_result(1) == list::FileExpandResult::MakeSINGLE_FILE(), "1 file is SINGLE_FILE");
		Check(list::expand_result(7) == list::FileExpandResult::MakeMULTIPLE_FILES(), "7 files is MULTIPLE_FILES");

		const auto exact = list::file_count(4, true);
		Check(exact.get_count() == 4 && exact.get_type() == list::FileExpansionType::MakeALL_FILES_EXPANDED(),
		      "a finished list counts exactly");
		const auto partial = list::file_count(4, false);
		Check(partial.get_count() == 4 && partial.get_type() == list::FileExpansionType::MakeNOT_ALL_FILES_KNOWN(),
		      "a list with globs left counts inexactly");

		Check(list::scan_fetches_unconditionally(list::MultiFileListScanType::MakeALWAYS_FETCH()),
		      "an ALWAYS_FETCH scan always fetches");
		Check(!list::scan_fetches_unconditionally(list::MultiFileListScanType::MakeFETCH_IF_AVAILABLE()),
		      "a FETCH_IF_AVAILABLE scan does not always fetch");

		// an entry built in Rust in one step, not default-constructed there and then overwritten here
		const auto entry = data::MultiFileFilterEntryLayout::new_(true, true, 5);
		Check(entry.is_set && entry.is_constant && entry.index == 5,
		      "an entry is built from its kind and its index");
	}

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
	// Every helper src/common/enums turned into a Rust call. The checks below
	// pin the two regressions a rewrite of the match tables is likely to
	// introduce: a comparison operator losing its name, and the `year`
	// specifier losing its numeric aliases.
	{
		namespace bridge = ahirudb::enums::bridge;
		Check(std::string_view(bridge::expression_type_to_operator(25)) == "=",
		      "expression_type_to_operator maps EQUAL to =");
		Check(bridge::is_left_outer_join(1), "JOIN LEFT is a left outer join");
		Check(bridge::inverse_join_type(1) == 2, "LEFT inverts to RIGHT in enums bridge");
		const auto status = bridge::http_status_message(500);
		Check(std::string_view(status.data(), status.size()) == "Internal Server Error",
		      "http_status_message names the 500 code");

		// The comparison labels, which the match table has to name exactly:
		// 25..30 are the comparisons and 37/40 the distinctness tests.
		Check(std::string_view(bridge::expression_type_to_string(25)) == "EQUAL", "25 names EQUAL");
		Check(std::string_view(bridge::expression_type_to_string(26)) == "NOTEQUAL", "26 names NOTEQUAL");
		Check(std::string_view(bridge::expression_type_to_string(27)) == "LESSTHAN", "27 names LESSTHAN");
		Check(std::string_view(bridge::expression_type_to_string(28)) == "GREATERTHAN", "28 names GREATERTHAN");
		Check(std::string_view(bridge::expression_type_to_string(29)) == "LESSTHANOREQUALTO",
		      "29 names LESSTHANOREQUALTO");
		Check(std::string_view(bridge::expression_type_to_string(30)) == "GREATERTHANOREQUALTO",
		      "30 names GREATERTHANOREQUALTO");
		Check(std::string_view(bridge::expression_type_to_string(37)) == "DISTINCT_FROM", "37 names DISTINCT_FROM");
		Check(std::string_view(bridge::expression_type_to_string(40)) == "NOT_DISTINCT_FROM",
		      "40 names NOT_DISTINCT_FROM");

		// `year` and its aliases all name the first specifier.
		for (const char *alias : {"year", "yr", "y", "years", "yrs"}) {
			Check(bridge::try_date_part_specifier(as_slice(alias)) == 0, "year aliases name the year specifier");
		}
		Check(bridge::try_date_part_specifier(as_slice("month")) == 1, "month names the month specifier");
		Check(bridge::try_date_part_specifier(as_slice("nonsense")) == -1, "an unknown specifier is refused");

		// The catalog type round trip, including INVALID, which C++ names too.
		Check(bridge::catalog_type_from_string(as_slice("INVALID")) == 0, "INVALID names the invalid catalog type");
		Check(bridge::catalog_type_from_string(as_slice("Table")) == 1, "Table names the table catalog type");
		Check(bridge::catalog_type_from_string(as_slice("nonsense")) == -1,
		      "an unknown catalog name yields the -1 sentinel");

		// The optimizer table: count, name by index, name by discriminant, and
		// discriminant by name, the last of which the C++ shell calls once.
		Check(bridge::optimizer_count() == 45, "the optimizer table has 45 entries");
		Check(std::string_view(bridge::optimizer_name(0)) == "expression_rewriter",
		      "optimizer index 0 is the expression rewriter");
		Check(std::string_view(bridge::optimizer_name_for_discriminant(1)) == "expression_rewriter",
		      "optimizer discriminant 1 is the expression rewriter");
		Check(bridge::optimizer_discriminant_of_name(as_slice("expression_rewriter")) == 1,
		      "the expression rewriter names discriminant 1");
		Check(bridge::optimizer_discriminant_of_name(as_slice("nonsense")) == -1,
		      "an unknown optimizer name yields the -1 sentinel");
	}

	// --- the buffer module -----------------------------------------------
	{
		// Memory usage tracker create/update/flush.
		auto usage = ahirudb::buffer::memory_usage::new_memory_usage();
		usage->update_used_memory(0, 1000, 0);
		Check(usage->get_used_memory(ahirudb::buffer::memory_usage::TOTAL_MEMORY_USAGE_INDEX, true) == 1000,
		      "memory usage flush returns the charged bytes");
		ahirudb::buffer::memory_usage::delete_memory_usage(usage);

		// Checked conversion refuses an out-of-range value.
		Check(!ahirudb::buffer::cast::checked_i64_from_idx(~0ULL).has_value(),
		      "checked_i64_from_idx refuses an out-of-range value");
		Check(ahirudb::buffer::cast::checked_idx_from_i64(-1) == std::nullopt,
		      "checked_idx_from_i64 refuses a negative value");

		// Dead-node counter never wraps.
		auto counter = ahirudb::buffer::dead_nodes::new_dead_counter();
		counter->increment();
		counter->decrement();
		counter->decrement(); // saturates at 0
		Check(counter->get() == 0, "dead-node counter saturates instead of wrapping");
		ahirudb::buffer::dead_nodes::delete_dead_counter(counter);

		// Slice copy refuses an out-of-range length.
		std::uint8_t dst[4] = {0, 0, 0, 0};
		std::uint8_t src[2] = {1, 2};
		auto refused = ahirudb::buffer::block_copy::copy_block(
		    rs_std::SliceRef<std::uint8_t>(dst, 4), rs_std::SliceRef<const std::uint8_t>(src, 2), 4);
		Check(!refused.has_value(), "block_copy refuses a short source");
	}

	// --- the metadata module ---------------------------------------------
	// The pure arithmetic kernels of src/storage/metadata, reached through
	// the bridge. These cannot construct a real MetadataManager (that needs
	// a BlockManager/BufferManager), so the smoke test pins the arithmetic:
	// a wrong bitfield, a wrong block-boundary split, or a wrong transition
	// flag shows up here as a wrong answer rather than a silent no-op.
	{
		namespace pointer = ahirudb::storage::metadata::pointer;
		namespace bridge = ahirudb::storage::metadata::bridge;
		namespace writer = ahirudb::storage::metadata::writer;
		namespace reader = ahirudb::storage::metadata::reader;

		const std::uint64_t METADATA_BLOCK_SIZE = 4096;

		// --- pointer encoding -------------------------------------------------
		// get_disk_pointer packs block index (low 56 bits) and sub-block
		// index (high 8 bits); the shell reads them back through the
		// accessors the C++ original exposed.
		const auto meta = bridge::get_disk_pointer(7, 3, 100);
		Check(meta.get_block_id() == 7, "get_disk_pointer encodes the block id");
		Check(meta.get_block_index() == 3, "get_disk_pointer encodes the sub-block index");
		Check(meta.offset == 100, "get_disk_pointer carries its offset");
		Check(meta.is_valid(), "a freshly built pointer is valid");

		// to_block_pointer / from_block_pointer round-trip through a
		// metadata block size. The C++ shell uses this pair to move
		// between the on-disk MetaBlockPointer and the BlockPointer the
		// block manager understands.
		const auto block = bridge::to_block_pointer(meta, METADATA_BLOCK_SIZE);
		Check(block.has_value(), "to_block_pointer accepts an in-range pointer");
		Check(block.value().block_id == 7, "the round-trip keeps the block id");
		Check(block.value().offset == 3 * METADATA_BLOCK_SIZE + 100,
		      "the round-trip keeps the block-local offset");
		const auto back = bridge::from_block_pointer(block.value(), METADATA_BLOCK_SIZE);
		Check(back.has_value(), "from_block_pointer accepts the round-tripped block pointer");
		Check(back.value() == meta, "to/from_block_pointer are complementary");

		// An out-of-range sub-block index is refused by to_block_pointer.
		const auto bad_meta = bridge::get_disk_pointer(0, 64, 0);
		Check(!bridge::to_block_pointer(bad_meta, METADATA_BLOCK_SIZE).has_value(),
		      "to_block_pointer refuses a sub-block index past the 64-block capacity");

		// --- free-list bitmask ------------------------------------------------
		// free_blocks_to_integer packs a list of sub-blocks into a u64;
		// blocks_from_integer / free_blocks_from_integer unpack it again.
		// The C++ shell round-trips the free list through these on every
		// metadata block load and flush.
		const std::uint8_t free_blocks[] = {0, 63, 31};
		const std::uint64_t free_integer = bridge::free_blocks_to_integer(free_blocks, 3);
		Check((free_integer == ((1ULL << 0) | (1ULL << 63) | (1ULL << 31))),
		      "free_blocks_to_integer packs the listed sub-blocks");

		const auto unpacked = bridge::blocks_from_integer(free_integer);
		Check(unpacked.count == 3, "blocks_from_integer unpacks the same number of blocks");
		{
			std::uint64_t repacked = 0;
			for (std::uintptr_t i = 0; i < unpacked.count; i++) {
				repacked |= 1ULL << unpacked.data[i];
			}
			Check(repacked == free_integer, "blocks_from_integer round-trips the bitmask");
		}
		bridge::free_block_list(unpacked);

		// free_blocks_from_integer(0) is the empty list, not a zero bitmask.
		const auto empty = bridge::free_blocks_from_integer(0);
		Check(empty.count == 0, "free_blocks_from_integer(0) is empty");
		bridge::free_block_list(empty);

		// --- metadata block size ----------------------------------------------
		// The default storage block (262144) packs 64 metadata blocks of
		// 4096 bytes each, the value the C++ shell installs on startup.
		Check(bridge::get_metadata_block_size(262144) == 4096,
		      "the default storage block yields 4096-byte metadata blocks");
		Check(bridge::get_metadata_block_size(0) == 0, "a zero block size yields zero metadata blocks");

		// --- writer / reader plans are complementary --------------------------
		// A write plan and a matching read plan cover the same byte range
		// with the same number of block-boundary transitions. This is the
		// core invariant the block-chain streaming relies on.
		{
			const std::uint64_t cases[][3] = {
				{0, METADATA_BLOCK_SIZE, 100},                          // single block
				{0, METADATA_BLOCK_SIZE, METADATA_BLOCK_SIZE},          // exactly one block
				{0, METADATA_BLOCK_SIZE, METADATA_BLOCK_SIZE + 512},    // two blocks
				{0, METADATA_BLOCK_SIZE, 2 * METADATA_BLOCK_SIZE + 1024}, // three blocks
				{100, METADATA_BLOCK_SIZE, 50},                         // offset within block
				{100, METADATA_BLOCK_SIZE, METADATA_BLOCK_SIZE - 50},   // fills to end of block
			};
			for (const auto &c : cases) {
				const auto write_list = bridge::compute_write_data(c[0], c[1], c[2], METADATA_BLOCK_SIZE);
				const auto read_list = bridge::compute_read_data(c[0], c[1], c[2], METADATA_BLOCK_SIZE, 8);

				std::uint64_t write_total = 0, read_total = 0;
				std::uintptr_t write_transitions = 0, read_transitions = 0;
				for (std::uintptr_t i = 0; i < write_list.count; i++) {
					write_total += write_list.segments[i].copy_amount;
					if (write_list.segments[i].next_block_after) {
						write_transitions++;
					}
				}
				for (std::uintptr_t i = 0; i < read_list.count; i++) {
					read_total += read_list.segments[i].copy_amount;
					if (read_list.segments[i].next_block_after) {
						read_transitions++;
					}
				}
				Check(write_total == c[2], "the write plan covers the full write size");
				Check(read_total == c[2], "the read plan covers the full write size");
				Check(write_transitions == read_transitions,
				      "the write and read plans cross the same number of boundaries");
				bridge::free_write_segment_list(write_list);
				bridge::free_read_segment_list(read_list);
			}
		}

		// A write that fits in one block produces a single non-transitioning
		// segment; the matching read does too.
		{
			const auto write_list = bridge::compute_write_data(0, METADATA_BLOCK_SIZE, 100, METADATA_BLOCK_SIZE);
			const auto read_list = bridge::compute_read_data(0, METADATA_BLOCK_SIZE, 100, METADATA_BLOCK_SIZE, 8);
			Check(write_list.count == 1, "a single-block write is one segment");
			Check(!write_list.segments[0].next_block_after, "a single-block write does not transition");
			Check(read_list.count == 1, "a single-block read is one segment");
			Check(!read_list.segments[0].next_block_after, "a single-block read does not transition");
			bridge::free_write_segment_list(write_list);
			bridge::free_read_segment_list(read_list);
		}

		// A write that crosses one boundary produces two segments with one
		// transition each; the read plan matches.
		{
			const auto write_list = bridge::compute_write_data(0, METADATA_BLOCK_SIZE, METADATA_BLOCK_SIZE + 512, METADATA_BLOCK_SIZE);
			const auto read_list = bridge::compute_read_data(0, METADATA_BLOCK_SIZE, METADATA_BLOCK_SIZE + 512, METADATA_BLOCK_SIZE, 8);
			Check(write_list.count == 2, "a two-block write is two segments");
			Check(write_list.segments[0].next_block_after, "the first write segment transitions");
			Check(!write_list.segments[1].next_block_after, "the last write segment does not transition");
			Check(read_list.count == 2, "a two-block read is two segments");
			Check(read_list.segments[0].next_block_after, "the first read segment transitions");
			Check(!read_list.segments[1].next_block_after, "the last read segment does not transition");
			bridge::free_write_segment_list(write_list);
			bridge::free_read_segment_list(read_list);
		}

		// --- next-block state --------------------------------------------------
		// After NextBlock the writer lands at offset 8 (sizeof(idx_t)) with
		// the full metadata block as capacity; the reader clamps a sub-8
		// next_offset up to 8 on its first transition.
		const auto next_state = bridge::compute_next_block_state(METADATA_BLOCK_SIZE);
		Check(next_state.offset == 8, "NextBlock installs offset 8");
		Check(next_state.capacity == METADATA_BLOCK_SIZE, "NextBlock installs the metadata block capacity");

		const auto read_next = bridge::compute_read_next_block_state(20, METADATA_BLOCK_SIZE);
		Check(std::get<0>(read_next) == 20 && std::get<1>(read_next) == METADATA_BLOCK_SIZE,
		      "ReadNextBlock keeps a next_offset of 20");
		const auto read_next_clamped = bridge::compute_read_next_block_state(4, METADATA_BLOCK_SIZE);
		Check(std::get<0>(read_next_clamped) == 8, "ReadNextBlock clamps a sub-8 next_offset to 8");

		// --- chain link --------------------------------------------------------
		// The block chain is terminal only at the -1 sentinel.
		Check(bridge::chain_link_is_terminal(~0ULL), "the terminator is u64::MAX");
		Check(!bridge::chain_link_is_terminal(0), "block 0 is not terminal");
		Check(!bridge::chain_link_is_terminal(42), "block 42 is not terminal");

		// --- modified-bit tracking --------------------------------------------
		// ClearModifiedBlocks clears one bit; BlockHasBeenCleared reads it.
		const std::uint64_t modified = 0b111;
		Check(bridge::clear_modified_bit(modified, 1) == 0b101, "ClearModifiedBlocks clears bit 1");
		Check(bridge::bit_is_cleared(0b101, 1), "BlockHasBeenCleared sees bit 1 as cleared");
		Check(!bridge::bit_is_cleared(0b101, 0), "BlockHasBeenCleared sees bit 0 as set");
	}

	if (failures == 0) {
		std::printf("ahirudb bindings OK\n");
	}
	return failures == 0 ? 0 : 1;
}
