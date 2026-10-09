- Migrate c++ code in `src/common/vector` to rust.
- Remove unnecessary code from headers in `src/include/duckdb/common/vector`.

## Why this module

It is the most self-contained core subsystem in the tree and the most
pointer-arithmetic-heavy code DuckDB has, which is what the migration is for.
`string_vector.cpp`, `flat_vector.cpp`, `list_vector.cpp` and `fsst_vector.cpp`
all do manual allocation, `memcpy` and `reinterpret_cast`, and `StringHeap`
(`src/common/types/string_heap.hpp` plus its 31-line `string_heap.cpp`) dispatches
between inlined and heap-allocated strings behind a hand-rolled allocator. That is
where the safety is actually bought.

## Facts about the code, so they do not have to be re-derived

- ~3,000 LOC across the 14 `.cpp` files, the same scale as task 001
  (`src/logging`, 2,203 LOC). This is one task, not a programme.
- Of the `duckdb/**` headers these sources include, 58 of 60 are under
  `common/`; only two reach outside it, and none reach into `catalog`, `planner`
  or `parser`. Keep it that way — if a port needs a header from another layer,
  that is a finding, not a detail.
- They do include `common/vector_operations/vector_operations.hpp`, for the
  function-pointer registration table. `src/common/vector_operations` (2,417 LOC)
  stays in C++ for this task and calls across the Crubit boundary. That is cheap:
  vector operations run once per 2,048-row vector, not once per row.
- `buffer_ptr`/`shared_ptr` reference counting and `VectorCache` will appear on
  the far side of the boundary as Crubit opaque types. Expect an interface made
  of method calls rather than of data, and allow design time for it.
- Whether `StringHeap` itself comes across is a design decision, not a given. Say
  which way you went and why.
- Crubit does not bridge generics, `dyn`, or exceptions. Anything that needs one
  of those stays behind a wrapper, as the logging port did.

## Build

The module needs its own `duckdb_library` target, as `duckdb_logging` has for the
logging port: `crubit_copts()` from `//rust:crubit_bindings.bzl`, and
`DUCKDB_LIB_DEPS + ["//ahirudb:ahirudb_cc_api", "//ahirudb:ahirudb_static"]`.
Two traps that cost time in task 001: `duckdb_library`'s `deps` and `includes`
*replace* their defaults rather than extending them, and a private header needs to
be in `hdrs` for Bazel to put it in the sandbox at all.

## Acceptance

- `bazel build //...` and `bazel test //...` pass, all 227 targets.
- `//ahirudb:integration_test` and `//ahirudb:crubit_smoke_test` pass; the smoke
  test should grow to cover something from this module, so that C++-to-Rust
  breakage in it fails a test rather than surfacing as a wrong answer.
- The C++ call sites reach the Rust implementation through the generated
  bindings, and the original C++ implementations are gone — not still present and
  unused.
- No new compiler warnings, and `unsafe` is avoided where a safe form will do.