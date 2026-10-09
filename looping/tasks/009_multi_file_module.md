- Migrate c++ code in `src/common/multi_file` to rust.
- Remove unnecessary code from headers in `src/include/duckdb/common/multi_file.hpp`.

## Why this one

3,359 LOC over 8 files: the buffered reader and writer that sit between blocks on
disk and blocks in memory. It is the plumbing every storage module goes through,
so getting it right early pays off across Phase 4.

## Notes

- No unsafe constructs in the module itself today -- the danger is in its
  callers, which pass `data_ptr_t` and a length. The win here is making the
  interface take slices so those callers cannot get it wrong. Expect the task to
  touch more call sites than the module.
- Three substantial C++ tests to port and extend: `test/common/test_file_system.cpp`
  (1,384 LOC), `test_async_file_writer.cpp` (2,181), `test_caching_file_system_wrapper.cpp`
  (905). This is the best-tested module in Phase 1; use that.
- Depends on `execution` headers, unported. Keep the seam on the C++ side.
- It reads and writes through `common/serializer`, so it must follow task 006.

See `looping/PLAN.md` for the build and acceptance recipe shared by every task.
