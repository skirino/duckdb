- Migrate c++ code in `src/storage/metadata` to rust.
- Remove unnecessary code from headers in `src/include/duckdb/storage/metadata.hpp`.

## Why this one

691 LOC over 3 files, the smallest of the storage modules, and it closes Phase 1
by making the on-disk metadata format readable and writable from Rust. Twelve
unsafe-ish constructs in 691 lines is the highest density in the phase -- this
module is nearly all hand-rolled pointer arithmetic over metadata blocks.

## Notes

- Reads and writes blocks through `common/serializer` (task 006), so it must
  follow it. It is the first thing that exercises task 006's interface from
  outside, so check that the seam is as narrow as it should be.
- No dedicated C++ unit test; `test/api/test_reset.cpp` touches it and the SQL
  suite covers it through reading and rewriting the catalog. Round-trip tests
  over real metadata blocks -- write, read back, compare -- are cheap and belong
  here, because a corruption here is silent.
- The format is on-disk and must not change. A port that alters the layout breaks
  every existing database file, so byte-for-byte compatibility is an acceptance
  criterion, not a nice-to-have.

See `looping/PLAN.md` for the build and acceptance recipe shared by every task.
