- Migrate c++ code in `src/common/allocator` to rust.
- Remove unnecessary code from headers in `src/include/duckdb/common/allocator.hpp`.

## Why this one

454 LOC over 3 files, no dependencies outside `common/`, and everything that
allocates goes through it -- so it is the right place to get the handle semantics
right before task 007 (row collections) and task 011 (the buffer pool) build on
them.

## Notes

- `Allocator`/`AllocatorHandle` is a shared, reference-counted handle with debug
  tracking and a thread-local block cache. Handle lifetime across the boundary is
  the same problem task 002 hit with `buffer_ptr`, at a smaller scale, so reuse
  whatever shape it settled on.
- Four adjacent C++ tests to port and extend: `test_allocator_debug_info.cpp`,
  `test_block_allocator_tls.cpp`, `test_column_data_allocator.cpp`,
  `test_custom_allocator.cpp`.
- There are four unsafe-ish constructs in the module today. They are the point of
  the task; a port should leave none.

See `looping/PLAN.md` for the build and acceptance recipe shared by every task.
