load("@rules_cc//cc:defs.bzl", "cc_shared_library", "cc_test")
load("@rules_shell//shell:sh_test.bzl", "sh_test")
load(
    "//bazel:duckdb_build_config.bzl",
    "DUCKDB_DEFINES",
    "DUCKDB_INCLUDES",
    "duckdb_library",
)

"""Macros for testing: the unittest targets and the loadable test extensions.

The unittest targets are mechanical: one sqllogictest list per test subdirectory
and speed class, and one cc_test per subdirectory and speed class with the same
link flags and the same fixture tree. Spelling that out 225 times is what made
BUILD.bazel grow past 14k lines, so it lives here instead.

A subdirectory whose corpus is too slow for one process is split into groups
that Bazel can run concurrently (sql_logic_test_groups); a coverage test then
guards that the groups still partition the corpus (sql_logic_test_coverage_test).

The loadable test extensions (duckdb_test_loadable_extension) are the shared
library + metadata-footer triplets the tests LOAD from test/extension/.
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

# The include paths the loadable test extensions add on top of the engine's.
LOADABLE_EXTENSION_INCLUDES = [
    "test/extension/debug_fs/include",
    "test/include",
    "third_party/catch",
    "third_party/sqlite/include",
]

def duckdb_test_loadable_extension(
        name,
        abi = "CPP",
        src = None,
        defines = [],
        includes = [],
        deps = [],
        hidden = True):
    """One loadable .duckdb_extension that tests LOAD by path under test/extension/.

    Each extension is its own shared library, and the loader reads the metadata
    footer off the end of the file, so three targets: the sources, the
    cc_shared_library, and a genrule that runs scripts/append_extension_metadata.py
    to append the footer and place the artifact where the test looks for it.

    The entry-point symbol follows from `abi` ("CPP" -> <name>_duckdb_cpp_init,
    "C_STRUCT_UNSTABLE" -> <name>_init_c_api_v2), and the output file from `name`;
    `src` defaults to test/extension/<name>.cpp. `defines`/`includes`/`deps` extend
    the shared lists, and `hidden` is False for an extension that links the C++ API
    wrapper, whose symbols must survive into the shared library.
    """
    if abi == "CPP":
        entry = name + "_duckdb_cpp_init"
    elif abi == "C_STRUCT_UNSTABLE":
        entry = name + "_init_c_api_v2"
    else:
        fail("unknown loadable extension abi: " + abi)

    duckdb_library(
        name = name + "_loadable_extension_srcs",
        srcs = [src or ("test/extension/" + name + ".cpp")],
        local_defines = DUCKDB_DEFINES + [
            "DUCKDB_BUILD_DIRECTORY=\\\".\\\"",
            "DUCKDB_BUILD_LOADABLE_EXTENSION",
        ] + defines,
        copts = ["-fPIC"] + (["-fvisibility=hidden"] if hidden else []),
        includes = sorted(DUCKDB_INCLUDES + LOADABLE_EXTENSION_INCLUDES + includes),
        linkopts = ["-Wl,-exported_symbol,_" + entry, "-Wl,-dead_strip"],
    )

    cc_shared_library(
        name = name + "_loadable_extension",
        deps = [
            ":" + name + "_loadable_extension_srcs",
            ":duckdb_static",
            ":dummy_static_extension_loader",
        ] + deps,
    )

    native.genrule(
        name = name + "_loadable_extension_file",
        srcs = [
            ":" + name + "_loadable_extension",
            ":duckdb_platform_out",
            ":generated_version_info",
            "scripts/append_extension_metadata.py",
        ],
        outs = ["test/extension/" + name + ".duckdb_extension"],
        cmd = "cp $(location :{name}_loadable_extension) $@ && chmod u+w $@ && python3 $(location scripts/append_extension_metadata.py) $@ --platform $(location :duckdb_platform_out) --version-from $(location :generated_version_info) --extension-version \"default-version\" --abi \"{abi}\"".format(
            name = name,
            abi = abi,
        ),
    )

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

def sql_logic_test_list(name, globs, kind):
    """The `-f` list of the sqllogictest files one directory owns."""
    # $$f survives Make-variable expansion into a literal $f.
    native.genrule(
        name = name,
        srcs = globs,
        outs = [name + ".txt"],
        cmd = 'for f in $(SRCS); do case "$$f" in %s) echo "$$f";; esac; done > $@' % SQL_LOGIC_SUFFIXES[kind],
    )

def subdirectory_test(name, args, size = "medium", testlist = None, deps = [],
                      fixture_tree = None):
    """One test subdirectory's tests, and only that subdirectory's.

    Only the -f list, and for the crawls that need real files the fixture tree,
    are per-target; the trees and the loadable test extensions are the same for
    every test, so they are named here instead of 221 times over. A fixture tree
    is a copy, so the built .duckdb_extension artifacts under test/ are not
    reachable from it -- hence the two shapes rather than one plus a tree.
    """
    if fixture_tree:
        trees = [":" + fixture_tree, ":test_data_without_fixtures"]
    else:
        trees = [":test_data", ":test_extension_artifacts"]
    cc_test(
        name = name,
        args = args,
        size = size,
        linkopts = TEST_LINKOPTS,
        deps = ["//:unittest_harness"] + deps,
        data = trees + ([testlist] if testlist else []),
    )

def sql_logic_test_groups(prefix, groups, kind, size = "medium", deps = []):
    """One list+test pair per group; `groups` maps a group name to its glob list.

    The groups partition one subdirectory's corpus, so Bazel runs them as
    concurrent tests instead of one long serial run. Returns the group testlist
    labels for sql_logic_test_coverage_test.
    """
    testlists = []
    for group, globs in groups.items():
        list_name = "%s_%s_%s_testlist" % (prefix, group, kind)
        sql_logic_test_list(name = list_name, globs = globs, kind = kind)
        subdirectory_test(
            name = "%s_%s_%s_test" % (prefix, group, kind),
            args = ["-f", list_name + ".txt"],
            size = size,
            deps = deps,
            testlist = ":" + list_name,
        )
        testlists.append(":" + list_name)
    return testlists

def sql_logic_test_coverage_test(name, oracle, testlists, size = "small"):
    """Fails unless `testlists` partition `oracle`: every file exactly once.

    A group that stops covering part of the corpus -- a new subdirectory added
    without a group, or two groups claiming the same files -- would otherwise
    shrink coverage silently. The oracle list is not run as a test itself.
    """
    # A genrule output is <label name>.txt and the test reads it relative to
    # the runfiles working directory, hence the bare file names.
    args = [label[1:] + ".txt" for label in [oracle] + testlists]
    sh_test(
        name = name,
        size = size,
        srcs = ["//bazel:sql_logic_coverage_check.sh"],
        args = args,
        data = [oracle] + testlists,
    )
