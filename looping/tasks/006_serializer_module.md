- Migrate c++ code in `src/common/serializer` to rust.
- Remove unnecessary code from headers in `src/include/duckdb/common/serializer.hpp`.

## Why this one

3,618 LOC over 11 files, no dependencies outside `common/`, and it is the
archetypal target for this migration: read and write primitives that take a
`data_ptr_t` and a length and do pointer arithmetic across them. Eight
unsafe-ish constructs today; a port should leave none, and should express every
range as a slice.

## Notes

- The C++ API takes `data_ptr_t` plus separate length arguments and trusts them.
  The Rust side should take a slice and let the length come from it, so an
  out-of-range read is a panic rather than a wild pointer. Watch for call sites
  that pass a length deliberately larger than what they then read.
- `test/serialize/serialization_test.cpp` is only ~300 LOC, so coverage here is
  mostly the SQL suite. Round-trip tests per primitive (varint, string, list,
  struct) are cheap and worth writing.
- Task 012 (`storage/metadata`) reads and writes blocks through this module, so
  doing it before that one matters.

See `looping/PLAN.md` for the build and acceptance recipe shared by every task.
