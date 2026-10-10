# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

## Overview

DuckDB is a high-performance analytical database system designed to be fast, reliable, portable, and easy to use. It is an analytical database management system with a rich SQL dialect, vectorized execution engine, and columnar storage format.

## Build Commands

### Basic Build

```bash
bazel build //...
```

## Testing

### Running Tests
```bash
bazel test //...
```

### Time-Limiting Queries

Use the `max_execution_time` setting (milliseconds, `0` = no limit) to abort a query that runs too long:

```sql
SET max_execution_time=5000;
SELECT count(*) FROM range(100000000) t1, range(1000) t2;
-- INTERRUPT Error: Query exceeded maximum execution time
```

The setting is `LOCAL` by default; use `SET GLOBAL max_execution_time=...` to apply it to all connections. From the shell:

```bash
build/reldebug/duckdb -c "SET max_execution_time=5000; SELECT ..."
```

The deadline is enforced at `ClientContext::InterruptCheck()` call sites, so it only fires once execution reaches one of them. Queries that check in frequently abort close to the limit, but time spent in planning or inside a long non-interruptible stretch is not bounded by it.


### Test File Format
Tests use the sqllogictest format (`.test` files). Example structure:
```sql
# name: test/sql/order/test_limit.test
# description: Test LIMIT keyword
# group: [order]

statement ok
CREATE TABLE test (a INTEGER, b INTEGER);

query I
SELECT a FROM test LIMIT 1
----
11

statement error
SELECT a FROM test LIMIT a
----
<REGEX>:Binder Error:.*not found.*
```

Test directives:
- `statement ok` - Statement should succeed
- `statement error` - Statement should fail
- `query I` - Query returning INTEGER column
- `query II` - Query returning two columns
- `----` - Separates query from expected results
- `<REGEX>:` - Expected error message pattern
- `require-env VAR` - Test requires environment variable

Slow tests should use `.test_slow` extension instead of `.test`.

Do not add `PRAGMA enable_verification` to tests - it should no longer be used in new tests.

## Code Formatting

```bash
make format-fix        # Format all code (clang-format + black)
make generate-files    # Generate files + format all code
```

Ensure you run formatting before committing.

## Architecture

### Query Execution Pipeline

```
SQL String
    ↓
[PARSER] - Uses a PEG parser to parse SQL into AST
    ↓
SQLStatement tree (ParsedExpression, TableRef objects)
    ↓
[PLANNER/BINDER] - Binds symbols to catalog, creates logical plan
    ↓
Logical Plan (LogicalOperator tree with bound Expressions)
    ↓
[OPTIMIZER] - Applies rule-based and cost-based optimizations
    ↓
Optimized Logical Plan
    ↓
[PHYSICAL PLAN GENERATOR] - Converts to physical operators
    ↓
Physical Plan (PhysicalOperator tree)
    ↓
[EXECUTOR] - Executes with vectorized, parallel pipelines
    ↓
Results
```

### Core Components

**Parser** (`src/parser/`)
- Converts SQL strings to Abstract Syntax Tree (AST)
- Uses a PEG-based parser
- The grammar is located in `*.gram` files and generated using `scripts/parser/build_grammar.sh`
- Outputs: `SQLStatement`, `ParsedExpression`, `TableRef` objects
- Key subdirectories: `expression/`, `statement/`, `tableref/`, `peg/`

For more details on adding new grammar, see the README located at `src/parser/peg/README.md`. 
Each new grammar rule must have a corresponding transformer rule, located at `src/parser/peg/transformer`.

**Planner** (`src/planner/`)
- Binds symbols to catalog entries and resolves types
- Creates logical query execution plan
- Key classes: `Binder`, `LogicalOperator`, bound `Expression` types
- Subdirectories: `binder/`, `expression/`, `subquery/`

**Optimizer** (`src/optimizer/`)
- Transforms logical plans without changing semantics
- Applies predicate pushdown, join ordering, expression rewriting, etc.
- Subdirectories: `join_order/`, `statistics/`, `rule/`, `pushdown/`

**Execution Engine** (`src/execution/`)
- Converts logical plan to physical plan and executes
- Push-based vectorized execution model
- Processes data in batches (typically 2048 rows)
- Key subdirectories: `operator/` (scan, join, filter, aggregate, etc.), `expression_executor/`

**Storage** (`src/storage/`)
- Manages persistent data storage and buffer management
- Block-based storage with compression
- Includes WAL (Write-Ahead Log) for durability
- Subdirectories: `buffer/`, `compression/`, `checkpoint/`, `table/`

