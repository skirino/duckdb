- Migrate c++ code in `src/common/exception` to rust.
- Remove unnecessary code from headers in `src/include/duckdb/common/exception.hpp`.

## Why this one is early and small

182 LOC over 4 files, the smallest module in Phase 1, and it is here to settle a
question rather than to move much code: **how does an exception cross the Crubit
boundary?** Every later module throws `Exception`/`IOException`/
`InternalException`, and the logging port did not have to answer it because the
bridge it needed could fail by returning an empty result instead.

Solve it here, on the smallest possible module, so the other ten tasks in this
phase can be written against the answer.

## Notes

- DuckDB throws a small hierarchy of concrete subclasses carrying a formatted
  message, and callers catch by base type. Crubit does not bridge exceptions, so
  the design question is what Rust returns instead, and where the message is
  re-thrown on the C++ side. That seam is the deliverable of this task.
- No dedicated C++ unit test exists for this module; the whole SQL suite catches
  regressions, because nearly every error path in DuckDB runs through here.
- `src/include/duckdb/common/exception.hpp` is included almost everywhere, so it
  must not pull in Rust bindings. Keep the Rust side behind the generated header,
  the way `log_rust_bridge.hpp` is kept out of `src/include`.

See `looping/PLAN.md` for the build and acceptance recipe shared by every task.
