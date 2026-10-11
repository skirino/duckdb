"""Moved to bazel/duckdb_test.bzl - do not edit directly."""
load("@rules_cc//cc:defs.bzl", "cc_test")

"""Macros for the generated unittest targets.

The unittest targets are mechanical: one sqllogictest list per test subdirectory
and speed class, and one cc_test per subdirectory and speed class with the same
link flags and the same fixture tree. Spelling that out 225 times is what made
BUILD.bazel grow past 14k lines, so it lives here instead.
"""