**Catalog** (`src/catalog/`)
- Metadata management for tables, schemas, functions, types, etc.
- Single source of truth for database metadata
- Key classes: `Catalog`, `CatalogEntry`, `SchemaCatalogEntry`

**Transaction Manager** (`src/transaction/`)
- ACID transaction management with MVCC
- Coordinates concurrent access to data
- Key files: `transaction_manager.cpp`, `undo_buffer.cpp`, `wal_write_state.cpp`

**Parallel Execution** (`src/parallel/`)
- Multi-threaded execution with task scheduling
- Pipeline-based parallelism
- Key files: `executor.cpp`, `pipeline_executor.cpp`, `task_scheduler.cpp`

**Functions** (`src/function/`)
- Built-in function implementations
- Types: `scalar/`, `aggregate/`, `table/`, `window/`, `pragma/`

### Directory Structure

```
/duckdb
├── src/                   # Core C++ source code
│   ├── include/duckdb/   # Public headers
│   ├── parser/           # SQL parsing
│   ├── planner/          # Logical planning
│   ├── optimizer/        # Query optimization
│   ├── execution/        # Physical execution
│   ├── storage/          # Data storage
│   ├── catalog/          # Metadata management
│   ├── transaction/      # Transaction management
│   ├── parallel/         # Parallelization
│   ├── function/         # Built-in functions
│   ├── common/           # Shared utilities and types
│   └── main/             # Database/connection management
├── extension/            # In-tree extensions (parquet, json, icu, etc.)
├── test/                 # Test framework and test cases
│   ├── sql/             # SQL regression tests (.test files)
│   └── api/             # C/C++ API tests
├── tools/                # Shell, sqllogictest, Swift
├── benchmark/            # Benchmark suites (TPC-H, TPC-DS, etc.)
├── scripts/              # Build and utility scripts
└── third_party/          # Third-party dependencies
```

## Extensions

DuckDB supports two types of extensions:

**In-Tree Extensions** (in `extension/` directory):
- Extensions are located in-tree
- One `duckdb_library` per extension in the root `BUILD.bazel`
- Code can be edited directly and checked into the repository.

**Out-of-Tree Extensions**:
- Extensions are located in a separate git repository
- Each is pinned by a descriptor in `.github/config/extensions/<name>.json`; `scripts/extension_config.py` is the one reader
- When changes have to be made, they have to be made in patch files stored in `.github/patches`
- Before adapting an out-of-tree extension, changing its pinned revision, or modifying `.github/patches/extensions`, read and follow `.github/patches/extensions/AGENTS.md`.

`make sync_out_of_tree_extensions` clones each pinned extension and applies its patches. Building one
is a Bazel target that does not exist yet, so that step stops at the checkouts.

## Key Development Patterns

### Data Flow
- **Vectorized Processing**: Data processed in columnar batches (not row-by-row), typically 2048 rows per batch
- **Vector class**: Represents a columnar batch of data
- **ColumnBinding**: Unique identifier `(table_index, column_index)` for columns throughout planning/execution

### Expression Types
- `ParsedExpression` - From parser, unbound
- `Expression` - Bound with type information
- `ExpressionExecutor` - Vectorized execution of expressions

### Memory Management
- Prefer `unique_ptr<T>` for exclusive ownership
- Use `shared_ptr<T>` only when necessary
- `optional_ptr<T>` for nullable references, `reference<T>` for non-nullable references
- Never use raw pointers

### Type System
- `LogicalType` - Abstract data type representation
- Type promotion rules in `src/function/cast_rules.cpp`
- Custom types supported via extension system

### Common Patterns
- **Visitor Pattern**: For tree traversal (e.g., `LogicalOperatorVisitor`, `ExpressionIterator`)
- **Factory Pattern**: `Deserialize()` methods for object creation
- **Class Hierarchy**: Base classes like `*Operator`, `*Entry`, `*Expression` with typed subclasses

## Vector API

Vectors carry their own size, and element access goes through typed iterators (reading) and typed
writers (writing). **The old-style vector loops are deprecated and must not be used in new code:**

```cpp
// DON'T - legacy pattern: ToUnifiedFormat + manual sel/validity indexing
auto idx = vdata.sel->get_index(i);
if (!vdata.validity.RowIsValid(idx)) {
	result_validity.SetInvalid(i);
}
result_data[i] = data[idx] + 1;
```

