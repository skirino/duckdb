#!/usr/bin/env python3
"""Builders for the extension-registry fixtures the scripts/ci tests share.

Every registry test writes the same two things into a temporary config
directory -- a descriptor and a group -- so the writers live here rather than
being copied into each of the test modules that needs them.
"""

import json
from pathlib import Path


def write_json(path, value):
    """Write one JSON file, creating the directories above it."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value), encoding="utf-8")


def write_descriptor(config_dir, name, **kwargs):
    """Write extensions/<name>.json, the descriptor shape extension_config reads."""
    entry = {
        "name": name,
        "git_url": f"https://example.com/{name}",
        "git_tag": "0" * 40,
        "submodules": [],
    }
    entry.update(kwargs)
    write_json(Path(config_dir) / "extensions" / f"{name}.json", entry)


def write_group(config_dir, name, names):
    """Write <name>.json, a group file: a list of extension names."""
    write_json(Path(config_dir) / f"{name}.json", list(names))
