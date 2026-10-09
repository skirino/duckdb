- Migrate c++ code in `src/common/sort` to rust.
- Remove unnecessary code from headers in `src/include/duckdb/common/sort`.

## Why this one

3,619 LOC over 8 files, and sort is where a bug is both likely and expensive:
radix passes over row data with hand-computed offsets. Seven unsafe-ish
constructs today.

## Notes

- Depends on `common/vector` (task 002) and reads rows laid out by
  `common/types/row` (task 007), so it must follow both.
- No dedicated C++ unit test; `ORDER BY` coverage in the SQL suite is broad but
  slow to localise. Sort comparison logic (comparator dispatch, payload ordering)
  is separable from the memory movement and is the part worth unit-testing first.
- Performance is the risk here rather than correctness. The port must not turn a
  radix pass into a per-row bounds check on the hot path; if it does, that is a
  finding to report rather than to accept. `benchmark/` has order-by
  benchmarks to measure against.

See `looping/PLAN.md` for the build and acceptance recipe shared by every task.
