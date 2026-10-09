- Migrate c++ code in `src/storage/buffer` to rust.
- Remove unnecessary code from headers in `src/include/duckdb/storage/buffer`.

## Why this one

1,103 LOC over 5 files, and it is `BufferManager`/`BufferPool`/`BufferHandle` --
the thing that decides what memory DuckDB is allowed to hold and when it is
released. That is both a memory-safety concern and the state the transaction and
parallel phases will be measured against later.

## Notes

- Five C++ tests to port and extend: `test_buffer_manager.cpp`,
  `test_buffer_pool_eviction.cpp`, `test_buffer_pool_reservation.cpp`,
  `test_file_buffer_handle_group.cpp`, `test_stream_result_buffer.cpp`. This is
  the best-covered module in the phase, and the tests assert on eviction and
  reservation accounting, so they pin down the semantics rather than just the
  API.
- `BufferHandle` is a reference-counted handle over a shared block -- the same
  lifetime problem task 002 had with `buffer_ptr` and task 004 with
  `AllocatorHandle`. Three modules with the same shape is a sign the answer
  belongs in the bridge rather than in each module.
- Eight unsafe-ish constructs today; a port should leave none.
- Depends on `main/settings` headers (the memory limit), which are not ported
  until Phase 7. Keep that seam narrow.

See `looping/PLAN.md` for the build and acceptance recipe shared by every task.
