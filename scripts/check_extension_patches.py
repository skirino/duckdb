#!/usr/bin/env python3
"""Verifies that every extension with patches also applies them.

Patches under .github/patches/extensions/<name> are only applied when the
extension's descriptor says so. Without that flag the patches are silently
ignored, which is how the mysql_scanner, postgres_scanner, sqlite_scanner and
sqlsmith patches went unapplied after their git_tag values were bumped.

Run with:
    python3 scripts/check_extension_patches.py [--config-dir DIR] [--patch-dir DIR]
"""

import argparse
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from scripts import extension_config


def collect_patches(patch_dir):
    """Map every extension that has patch files on disk to its patch filenames."""
    patches = {}
    if not patch_dir.is_dir():
        return patches
    for entry in sorted(patch_dir.iterdir()):
        if not entry.is_dir():
            continue
        names = sorted(child.name for child in entry.glob("*.patch"))
        if names:
            patches[entry.name] = names
    return patches


def check(config_dir, patch_dir):
    """Return the list of complaints; empty means the check passed."""
    patches = collect_patches(patch_dir)
    applying = {descriptor.name for descriptor in extension_config.applying_extensions(config_dir)}

    errors = []
    for name in sorted(patches):
        if name in applying:
            continue
        count = len(patches[name])
        text = (
            f"{name}: {count} {'patch' if count == 1 else 'patches'} on disk, " "but the descriptor does not apply them"
        )
        for patch in patches[name]:
            text += f"\n  {patch}"
        errors.append(text)
    for name in sorted(applying - set(patches)):
        errors.append(f"{name}: no patches, but the descriptor applies them")

    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--config-dir",
        type=Path,
        default=REPO_ROOT / ".github" / "config",
        help="Directory holding the extension descriptors and groups.",
    )
    parser.add_argument(
        "--patch-dir",
        type=Path,
        default=REPO_ROOT / ".github" / "patches" / "extensions",
        help="Directory holding the per-extension patch directories.",
    )
    args = parser.parse_args()

    errors = check(args.config_dir, args.patch_dir)
    if errors:
        for error in errors:
            print(error)
        print(
            "extension patch check failed, see .github/patches/extensions/README.md",
            file=sys.stderr,
        )
        return 1

    applying = extension_config.applying_extensions(args.config_dir)
    print(f"ok: {len(applying)} extensions apply patches, all accounted for")
    return 0


if __name__ == "__main__":
    sys.exit(main())
