#!/usr/bin/env python3
"""Append the DuckDB extension metadata footer to a built extension.

The layout is a WebAssembly custom section, matching
scripts/append_metadata.cmake (which does the same for CMake builds): the
loader reads the last ParsedExtensionMetaData::FOOTER_SIZE bytes of the file as
eight 32-byte fields followed by a 256-byte signature, so the eight fields are
written back to front, NUL-padded, with META1 last.

Usage: append_extension_metadata.py <file> --platform <platform>
         --version <duckdb version> --extension-version <version>
         --abi <abi type>
"""
import argparse
import sys

# Section id (1) + section length as LEB128 (2) + name length as LEB128 (1) +
# name (16) + payload length as LEB128 (2) + 8 * 32 fields + a 256 byte
# signature. The loader only reads the trailing FOOTER_SIZE bytes, so the
# lengths are written to match scripts/append_metadata.cmake exactly.
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
    parser.add_argument("--version", required=True)
    parser.add_argument("--extension-version", default="")
    parser.add_argument("--abi", required=True)
    args = parser.parse_args()

    with open(args.platform, "rb") as f:
        platform = f.read().decode().strip("\n")

    name = b"duckdb_signature"
    payload = b"".join([
        # META8..META1, so META1 (the magic value) ends up last in the file.
        b"\0" * FIELD_SIZE,        # META8
        b"\0" * FIELD_SIZE,        # META7
        b"\0" * FIELD_SIZE,        # META6
        field(args.abi),            # META5
        field(args.extension_version),  # META4
        field(args.version),        # META3
        field(platform),            # META2
        field(METADATA_VERSION),    # META1
        b"\0" * SIGNATURE_SIZE,
    ])

    name_length = leb128(len(name))
    payload_length = leb128(len(payload))
    section = (b"\0" + leb128(len(name_length) + len(name) + len(payload_length) + len(payload))
               + name_length + name + payload_length + payload)
    assert len(section) == SECTION_SIZE, len(section)

    with open(args.extension, "ab") as f:
        f.write(section)


if __name__ == "__main__":
    sys.exit(main())