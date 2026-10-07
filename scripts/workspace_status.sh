#!/usr/bin/env bash
# Bazel workspace status: this checkout's commit and version.
#
# Bazel runs this once per build and hands the KEY VALUE lines to every rule
# through ctx.info_file, which is how a build gets a value that follows the
# checkout without putting it on the command line. The STABLE_ prefix keeps the
# keys in the stable status file, so the action that reads them is a cache hit
# unless the commit actually moved; volatile-status.txt carries BUILD_TIMESTAMP
# and would re-run it every build. The recipes mirror CMake's,
# which bakes the same two values at configure time (CMakeLists.txt:443 and
# :501): `git log -1 --format=%H` truncated to 10 and lowercased, and
# release_version.txt's major.minor with the patch at 0 and the dev iteration at
# `git rev-list --count HEAD`.
set -euo pipefail
repo="$(git rev-parse --show-toplevel)"
echo "STABLE_DUCKDB_SOURCE_ID $(git -C "$repo" log -1 --format=%H | cut -c1-10 | tr '[:upper:]' '[:lower:]')"
echo "STABLE_DUCKDB_VERSION v$(tr -d '[:space:]' <"$repo/scripts/ci/release_version.txt").0-dev$(git -C "$repo" rev-list --count HEAD)"