```cpp
// DO - iterator + writer
auto input_values = input.Values<int64_t>();
auto writer = FlatVector::Writer<int64_t>(result, count);
for (auto entry : input_values) {
	if (!entry.IsValid()) {
		writer.WriteNull();
		continue;
	}
	writer.WriteValue(entry.GetValue() + 1);
}
```

Exceptions: element-wise scalar functions should still use the executors (`UnaryExecutor`,
`BinaryExecutor`, `GenericExecutor`, ...), and type-erased code that does not know the C++ type at
compile time (hash tables, sorting, row layout, storage) still uses `ToUnifiedFormat(data)`.

Headers: `duckdb/common/vector/vector_iterator.hpp`, `duckdb/common/vector/vector_writer.hpp`.

### Reading: `Vector::Values<T>()`

Works on any vector type - never `Flatten()` just to read. Build it once, outside the loop (it
materializes a `UnifiedVectorFormat`), then iterate or index into it. It is not copyable; pass it to
helpers as `const VectorIterator<T> &`.

```cpp
auto values = vec.Values<string_t>();
for (auto entry : values) {   // or: values[i], values.size(), values.CanHaveNull()
	entry.IsValid();          // per-row validity
	entry.GetValue();         // value (asserts validity); GetValueUnsafe() skips the assert
	entry.GetIndex();         // row index
}
```

Also `vec.ValidValues<T>()` (skips NULL rows entirely) and `vec.Validity()` (validity only).

Nested types have iterator specializations, so nested reads need no child/offset arithmetic:

```cpp
// STRUCT(BIGINT, VARCHAR)
auto structs = vec.Values<VectorStructType<int64_t, string_t>>();
for (auto entry : structs) {
	entry.IsValid();                         // top-level struct NULL
	auto a = entry.GetChildValue<0>();       // per-child ValueEntry
	entry.ForEach([&](auto &child) { ... }); // all children in declaration order
}

// LIST(BIGINT) - nests recursively, e.g. VectorListType<VectorListType<double>>
auto lists = vec.Values<VectorListType<int64_t>>();
for (auto entry : lists) {
	entry.GetListLength();
	entry.GetChildValue(idx);
	for (auto child : entry.GetChildValues()) { ... }
}

// VARIANT - traverses shredded and unshredded variants without unshredding
VectorIterator<VectorVariantType> variants(vec);
```

The `VectorListType<T>` iterator does not accept a `DICTIONARY_VECTOR` source - flatten first if the
list vector may be a dictionary.

### Writing: `FlatVector::Writer<T>()`

Writers are push-based: rows are written in order, and the writer sets the result vector's size
itself (no `SetSize` / `SetCardinality` call). Use the `(result, count, offset)` overload to append
at an offset. On destruction the writer asserts that exactly `count` rows were written - call
`Truncate()` if you deliberately write fewer.

```cpp
auto writer = FlatVector::Writer<int64_t>(result, count);
writer.WriteValue(value);
writer.WriteNull();
```

`VectorWriter<string_t>` adds `WriteStringRef(val)` (reference without copying into the vector heap -
keep it alive via `StringVector::AddHeapReference`), `WriteEmptyString(len)` and `GetHeap()`.

Nested writers mirror the iterators:

```cpp
// STRUCT: WriteNull() propagates to every child, keeping the children in sync
auto writer = FlatVector::Writer<VectorStructType<int64_t, string_t>>(result, count);
writer.WriteValue([&](auto &a_writer, auto &b_writer) { ... });
writer.ForEach([&](auto &child_writer) { ... });  // same operation for every child

// LIST, known length per row
for (auto &child_writer : list_writer.WriteList(n)) {
	child_writer.WriteValue(...);
}

// LIST, unknown length - grows on demand, finalized when the returned writer goes out of scope
auto list = list_writer.WriteDynamicList();
list.WriteElement().WriteValue(...);
```

If the element type is not known at compile time, `FlatVector::Writer<list_entry_t>` has a
`WriteDynamicList()` returning a `DynamicListAppender` (`Append(source, sel, ...)` / `AppendNulls`).
For the rare case where rows cannot be written in order, `FlatVector::ScatterWriter<T>(vec)` supports
random access (`writer[idx] = value`, `writer.SetInvalid(idx)`).

## Coding Guidelines (Key Points)

