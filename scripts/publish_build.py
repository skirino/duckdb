#!/usr/bin/env python3
"""Publishes a Bazel build under the layout the rest of the repository expects.

Bazel writes its outputs into one flat `bazel-bin/`, but the artifact scripts,
the CI workflow steps and the contributor docs all reach for `build/<type>/...`:
`build/release/duckdb`, `build/release/src/libduckdb.so`,
`build/release/test/run`, `build/relassert/duckdb`. This script links those
paths to Bazel's output, so nothing has to copy a binary it does not own and
switching build types costs nothing.

The links are hard links rather than symlinks: both artifact scripts copy
`build/<type>` with the links intact (`cp -a`, `cp -P`) and then tar the result,
so a symlink ships as a symlink -- and an absolute one, pointing into the Bazel
cache of the machine that built it, which resolves to nothing on the machine
that unpacks it. A hard link is the file, so `build/<type>/duckdb` is a real
binary again.

The unittest entry is the one file that is generated rather than linked:
`build/<type>/test/run` is the wrapper around the unittest binary, and its
template (`scripts/ci/test_runner_wrapper.py.in`) has to be expanded with the
source root and the unittest's file name.

Run with:
    python3 scripts/publish_build.py --output-dir build/release [--config --config=relassert]
"""

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

WRAPPER_TEMPLATE = REPO_ROOT / "scripts" / "ci" / "test_runner_wrapper.py.in"

# Bazel's output name -> where the rest of the repository looks for it. The CLI
# is the only one that is renamed, because Bazel names it after its target.
BINARIES = {
    "shell": "duckdb",
    "unittest": "test/unittest",
    "benchmark_runner": "benchmark/benchmark_runner",
}


def bazel_bin(config):
    """The bazel-bin directory for a build type, which is per-configuration."""
    args = ["bazel", "info", "bazel-bin"]
    if config:
        args.extend(config.split())
    result = subprocess.run(args, capture_output=True, text=True, cwd=REPO_ROOT)
    if result.returncode != 0:
        raise RuntimeError(f"`{' '.join(args)}` failed:\n{result.stderr}")
    return Path(result.stdout.strip())


def link(source, destination):
    """Put source at destination, replacing whatever was there."""
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.is_symlink() or destination.exists():
        destination.unlink()
    try:
        os.link(source, destination)
    except OSError:
        # Filesystems without hard links (or a checkout on a different volume
        # than the Bazel output cache) still work, it just costs a copy.
        shutil.copy2(source, destination)


def publish_shared_library(bazel_bin_dir, output_dir):
    """The C API shared library, which Bazel names for the platform."""
    patterns = ["libduckdb.so*", "libduckdb.dylib", "libduckdb.dll"]
    for pattern in patterns:
        matches = sorted(path for path in bazel_bin_dir.glob(pattern) if path.is_file())
        if matches:
            for match in matches:
                link(match, output_dir / "src" / match.name)
            return [match.name for match in matches]
    return []


def publish_loadable_extensions(bazel_bin_dir, output_dir):
    """The test extensions, which the suite LOADs by path."""
    published = []
    extension_dir = bazel_bin_dir / "test" / "extension"
    if not extension_dir.is_dir():
        return published
    for extension in sorted(extension_dir.glob("*.duckdb_extension")):
        link(extension, output_dir / "test" / "extension" / extension.name)
        published.append(extension.name)
    return published


def write_run_wrapper(output_dir, unittest_name):
    """Expand the unittest wrapper template next to the binary it runs."""
    if not unittest_name:
        unittest_name = "unittest" + (".exe" if os.name == "nt" else "")

    template = WRAPPER_TEMPLATE.read_text()
    expanded = template.replace("@PROJECT_SOURCE_DIR@", str(REPO_ROOT))
    expanded = expanded.replace("@RUN_WRAPPER_UNITTEST_NAME@", unittest_name)

    run = output_dir / "test" / ("run.bat" if os.name == "nt" else "run")
    run.parent.mkdir(parents=True, exist_ok=True)
    run.write_text(expanded)
    run.chmod(0o755)
    if os.name == "nt":
        # The Windows CI job invokes run.py rather than the extensionless run.
        (output_dir / "test" / "run.py").write_text(expanded)
    return run


def publish(bazel_bin_dir, output_dir, unittest_name=""):
    """Link output_dir to the build in bazel_bin_dir; returns what it published."""
    published = []
    for source_name, destination in BINARIES.items():
        source = bazel_bin_dir / source_name
        if source.is_file():
            link(source, output_dir / destination)
            published.append(destination)
    published.extend(publish_shared_library(bazel_bin_dir, output_dir))
    published.extend(publish_loadable_extensions(bazel_bin_dir, output_dir))
    write_run_wrapper(output_dir, unittest_name)
    return published


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output-dir",
        required=True,
        type=Path,
        help="Directory to publish into, e.g. build/release.",
    )
    parser.add_argument(
        "--config",
        default="",
        help="The .bazelrc config the build used, e.g. '--config=relassert'.",
    )
    parser.add_argument(
        "--unittest-name",
        default="",
        help="File name of the unittest binary the wrapper should run.",
    )
    args = parser.parse_args()

    output_dir = args.output_dir
    if not output_dir.is_absolute():
        output_dir = REPO_ROOT / output_dir
    output_dir.mkdir(parents=True, exist_ok=True)

    bazel_bin_dir = bazel_bin(args.config)
    published = publish(bazel_bin_dir, output_dir, args.unittest_name)

    print(f"published {output_dir} from {bazel_bin_dir}")
    for name in published:
        print(f"  {name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
