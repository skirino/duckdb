# The C++ to Rust migration, in order

## Ordering rule

Port the base layer first, and work outward.

`common/*` has by far the highest ratio of "depended on by" to "depends on" in
the tree, so it is both where a Rust foundation pays off most and what makes
every later port cheaper and less risky. Crubit lets Rust call back into C++, so
a module is never *blocked* by an unported dependency -- but the further out you
go, the more of those call-backs you accumulate, and a module ported with three
unported dependencies behind it is much harder to review than one with none.

Within that, two other things decide order:

- **Unsafe-construct density.** The point of the migration is memory safety, so
  the modules doing manual allocation, `memcpy` and pointer arithmetic go first.
- **Size.** A task should be one loop iteration. 001 (`src/logging`) was 2,203
  LOC and 002 (`src/common/vector`) is 2,963; anything much over ~4,000 needs
  splitting into several tasks, which the roadmap marks.

## Task file conventions

- `tmp/NNN_${task_title}/task.md`, numbered in migration order. The number only orders
  the queue.
- Each task file states the goal, the scope as paths, facts specific to that
  module, and acceptance. Anything true of every port lives here instead.
- A module's headers stay out of `src/include` if it pulls in Rust bindings, and
  private headers need to be in `hdrs` to reach the sandbox.

## How a port is built and verified

The same shape for every task:

- The module gets its own `duckdb_library` target in `BUILD.bazel`, with
  `copts = crubit_copts()` and
  `deps = DUCKDB_LIB_DEPS + ["//ahirudb:ahirudb_cc_api", "//ahirudb:ahirudb_static"]`.
  Both `deps` and `includes` on `duckdb_library` *replace* their defaults rather
  than extending them, which is the trap task 002 hit.
- The Rust code goes in `ahirudb/src/<module>/`, keeping the C++ file structure,
  per `AGENTS.md`. Crubit only -- no hand-written `extern "C"`.
- `bazel build //...` and `bazel test //...` pass, all 227 targets.
- `//ahirudb:integration_test` and `//ahirudb:crubit_smoke_test` pass, and the
  smoke test grows to cover something from the module, so that a broken bridge
  fails a test instead of producing a wrong answer.
- The original C++ implementations are deleted, not left unused, and the C++
  callers reach the Rust version through the generated bindings.
- No new compiler warnings.

Two things that have not been solved yet and that an early task should expect to
have to solve: **how an exception crosses the Crubit boundary** (task 003), and
**how a reference-counted handle looks on the far side** (task 002 hit this with
`buffer_ptr` and `VectorCache`). Solving them in small tasks, once, is the reason
those tasks are first.

## The order

LOC and file counts are of `src/<module>/**` including its own headers. "deps"
is how many of its `duckdb/**` includes fall outside its own module, which is a
rough proxy for how much unported code it reaches across to.

### Phase 1 - Safety core

The buffer and primitive layer. Nothing outside `common/` has to be ported first.

| task | module | LOC | cpp | deps | notes |
|---|---|---|---|---|---|
| 002 | `common/vector` | 2,963 | 14 | 2 | vector types, string heap; `vector_operations` stays C++ |
| 003 | `common/exception` | 182 | 4 | 0 | smallest module here; owns the error-crossing design |
| 004 | `common/allocator` | 454 | 3 | 0 | `AllocatorHandle`, debug info, TLS |
| 005 | `common/enums` | 1,593 | 13 | 1 | `EnumUtil` string<->enum; parser, serialization, C API all read it |
| 006 | `common/serializer` | 3,618 | 11 | 0 | read/write primitives over `data_ptr_t`; the archetypal pointer arithmetic |
| 007 | `common/types/row` | 4,601 | 7 | 4 | `RowLayout`, `RowDataCollection`; no dedicated unit test |
| 008 | `common/row_operations` | 655 | 2 | 2 | aggregate/scatter over rows; must follow 007 |
| 009 | `common/multi_file` | 3,359 | 8 | 1 | read/write buffer plumbing; `test_file_system.cpp`, `test_async_file_writer.cpp` cover it |
| 010 | `common/sort` | 3,619 | 8 | 0 | radix sort over rows; needs 002 |
| 011 | `storage/buffer` | 1,103 | 5 | 1 | `BufferManager`/`BufferPool`; five candidate unit tests exist |
| 012 | `storage/metadata` | 691 | 3 | 0 | metadata block read/write; highest unsafe density in this phase |

### Phase 2 - Data model

`common/types` root is 15,869 LOC over 31 files and is depended on by twenty
modules. Port it in pieces, in this order, so the pieces most depended on land
first: `LogicalType`, then `Value`, then `DataChunk` and the validity mask, then
`common/types/column` (2,608) and `common/types/variant` (929).

