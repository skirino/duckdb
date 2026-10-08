#!/usr/bin/env python3
"""Fail when the stamped version header does not describe the current HEAD.

The engine's commit and version come from generated_version_info.hpp, which a rule
fills in from the workspace status script (scripts/workspace_status.sh) once per
build. CMake takes the same two values from its configure instead, so its binaries
go stale between configures; this checks the Bazel side has not.

Three things are checked: the header agrees with HEAD, BUILD.bazel defines neither
value the header owns, and the one checkout-derived define that has to stay -- the
identity an unstable extension reports on load -- still matches the engine.

Usage (standalone): check_source_id.py [<repo root>]
Usage (as a test):   check_source_id.py   # runfiles root holds the header, .git/
"""
import os
import re
import subprocess
import sys

HEADER = "generated_version_info.hpp"
HEADER_OWNED = ("DUCKDB_SOURCE_ID", "DUCKDB_VERSION")
# An unstable extension reports this as its version, and the engine rejects the load
# when it differs from its own (src/main/extension.cpp:50). It has to be compiled in,
# so unlike the header's values it cannot come from the header -- but it must not
# drift from DUCKDB_SOURCE_ID either, or every loadable extension fails to load.
LOAD_IDENTITY_DEFINE = "DUCKDB_EXTENSION_API_VERSION_UNSTABLE"


def head_from_git_dir(git_dir):
    """Resolve HEAD the way git does, without needing git."""
    head = open(os.path.join(git_dir, "HEAD")).read().strip()
    if not head.startswith("ref:"):
        return head
    ref = head.split(":", 1)[1].strip()
    path = os.path.join(git_dir, ref)
    if os.path.exists(path):
        return open(path).read().strip()
    packed = os.path.join(git_dir, "packed-refs")
    if os.path.exists(packed):
        with open(packed) as f:
            for line in f:
                if line.startswith(("#", "^")):
                    continue
                fields = line.split()
                if len(fields) == 2 and fields[1] == ref:
                    return fields[0]
    raise SystemExit(f"cannot resolve {ref}: neither {path} nor packed-refs has it")


def stamped(repo):
    path = os.path.join(repo, HEADER)
    if not os.path.exists(path):
        raise SystemExit(f"{HEADER} is missing: nothing stamped the version")
    values = {}
    for line in open(path):
        match = re.match(r'#define (DUCKDB_\w+) "([^"]*)"', line)
        if match and match.group(1) in HEADER_OWNED:
            values[match.group(1)] = match.group(2)
    missing = [name for name in HEADER_OWNED if name not in values]
    if missing:
        raise SystemExit(f"{HEADER} does not define {', '.join(missing)}")
    return values


def build_define(build, name):
    """A define's value as BUILD.bazel spells it, or None when it is absent."""
    match = re.search(rf'"{name}=([^"]*)"', build)
    return match.group(1) if match else None


def main():
    if len(sys.argv) > 1:
        repo = sys.argv[1]
        head = subprocess.run(["git", "-C", repo, "log", "-1", "--format=%H"],
                              capture_output=True, text=True, check=True).stdout.strip()
    else:
        repo = os.getcwd()
        head = head_from_git_dir(os.path.join(repo, ".git"))
    # CMake compares against `git log -1 --format=%H` truncated to 10 and
    # lowercased (CMakeLists.txt:459-465), so the comparison uses that form.
    head = head[:10].lower()

    values = stamped(repo)
    failures = []
    if values["DUCKDB_SOURCE_ID"] != head:
        failures.append(f"{HEADER} says DUCKDB_SOURCE_ID {values['DUCKDB_SOURCE_ID']}, "
                        f"HEAD is {head}")
    build = open(os.path.join(repo, "BUILD.bazel")).read()
    for name in HEADER_OWNED:
        if re.search(rf'"{name}=', build):
            failures.append(f"BUILD.bazel still defines {name}; {HEADER} owns it, and a "
                            f"define would silently win where the header is not included")
    load_identity = build_define(build, LOAD_IDENTITY_DEFINE)
    if load_identity is None:
        failures.append(f"BUILD.bazel no longer defines {LOAD_IDENTITY_DEFINE}; a "
                        f"loadable extension would report an empty version")
    elif load_identity != values["DUCKDB_SOURCE_ID"]:
        failures.append(f"BUILD.bazel defines {LOAD_IDENTITY_DEFINE}={load_identity}, the "
                        f"engine reports {values['DUCKDB_SOURCE_ID']}; an unstable "
                        f"extension is rejected when the two differ, so every loadable "
                        f"extension would fail to load")
    if failures:
        print("\n".join(failures))
        print("\nRe-run the BUILD.bazel generator, then commit the result.")
        return 1
    print(f"{HEADER} matches HEAD {head} (version {values['DUCKDB_VERSION']}), "
          f"{LOAD_IDENTITY_DEFINE} agrees")
    return 0


if __name__ == "__main__":
    sys.exit(main())