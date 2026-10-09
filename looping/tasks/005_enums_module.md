- Migrate c++ code in `src/common/enums` to rust.
- Remove unnecessary code from headers in `src/include/duckdb/common/enums`.

## Why this one

1,593 LOC over 13 files, and it is read by everything that speaks text: the
parser's keyword lookup, storage serialization, and the C API's error and type
names. Nothing here is unsafe today, so the payoff is that a Rust
`EnumUtil` becomes the single answer for "what is this enum called" that the
parser and the wire format can both rely on.

## Notes

- `EnumUtil` is a large family of string<->enum converters generated from the
  enum lists under `src/include/duckdb/common/enums/`. Porting it means deciding
  whether the generator's output moves too -- see `scripts/generate_enum_util.cpp`
  and the note in `AGENTS.md` about regenerating after changing a source.
- It is the only Phase 1 module with a dependency outside `common/`
  (`main/http`, for HTTP method names). Either port that slice with it or have
  the Rust side return the names and keep the mapping on the C++ side; say which.
- No dedicated C++ unit test. `test_reset.cpp` and the extension demos touch it.
  A round-trip test per enum family is the cheap way to cover this properly.

See `looping/PLAN.md` for the build and acceptance recipe shared by every task.
