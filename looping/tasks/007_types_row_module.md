- Migrate c++ code in `src/common/types/row` to rust.
- Remove unnecessary code from headers in `src/include/duckdb/common/types/row`.

## Why this one

4,601 LOC over 7 files, and it is where rows are laid out by aggregate: offsets
and validity are computed by hand and every access is pointer arithmetic relative
to a base. This is the second-highest memory-safety payoff in Phase 1, after
`common/vector`.

## Notes

- Largest single module in this phase. It is seven files of dense layout code, so
  if it does not fit one iteration, split by concept -- `RowLayout` and friends
  first, then `RowDataCollection` -- rather than by file count, and add a task
  file for the remainder.
- Nine unsafe-ish constructs today. They are the deliverable.
- No dedicated C++ unit test; coverage is the SQL suite plus the Arrow tests
  (`test/arrow/arrow_roundtrip.cpp` and friends exercise row layout through the
  Arrow bridge). Add layout tests, since the existing coverage is indirect and
  slow to localise.
- It depends on `src/storage`, `src/main` and `src/parallel` headers, none of
  which are ported. Those call-backs are the main design cost here; keep them
  narrow and on the C++ side.
- Task 008 (`common/row_operations`) operates on rows and must follow this one.

See `looping/PLAN.md` for the build and acceptance recipe shared by every task.
