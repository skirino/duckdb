load("@rules_cc//cc:defs.bzl", "cc_test")
load("@rules_shell//shell:sh_test.bzl", "sh_test")
load("//bazel:duckdb_build_config.bzl", "duckdb_library", "DUCKDB_INCLUDES")

"""Macros for the generated unittest targets.

The unittest targets are mechanical: one sqllogictest list per test subdirectory
and speed class, and one cc_test per subdirectory and speed class with the same
link flags and the same fixture tree. Spelling that out 225 times is what made
BUILD.bazel grow past 14k lines, so it lives here instead.

A subdirectory whose corpus is too slow for one process is split into groups
that Bazel can run concurrently (sql_logic_test_groups); a coverage test then
guards that the groups still partition the corpus (sql_logic_test_coverage_test).
"""

# The trees every target ships: the tests include
# fixtures from other directories by path (a .test_template in test/sql/tpch, the
# CSVs under test/sakila/data), so each target ships all of them. BUILD.bazel turns
# this into a single filegroup, so the glob is expanded once.
TEST_DATA_GLOBS = [
    "test/**",
    "benchmark/aoc24/**",
    "benchmark/recursive_cte/**",
    "data/**",
    "third_party/**",
    # The TPC-H and TPC-DS answer sets live beside the data generators, and the
    # SQL tests read them by path to compare a query against known results. Only
    # the three scale factors a test names are listed: every target ships this
    # filegroup, so the sf10 and sf100 answer sets -- 14 MB of the 16, and no test
    # that runs here -- would be paid for 226 times over.
    "extension/tpcds/dsdgen/answers/sf0.01/**",
    "extension/tpcds/dsdgen/answers/sf1/**",
    "extension/tpch/dbgen/answers/sf0.01/**",
    "extension/tpch/dbgen/answers/sf0.1/**",
    "extension/tpch/dbgen/answers/sf1/**",
]

# The same list for a target that takes data/ as a real copy (see FIXTURE_TREES):
# it cannot also glob data/**, or the two would claim the same paths.
TEST_DATA_GLOBS_WITHOUT_FIXTURES = [g for g in TEST_DATA_GLOBS if not g.startswith("data/")]

# The top-level trees a target copies instead of symlinking, when its corpus globs
# `dir/**`: DuckDB's crawl skips symlinks (LocalFileSystem::CrawlDirectoryLevel).
FIXTURE_TREES = ["data"]

TEST_LINKOPTS = [
    "-Wl,-search_paths_first",
    "-Wl,-headerpad_max_install_names",
]

# Which files each list holds. `-f` registers exactly the paths it is given and
# clears the rest of the Catch2 registry (test/unittest.cpp), which is why the
# list is a file rather than a `prefix*` Catch2 pattern.
SQL_LOGIC_SUFFIXES = {
    "fast": "*.test",
    "slow": "*.test_slow|*.test_coverage",
}

def duckdb_test_directory(
    deps = [],
    sqllogictest_size = "medium",
    cpp_size = "medium",
):
    """A whole test directory: its corpus, and the fast (and slow) runs over it.

    This is the shape nearly every test subdirectory wants, spelled once. `name`
    is the directory slug (the package's own name); the corpus is every file in
    the package, and the fast list is its non-hidden `.test` files. A directory
    with hidden corpus also gets the slow run; one with C++ cases gets its own
    cc_test pairs over the Catch2 registry (excluding `*.test`), because `-f`
    clears that registry and the two halves cannot share a binary.

    The `-f` argument is a runfiles-relative path, so the package prefix is
    derived here rather than spelled per call.
    """
    pkg = native.package_name()
    cpp_srcs = native.glob(["**/*.cpp"], allow_empty = True)
    sqllogictest_fast = native.glob(["**/*.test"], allow_empty = True)
    sqllogictest_slow = native.glob(["**/*.test_slow", "**/*.test_coverage"], allow_empty = True)

    def _cc_test(name, args, size, data = []):
        cc_test(
            name = name,
            args = args,
            size = size,
            linkopts = TEST_LINKOPTS,
            deps = ["//:unittest_harness"] + deps + ([":lib"] if cpp_srcs else []),
            data = ["//:test_data", "//:test_extension_artifacts"] + data,
        )

    if cpp_srcs:
        duckdb_library(
            name = "lib",
            srcs = cpp_srcs,
            includes = sorted(DUCKDB_INCLUDES + [
                "test/extension/debug_fs/include",
                "test/include",
                "third_party/catch",
                "third_party/sqlite/include",
            ]),
        )
        _cc_test(
            name = "cpp_nonhidden_test",
            args = ["~[.]", "~*.test", "~*.test_slow", "~*.test_coverage"],
            size = cpp_size,
        )
        _cc_test(
            name = "cpp_hidden_test",
            args = ["[.]", "~*.test", "~*.test_slow", "~*.test_coverage"],
            size = "large",
        )

    def _sqllogictest(kind, test_size):
        list_name = "sqllogictest_%s_list" % kind
        native.genrule(
            name = list_name,
            srcs = native.glob(["**"]),
            outs = [list_name + ".txt"],
            cmd = 'for f in $(SRCS); do case "$$f" in %s) echo "$$f";; esac; done > $@' % SQL_LOGIC_SUFFIXES[kind],
        )
        _cc_test(
            name = "sqllogictest_%s_test" % kind,
            args = ["-f", pkg + "/" + list_name + ".txt"],
            size = test_size,
            data = [":" + list_name],
        )

    if sqllogictest_fast:
        _sqllogictest("fast", sqllogictest_size)
    if sqllogictest_slow:
        _sqllogictest("slow", "large")

def duckdb_demo_loadable_extension(
    
):
    duckdb_library(
        name = "cpp_api_demo_loadable_extension_srcs",
        srcs = ["test/extension/cpp_api_demo.cpp"],
        local_defines = DUCKDB_DEFINES + [
            "DUCKDB_BUILD_DIRECTORY=\\\".\\\"",
            "DUCKDB_BUILD_LOADABLE_EXTENSION",
            "DUCKDB_CPP_API_LOADABLE",
            "DUCKDB_EXTENSION_API_VERSION_UNSTABLE=bc5e2bb7ee",
            "DUCKDB_EXTENSION_NAME=cpp_api_demo",
        ],
        includes = sorted(DUCKDB_INCLUDES + DUCKDB_TEST_INCLUDES),
        linkopts = ["-Wl,-exported_symbol,_cpp_api_demo_init_c_api_v2", "-Wl,-dead_strip"],
    )
    cc_shared_library(
        name = "cpp_api_demo_loadable_extension",
        deps = [
            ":cpp_api_demo_loadable_extension_srcs",
            ":duckdb_static",
            ":dummy_static_extension_loader",
            ":duckdb_cpp_api",
        ],
    )
    native.genrule(
        name = "cpp_api_demo_loadable_extension_file",
        srcs = [":cpp_api_demo_loadable_extension", ":duckdb_platform_out", ":generated_version_info", "scripts/append_extension_metadata.py"],
        outs = ["test/extension/cpp_api_demo.duckdb_extension"],
        cmd = "cp $(location :cpp_api_demo_loadable_extension) $@ && chmod u+w $@ && python3 $(location scripts/append_extension_metadata.py) $@ --platform $(location :duckdb_platform_out) --version-from $(location :generated_version_info) --extension-version \"default-version\" --abi \"C_STRUCT_UNSTABLE\"",
    )