### C++ Style
- Use tabs for indentation, spaces for alignment
- Lines should not exceed 120 columns (run formatter)
- Use `[u]int(8|16|32|64)_t` instead of `int`, `long`, etc.
- Use `idx_t` instead of `size_t` for offsets/indices/counts
- Use `const` references for non-trivial objects
- Use C++11 range-based for loops when possible
- Always use braces for if statements and loops
- Never use `const_cast`

### Comment Conventions

Try to keep comments short. In general, comments should be one short line. Only in exceptional situations should comments be more than one short line. Code should be mostly self-descriptive and too many large comments make code harder to read and understand.

Avoid adding comments specific to how a change was made to the code that relates to a specific issue. For example, a comment like "add +1 to fix an off-by-one error" is not relevant to understanding the code. Such comments related to specific issues that were addressed belong in a PR description or commit message, not in the code itself.

### Naming Conventions
- **Files**: `snake_case` (e.g., `abstract_operator.cpp`)
- **Types**: `PascalCase` (e.g., `LogicalOperator`)
- **Variables**: `snake_case` (e.g., `chunk_size`)
- **Functions**: `PascalCase` (e.g., `GetChunk`)

### Class Layout
```cpp
class MyClass {
public:
    MyClass();
    int my_public_variable;

public:
    void MyFunction();

private:
    void MyPrivateFunction();

private:
    int my_private_variable;
};
```

### Error Handling
- Use exceptions for query-terminating errors (parser error, table not found, out-of-memory, etc.)
- Use return values for errors that are recoverable during a query
- Use `D_ASSERT` for programmer errors (never triggered by user input)
- Assert liberally with clear comments

### Testing Requirements
- Prefer sqllogictest framework (`.test` files) over C++ tests
- Test with different types (numerics, strings, nested types)
- Test unexpected/incorrect usage, not just happy path
- Slow tests should use `.test_slow` extension
- All tests must pass before submitting PR (`make allunit`)
- Aim for high code coverage

## Navigation Tips

### Finding Components
- Entry point: `src/main/database.cpp` (DatabaseInstance)
- Query execution coordinator: `src/main/client_context.cpp`
- SQL parsing: `src/parser/parser.cpp`
- Logical planning: `src/planner/planner.cpp`
- Optimization orchestration: `src/optimizer/optimizer.cpp`
- Physical plan generation: `src/execution/physical_plan_generator.cpp`
- Execution orchestration: `src/parallel/executor.cpp`

### Searching the Codebase
- Use `grep` or `ripgrep` for code search
- Function definitions typically in `.cpp` files
- Class declarations in `src/include/duckdb/` headers
- Test cases in `test/sql/` by functionality

### Understanding a Feature
1. Find test cases in `test/sql/` to see usage examples
2. Trace from parser → planner → optimizer → execution
3. Look for corresponding `*Statement`, `*Operator`, `*Expression` classes
4. Check function registration in catalog

### Modifying Generated Files
Some files are auto-generated. After modifying their sources, run:
```bash
make generate-files
```
This regenerates:
- C API bindings
- Function registration
- Settings
- Serialization code
- Storage info
- Metric enums
- Enum utilities

## Documentation

- Main docs: https://duckdb.org/docs/
- Development docs: https://duckdb.org/dev/
- Build guide: https://duckdb.org/docs/dev/building/overview
- Testing docs: https://duckdb.org/dev/testing

## Important Files

- `Makefile` - The driver CI and developers call; every target shells out to `bazel`
- `BUILD.bazel` - The build itself: every target in the tree
- `duckdb_build_config.bzl` - The includes, defines and flags every library shares
- `.bazelrc` - Toolchain flags, and the four build types as configs
- `CONTRIBUTING.md` - Contribution guidelines
- `test/README.md` - Testing documentation
- `extension/README.md` - How an extension becomes a target
- `scripts/format.py` - Code formatter
- `scripts/generate_*.py` - Code generation scripts

## Migration from C++ to Rust

The codebase is being migrated to Rust for memory- and thread-safety. For the
migration please keep the following in mind:

- The Rust code lives under `ahirudb`, which holds the one `ahirudb` crate. The
  single-crate structure is intentional: Crubit's C++/Rust interop is
  significantly simpler with one crate, so do not split it into several.
- Keep the structure of the existing C++ code in the Rust code. Do not
  drastically restructure it; stay faithful to the original. The rust code
  should behave basically the same way as the C++ code: should do the same thing
  with the same time and space complexity.
