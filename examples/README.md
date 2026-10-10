# Embedding examples

Two programs that link libduckdb and run a query: `embedded-c/main.c` through the C API and
`embedded-c++/main.cpp` through `duckdb.hpp`.

    bazel run //examples:embedded-c
    bazel run //examples:embedded-c++
