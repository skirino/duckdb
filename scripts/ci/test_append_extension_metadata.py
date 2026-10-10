#!/usr/bin/env python3

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT = REPO_ROOT / "scripts" / "append_extension_metadata.py"

# ParsedExtensionMetaData::FOOTER_SIZE, and the eight 32-byte fields the loader splits it into.
FIELD_SIZE = 32
FOOTER_SIZE = 512
FIELDS = 8


def parse_footer(extension):
    """Read the trailing footer the way ExtensionHelper::ParseExtensionMetaData does."""
    with open(extension, "rb") as f:
        f.seek(-FOOTER_SIZE, 2)
        raw = f.read()
    fields = [raw[i * FIELD_SIZE : (i + 1) * FIELD_SIZE].rstrip(b"\0").decode() for i in range(FIELDS)]
    fields.reverse()
    return dict(
        zip(
            [
                "magic",
                "platform",
                "version",
                "extension_version",
                "abi",
                "meta6",
                "meta7",
                "meta8",
            ],
            fields,
        )
    )


class AppendExtensionMetadataTest(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp_dir.cleanup)
        self.root = Path(self.temp_dir.name)
        self.platform = self.root / "duckdb_platform_out"
        self.platform.write_text("osx_arm64\n")
        self.version = self.root / "version.hpp"
        self.version.write_text('#define DUCKDB_SOURCE_ID "v1.5.6-abcdef0"\n')

    def extension(self, name="demo_capi"):
        path = self.root / f"{name}.duckdb_extension"
        path.write_bytes(b"\x7fELF not really")
        return path

    def append(self, extension, abi, extra=()):
        return subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                str(extension),
                "--platform",
                str(self.platform),
                "--version-from",
                str(self.version),
                "--extension-version",
                "default-version",
                "--abi",
                abi,
                *extra,
            ],
            capture_output=True,
            text=True,
        )

    def test_cpp_footer_carries_the_duckdb_version(self):
        extension = self.extension("cpp_ext")
        self.assertEqual(self.append(extension, "CPP").returncode, 0)
        footer = parse_footer(extension)
        self.assertEqual(footer["magic"], "4")
        self.assertEqual(footer["platform"], "osx_arm64")
        self.assertEqual(footer["abi"], "CPP")
        self.assertEqual(footer["version"], "v1.5.6-abcdef0")
        self.assertEqual(footer["extension_version"], "default-version")

    def test_c_struct_footer_carries_the_capi_version(self):
        # The loader reads the version field as a C API version for C_STRUCT and range-checks it,
        # so the DuckDB version there is a footer no engine can load.
        extension = self.extension()
        result = self.append(extension, "C_STRUCT", ["--capi-version", "v1.5.6"])
        self.assertEqual(result.returncode, 0, result.stderr)
        footer = parse_footer(extension)
        self.assertEqual(footer["abi"], "C_STRUCT")
        self.assertEqual(footer["version"], "v1.5.6")

    def test_c_struct_without_a_capi_version_is_an_error(self):
        result = self.append(self.extension(), "C_STRUCT")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("--capi-version", result.stderr)

    def test_capi_version_on_another_abi_is_an_error(self):
        result = self.append(self.extension("cpp_ext"), "CPP", ["--capi-version", "v1.5.6"])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("--capi-version", result.stderr)
