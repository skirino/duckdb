load("@rules_cc//cc:defs.bzl", "cc_test")

"""Macros for the generated unittest targets.

The unittest shards are mechanical: one sqllogictest list per directory and speed
class, and one cc_test per directory and speed class with the same link flags and
the same fixture tree. Spelling that out 225 times is what made BUILD.bazel grow
past 14k lines, so it lives here instead.
"""

# The trees every shard ships, as CMake's unittest sees them: the tests include
# fixtures from other directories by path (a .test_template in test/sql/tpch, the
# CSVs under test/sakila/data), so each shard ships all of them. BUILD.bazel turns
# this into a single filegroup, so the glob is expanded once.
TEST_DATA_GLOBS = [
    "test/**",
    "benchmark/aoc24/**",
    "benchmark/recursive_cte/**",
    "data/**",
    "third_party/**",
]

# The same list for a shard that takes data/ as a real copy (see FIXTURE_TREES):
# it cannot also glob data/**, or the two would claim the same paths.
TEST_DATA_GLOBS_WITHOUT_FIXTURES = [g for g in TEST_DATA_GLOBS if not g.startswith("data/")]

# The top-level trees a shard copies instead of symlinking, when its corpus globs
# `dir/**`: DuckDB's crawl skips symlinks (LocalFileSystem::CrawlDirectoryLevel).
FIXTURE_TREES = ["data"]

SHARD_LINKOPTS = [
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

def shard_test(name, args, size, data, deps = ["//:unittest_harness"]):
    """One unittest shard: a directory's tests, and only that directory's."""
    cc_test(
        name = name,
        args = args,
        size = size,
        linkopts = SHARD_LINKOPTS,
        deps = deps,
        data = data,
    )