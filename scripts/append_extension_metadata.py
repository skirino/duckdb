#!/usr/bin/env python3
"""Append the DuckDB extension metadata footer to a built extension.

The layout is a WebAssembly custom section: the loader reads the last
ParsedExtensionMetaData::FOOTER_SIZE bytes of the file as eight 32-byte fields
followed by a 256-byte signature, so the eight fields are written back to front,
NUL-padded, with META1 last.

Which version goes in that field depends on the ABI. A C_STRUCT footer carries the
C API version the extension was compiled against (--capi-version), because that is
what the loader range-checks; CPP and C_STRUCT_UNSTABLE carry the DuckDB version
instead, and there the value comes from the file the engine itself reads
(--version-from), so the two cannot disagree about which build this is.

Usage: append_extension_metadata.py <file> --platform <platform>
         (--version <duckdb version> | --version-from <generated header>)
         --extension-version <version> --abi <abi type> [--capi-version v1.5.6]
"""

import argparse
import re
import sys

SOURCE_ID = re.compile(r'#define DUCKDB_SOURCE_ID "(?P<value>[^"]+)"')

# Section id (1) + section length as LEB128 (2) + name length as LEB128 (1) +
# name (16) + payload length as LEB128 (2) + 8 * 32 fields + a 256 byte
# signature. The loader only reads the trailing FOOTER_SIZE bytes, so every
# length here has to match it exactly.
SECTION_SIZE = 534
SIGNATURE_SIZE = 256
FIELD_SIZE = 32
METADATA_VERSION = "4"


def leb128(value):
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        out.append(byte | 0x80 if value else byte)
        if not value:
            return bytes(out)


def field(value):
    return value.encode()[:FIELD_SIZE].ljust(FIELD_SIZE, b"\0")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("extension")
    parser.add_argument("--platform", required=True)
    version = parser.add_mutually_exclusive_group(required=True)
    version.add_argument("--version")
    version.add_argument("--version-from")
    parser.add_argument("--extension-version", default="")
    parser.add_argument("--abi", required=True)
    parser.add_argument(
        "--capi-version",
        default="",
        help="the C API version this extension is built against, e.g. v1.5.6; required by, and only by, --abi C_STRUCT",
    )
    args = parser.parse_args()

    if (args.abi == "C_STRUCT") != bool(args.capi_version):
        sys.exit(
            "--abi C_STRUCT needs --capi-version and the other ABIs must not have one: the version field "
            "carries the C API version for C_STRUCT and the DuckDB version for the rest"
        )

    if args.version_from:
        with open(args.version_from) as f:
            match = SOURCE_ID.search(f.read())
        if not match:
            sys.exit(f"no DUCKDB_SOURCE_ID in {args.version_from}")
        args.version = match.group("value")
    with open(args.platform, "rb") as f:
        platform = f.read().decode().strip("\n")

    # META3 is read back as the C API version for C_STRUCT and as the DuckDB
    # version for the other ABIs (src/main/extension/extension_load.cpp).
    build_version = args.capi_version or args.version

    name = b"duckdb_signature"
    payload = b"".join(
        [
            # META8..META1, so META1 (the magic value) ends up last in the file.
            b"\0" * FIELD_SIZE,  # META8
            b"\0" * FIELD_SIZE,  # META7
            b"\0" * FIELD_SIZE,  # META6
            field(args.abi),  # META5
            field(args.extension_version),  # META4
            field(build_version),  # META3
            field(platform),  # META2
            field(METADATA_VERSION),  # META1
            b"\0" * SIGNATURE_SIZE,
        ]
    )

    name_length = leb128(len(name))
    payload_length = leb128(len(payload))
    section = (
        b"\0"
        + leb128(len(name_length) + len(name) + len(payload_length) + len(payload))
        + name_length
        + name
        + payload_length
        + payload
    )
    assert len(section) == SECTION_SIZE, len(section)

    with open(args.extension, "ab") as f:
        f.write(section)


if __name__ == "__main__":
    sys.exit(main())
