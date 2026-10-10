# duckdb-cpp example

Minimal consumer of the duckdb-cpp API: opens an in-memory database, runs a
query, reads a value back out. `main.cpp` is the whole program.

## Build against this tree

The package is a directory of headers plus one translation unit, so an example
build is a header path, a source file, and the engine:

    bazel build //:duckdb_cpp_api
    c++ -std=c++17 -I src/include -I third_party/fmt/include \
        -c tools/cpp/duckdb_cpp.cpp -o /tmp/duckdb_cpp.o
    c++ -std=c++17 -I src/include -I third_party/fmt/include \
        main.cpp /tmp/duckdb_cpp.o -L bazel-bin -lduckdb -o example
    ./example

Expected output:

    ┌────────┐
    │ answer │
    │ int32  │
    ├────────┤
    │     42 │
    └────────┘

`//:duckdb_cpp_api` is the same set of sources as a `cc_library`, so a Bazel
consumer can just depend on it instead of compiling `duckdb_cpp.cpp` by hand.
