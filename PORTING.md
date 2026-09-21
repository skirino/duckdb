# Migration from C++ to Rust

Currently the codebase is bing migrated to Rust for memory- and thread-safety.
For the migration please keep the folloings in mind:

- The rust code is located under `ahirudb` directory. The directory contains `ahirudb` crate.
  (The actual source files are under `ahirudb/src/`.)
  The single crate structure is intentional and should not be changed to multiple crates,
  because C++-Rust interop governed by crubit is significantly simpler with just 1 crate.
- Also translate C++ unit tests to Rust, but you don't need to translate SQL-level tests.
- Try not to change cmake configuration files. It is OK to change them when you need to pass
  headers of functions defined in Rust to the C++ compiler, but for other cases change them
  only when absolutely necessary.
- Also change the calling side of the C++ code. When done remove the C++ side of the code.
- Try to eliminate `unsafe` blocks in Rust code.
- Try not to introduce new compiler warnings into the codebase.
- To verify the resulting codebase after porting some piece of code, run the followings
  at the repository toplevel directory:
  - `make`
  - `build/release/test/unittest "*"`
    - (This could take ~30 minutes.)
