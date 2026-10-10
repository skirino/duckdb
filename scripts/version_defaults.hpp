// The commit and version a Bazel build reports unless it is stamped.
//
// The rule in duckdb_codegen.bzl copies this over generated_version_info.hpp when
// --//:stamp_version is off, so the one translation unit that reads the macros and
// the extension metadata footers that parse them see the same text. Copying rather
// than #including it keeps these values off the command line, where they would
// land in the defines of every compile action.
//
// Only the shapes matter here, because nothing in the build may depend on the
// checkout: a 'v'-prefixed dev version, and an all-digits commit. The suite
// checks those shapes, never a real hash, and the extension version the engine
// reports is this same string. Pass --//:stamp_version to bake this checkout's
// own commit and version in instead.
#ifndef DUCKDB_VERSION_DEFAULTS_HPP
#define DUCKDB_VERSION_DEFAULTS_HPP
#define DUCKDB_SOURCE_ID "0000000000"
#define DUCKDB_VERSION   "v2.0.0-dev0"
#endif