- Also port C++ unit tests to Rust. SQL-level tests do not need porting.
- Do not write `extern "C"` glue. Crubit generates the bindings; call the
  generated function instead. When porting a C++ function, change its C++ callers
  to call the Rust one and remove the original C++ implementation.
- Try to eliminate `unsafe` blocks, and do not introduce new compiler warnings.
- Add a Rust library only when it solves a significantly difficult problem. Do
  not introduce one just to simplify something that is already simple.
- Run `cargo fmt` in `ahirudb/` when done editing Rust code.

### What Crubit cannot bridge

Crubit does not bridge generics, `dyn` or any other dynamic dispatch, or
exceptions. References are supported only as function parameters, and only in
cases it can prove do not add aliasing undefined behaviour to the C++ caller.
Enums bridge as opaque objects with explicit conversions, not as C++ enums.

**The trap: a missing binding does not fail the build.** When Crubit cannot
generate bindings for an item, it leaves the item out of the generated header and
writes a comment in its place saying why. A port can therefore compile, link,
and pass every test while a function silently does nothing. So after generating,
read the header — `bazel-bin/ahirudb/crubit_include/crubit/ahirudb.h` — and look
for `Error generating bindings for`. Check this on every task; it is the failure
mode the smoke test cannot see.

`#[crubit_annotate::must_bind]` turns it into a build failure instead, and is
the right thing on anything crossing the bridge. It is not wired up yet: the
dependency is commented out in `ahirudb/Cargo.toml` and points at a build
tree that does not exist here, and the crate is not copied into
`@crubit_toolchain`. Wiring it is a small task of its own.

When something cannot be bridged, wrap it rather than dropping it. A `Vec<T>`
is not supported; `pub struct MyVec(Vec<T>)` is.

### Quality bar

**`unsafe`.** Every `unsafe` block and every `unsafe impl` carries a `// SAFETY:`
comment saying what invariant makes it sound. New modules start with
`#![deny(unsafe_code)]`, dropped only with a documented reason. Zero unsafe is the
goal, not the rule — the FFI layer cannot avoid some. The crate is at 12 uses
today, 4 of them bare `unsafe impl Send`/`Sync` with empty bodies in
`ahirudb/src/logging/logger.rs` and `log_storage.rs`.

**Panics.** A panic must not reach the boundary. Anything crossable returns a
`Result` or an explicit error value. Unwinding into a C++ frame is undefined
behaviour, and in practice a panic crossing the bridge aborts the process rather
than failing the query.

**Thread safety.** The migration exists for memory and thread safety, so a type
reachable from more than one thread must be `Send + Sync` by construction — locks,
atomics, owned data — and not by assertion. An `unsafe impl Send`/`Sync` is a
last resort that has to say which invariant makes it hold. The existing two are
the problem to fix, not the pattern to copy.

**Lints.** `[lints]` in `Cargo.toml` is inert here: the `rust_library` is built by
rules_rust, which does not read the manifest. Crate-wide lints go in `lib.rs` as
crate attributes, which both build paths honour.

### How C++ reaches Rust

C++/Rust interop goes through Crubit, which generates a C++ header and a staticlib
from the crate. Crubit has Bazel rules of its own (`@rules_crubit`), but they are
Linux-only and build LLVM from sources, so this build uses the same generator
Crubit's own upstream integration drives, `cargo cpp_api_from_rust`:

- `MODULE.bazel` pins the Crubit revision, and `rust/crubit_toolchain.bzl` builds
  the generator once and exposes it as `@crubit_toolchain`.
- `rust/crubit_bindings.bzl` has the `crubit_bindings` rule, which runs the
  generator over a crate. `ahirudb/BUILD.bazel` uses it; the output is
  `//ahirudb:ahirudb_cc_api` (headers) and `//ahirudb:ahirudb_static` (the
  staticlib C++ links).
- `crubit_copts()` in the same file is what a translation unit including the
  generated headers needs: C++20, and two warnings off.
- `//:duckdb_logging` is the only library that does this today. Its private
  `log_rust_bridge.hpp` is deliberately not under `src/include`, because only
  `src/logging` should see the generated bindings.

`//ahirudb:crubit_smoke_test` checks that C++ can call into the crate at all, and
`//ahirudb:integration_test` runs the crate's own tests. Run both alongside the
C++ suite: `bazel test //...`.

The generator compiles against rustc internals, which upstream renames between
Rust releases, so the nightly it is built with is pinned (`nightly-2026-09-29`,
the version Crubit pins as its own `rust_version`) rather than tracking
`nightly`.