### Phase 3 - Leaf services

Depend only on Phases 1-2, so they can go in any order; listed small first.

`parser/parsed_data` (2,760) . `parser/tableref` (935) . `parser/query_node`
(993) . `parser/constraints` (197) . `parser/statement` (1,010) . `parser/expression`
(1,645) . `parser` root (3,003) . `common/operator` (3,787) . `common/progress_bar`
(656) . `common/crypto` (257) . `main/chunk_scan_state` (110) . `main/buffered_data`
(529) . `storage/external_file_cache` (1,501) . `optimizer/matcher` (174) .
`optimizer/pullup` (212) . `optimizer/statistics` (2,637) . `common/tree_renderer`
(2,295; needs `execution/operator` and `main/profiler`, so late despite being
small).

### Phase 4 - Storage

`storage/compression` (10,435 / 36) is the largest single win in the tree for
memory safety and must be split -- by algorithm, one task each. Then
`storage/statistics` (3,919) . `storage/serialization` (5,457) . `storage/checkpoint`
(446) . `storage/table` (16,023 / 26, split) . `storage` root (13,054 / 26, split).

### Phase 5 - Catalog, planner, optimizer

`catalog/default` (1,488) . `catalog/catalog_entry` (3,839) . `catalog` (4,581) .
`planner/expression` (1,728) . `planner/expression_binder` (1,926) .
`planner/operator` (2,790) . `planner/filter` (2,996) . `planner/subquery` (4,871) .
`optimizer/pushdown` (1,638) . `optimizer/rule` (4,527) . `optimizer/join_order`
(4,987) . `optimizer/relation_statistics` (888) .
`optimizer/compressed_materialization` (528) . `planner/binder` (17,181 / 75,
split) . `planner` (8,622 / 25, split) . `optimizer` (24,289 / 53, split).

### Phase 6 - Execution, functions, concurrency

The widest blast radius: these are what every other module calls. Port them once
the layers beneath are in Rust, so the call-backs they need are few.
`execution/expression_executor` (1,193) . `execution/physical_plan` (4,066) .
`execution/operator/helper` (2,479) . `execution/operator/scan` (1,099) .
`execution/operator/set` (4,454) . `execution/operator/aggregate` (4,655) .
`execution/operator/join` (11,219 / 17, split) . `execution/operator/persistent`
(8,189 / 11, split) . `execution/operator/csv_scanner` (8,622 / 27, split) .
`execution` root (8,278 / 14) . `execution/index` (5,844 / 22) .
`function/scalar/*` and `function/table` and `function` root, in the order their
own subdirectories are useful: `function/variant` (371) . `function/cast` (6,448) .
`function/aggregate` (2,360) . `function/window` (7,262) . `function/scalar/*`
(string 4,382, operator 2,965, variant 1,945, system 1,299, struct 1,269, list 955,
compressed_materialization 729, comparison 483, geometry 476, generic 425, date
342, sequence 205) . `function/scalar` root (3,201) . `function` root (5,716 / 22) .
`parallel` (7,271 / 23) . `transaction` (2,679 / 12) -- the last two for the
thread-safety argument, which is the whole reason for the exercise.

### Phase 7 - Frontend, API, interop

`main/settings` (2,232) . `main/http` (1,712) . `main/secret` (1,747) .
`main/profiler` (307) . `common/arrow` (2,387) . `main/extension` (3,513) .
`common/adbc` (4,530) . `main` (10,902 / 37, split) . `main/relation` (1,759 / 27,
split) . `main/capi` (20,213 / 66, split last).

## Not in this plan

**`parser/peg` (53,049 / 63).** Not a translation job. 40 `.gram` files under
`src/parser/peg/grammar/` are compiled by pegRL into the checked-in
`src/include/duckdb/parser/peg/compiled_grammar.hpp`, and
`scripts/parser/generate_transformer.py` generates the `transform_*.cpp`
trampolines. There is no hand-written parser to port: it would mean writing a
parser generator in Rust first, with the C++ one still in place until it is ready.
That is a project of its own and should be scoped separately, not run as a loop
task.

**`src/main/extension` loadable extensions.** The demo extensions under
`test/extension/` are C++ by design and exercise the loadable path; they stay.

## Things that will need solving along the way

- Exceptions across the boundary (task 003 should settle it).
- Reference-counted handles: `buffer_ptr`, `VectorCache`, `StringHeap` lifetimes.
- Crubit has no generics, no `dyn`, and no exceptions. Anything needing one stays
  behind a wrapper, as the logging port did.
- `//ahirudb:crubit_smoke_test` is the only thing that catches a broken bridge.
  Every task should add to it; if a task cannot, that is worth knowing early.
