"""Compile settings, generated alongside BUILD.bazel -- do not edit.

CMake puts the same include paths, defines and dependencies on every translation
unit it compiles. Stated here once, the 60 libraries in BUILD.bazel are a handful
of lines each, and the settings are readable in one place instead of scattered
across every target.
"""

load("@rules_cc//cc:defs.bzl", "cc_library")

# The -isystem paths, which are CMake's include_directories() for the whole build.
DUCKDB_INCLUDES = [
    "codegen",
    "extension",
    "extension/core_functions/include",
    "extension/parquet/include",
    "src/include",
    "third_party/concurrentqueue",
    "third_party/fast_float",
    "third_party/fastpforlib",
    "third_party/fmt/include",
    "third_party/fsst",
    "third_party/httplib",
    "third_party/hyperloglog",
    "third_party/jaro_winkler",
    "third_party/mbedtls/include",
    "third_party/miniz",
    "third_party/pcg",
    "third_party/pdqsort",
    "third_party/re2",
    "third_party/ska_sort",
    "third_party/skiplist",
    "third_party/tdigest",
    "third_party/utf8proc/include",
    "third_party/vergesort",
    "third_party/yyjson/include",
    "third_party/zstd/include",
]

# What the test libraries add: the Catch2 and test helper headers.
DUCKDB_TEST_INCLUDES = [
    "test/extension/debug_fs/include",
    "test/include",
    "third_party/catch",
    "third_party/sqlite/include",
    "tools/cpp",
]

# The defines every library carries, beyond what .bazelrc passes.
DUCKDB_DEFINES = [
    "DUCKDB",
    "DUCKDB_BUILD_LIBRARY",
    "DUCKDB_EXTENSION_CORE_FUNCTIONS_LINKED=1",
    "DUCKDB_EXTENSION_PARQUET_LINKED=1",
    "DUCKDB_MAIN_LIBRARY",
    "DUCKDB_MAJOR_VERSION=2",
    "DUCKDB_MINOR_VERSION=0",
    "DUCKDB_PATCH_VERSION=0",
    "GENERATED_EXTENSION_HEADERS=1",
]

# -fPIC is Bazel's spelling of CMake's POSITION_INDEPENDENT_CODE property.
DUCKDB_COPTS = ["-fPIC"]

DUCKDB_LIB_DEPS = [":duckdb_headers", ":generated_extension_loader"]

def duckdb_library(
        name,
        srcs,
        local_defines,
        includes = DUCKDB_INCLUDES,
        copts = DUCKDB_COPTS,
        deps = DUCKDB_LIB_DEPS,
        linkopts = None,
        alwayslink = True):
    """One library of the DuckDB build.

    Every attribute has the value nearly every library wants, so a target states
    only where it differs."""
    cc_library(
        name = name,
        srcs = srcs,
        local_defines = local_defines,
        copts = copts,
        includes = includes,
        deps = deps,
        linkopts = linkopts or [],
        alwayslink = alwayslink,
    )
