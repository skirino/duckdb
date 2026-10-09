- Migrate c++ code in `src/common/row_operations` to rust.
- Remove unnecessary code from headers in `src/include/duckdb/common/row_operations.hpp`.

## Why this one

655 LOC over 2 files, no unsafe constructs today, and nothing outside `common/`
that is not already handled by task 007. It is small because it is the last
piece of the row picture: it aggregates and scatters *over* the layout that
`common/types/row` defines.

## Notes

- Depends on task 007, and on `execution` and `catalog` headers. Both are
  unported, so the call-backs go the other way -- Rust calling C++ -- which is
  the shape Crubit makes easy. Keep them at the boundary.
- No dedicated C++ unit test and no unsafe constructs, so this is a
  correctness-and-clarity port rather than a safety one. Do not over-engineer it:
  port it as the structure it already is.
- At 655 LOC this is a short task. If it finishes early, `main/chunk_scan_state`
  (110 LOC) and `optimizer/matcher` (174 LOC) are Phase 3 modules that depend
  only on the base layer and could follow it -- but do not quietly widen the
  scope; the point of a small task is that it stays reviewable.

See `looping/PLAN.md` for the build and acceptance recipe shared by every task.
