#!/usr/bin/env python3
"""Reader for the extension registry under .github/config.

Two kinds of JSON file live there:

  extensions/<name>.json  one out-of-tree extension pin:
                             {"name", "git_url", "git_tag",
                              "submodules": [...], "apply_patches": bool,
                              "enabled": bool}
  <group>.json            a list of extension names, used to describe a build
                          configuration (in-tree, out-of-tree, a release set).

`enabled` is false for the extensions the registry deliberately keeps out of
every configuration (vortex, lance). They stay described -- the pin is the input
to the patch workflow -- but resolving a group skips them.

Every consumer goes through here rather than parsing the files itself, so there
is one reader and one shape.
"""

import json
from dataclasses import dataclass, field
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CONFIG_DIR = REPO_ROOT / ".github" / "config"
EXTENSIONS_SUBDIR = "extensions"

# The extensions that live in this repository, under extension/<name>. A group
# may name one of these: there is nothing to pin or sync for them, so resolving
# a group yields no descriptor for them.
IN_TREE_EXTENSIONS = [
    "autocomplete",
    "core_functions",
    "demo_capi",
    "icu",
    "json",
    "parquet",
    "tpch",
    "tpcds",
]

GROUP_SUFFIX = ".json"
DESCRIPTOR_SUFFIX = ".json"


@dataclass(frozen=True)
class ExtensionPin:
    """One out-of-tree extension's pinned upstream revision."""

    name: str
    git_url: str
    git_tag: str
    submodules: list = field(default_factory=list)
    apply_patches: bool = False
    enabled: bool = True

    def to_dict(self):
        return {
            "name": self.name,
            "git_url": self.git_url,
            "git_tag": self.git_tag,
            "submodules": list(self.submodules),
            "apply_patches": self.apply_patches,
        }


def descriptor_path(config_dir, name):
    """Path of one extension's descriptor, by extension name."""
    return Path(config_dir) / EXTENSIONS_SUBDIR / (name + DESCRIPTOR_SUFFIX)


def group_path(config_dir, name):
    """Path of one group's file, by group name (with or without the suffix)."""
    name = str(name)
    if name.endswith(GROUP_SUFFIX):
        name = name[: -len(GROUP_SUFFIX)]
    return Path(config_dir) / (name + GROUP_SUFFIX)


def _read_json(path):
    with open(path, "r", encoding="utf-8") as handle:
        return json.load(handle)


def load_pin(config_dir, name, optional=False):
    """Read one extension descriptor by extension name.

    `optional` returns None for a name that has no descriptor at all, which is
    how an in-tree extension named by a group resolves.
    """
    path = descriptor_path(config_dir, name)
    if not path.is_file():
        if optional:
            return None
        raise FileNotFoundError(f"no extension descriptor for '{name}' at {path}")
    data = _read_json(path)
    for key in ("git_url", "git_tag"):
        if key not in data:
            raise ValueError(f"{path}: descriptor is missing '{key}'")
    return ExtensionPin(
        name=data.get("name", name),
        git_url=data["git_url"],
        git_tag=data["git_tag"],
        submodules=list(data.get("submodules", [])),
        apply_patches=bool(data.get("apply_patches", False)),
        enabled=bool(data.get("enabled", True)),
    )


def load_pins(config_dir=DEFAULT_CONFIG_DIR):
    """Read every extension descriptor, keyed by extension name."""
    directory = Path(config_dir) / EXTENSIONS_SUBDIR
    pins = {}
    for path in sorted(directory.glob("*" + DESCRIPTOR_SUFFIX)):
        pin = load_pin(config_dir, path.stem)
        pins[pin.name] = pin
    return pins


def load_group(config_dir, name):
    """Read one group's list of extension names, without resolving the pins."""
    path = group_path(config_dir, name)
    if not path.is_file():
        raise FileNotFoundError(f"no extension group named '{name}' at {path}")
    data = _read_json(path)
    if not isinstance(data, list):
        raise ValueError(f"{path}: a group is a list of extension names")
    return list(data)


def list_groups(config_dir=DEFAULT_CONFIG_DIR):
    """Every group name defined under the config directory."""
    return sorted(path.stem for path in Path(config_dir).glob("*" + GROUP_SUFFIX))


def resolve_path(config_dir, path):
    """Resolve a descriptor or group path to the pins it names.

    A path under extensions/ names one extension; anything else names a group,
    or -- when no such group exists -- a single extension, which is how a caller
    names one pin on a command line. Disabled extensions are dropped, since a
    configuration never wants them.
    """
    config_dir, path = _relative_to_config(config_dir, path)

    if path.parts and path.parts[0] == EXTENSIONS_SUBDIR:
        pins = [load_pin(config_dir, Path(*path.parts[1:]).stem)]
    elif (config_dir / path).is_file():
        names = load_group(config_dir, path.stem)
        pins = [pin for pin in (load_pin(config_dir, name, optional=True) for name in names) if pin]
    else:
        pin = load_pin(config_dir, path.stem, optional=True)
        if pin is None:
            raise FileNotFoundError(f"no extension descriptor or group at {config_dir / path}")
        pins = [pin]

    seen = set()
    unique = []
    for pin in pins:
        if pin.enabled and pin.name not in seen:
            seen.add(pin.name)
            unique.append(pin)
    return unique


def _relative_to_config(config_dir, path):
    """Rewrite a path into one relative to the config directory.

    A path is normally already relative to the config directory. A caller
    running from the repository root may instead pass a repository-relative
    path such as .github/config/extensions/httpfs.json, which is rebased here.
    """
    config_dir = Path(config_dir)
    path = Path(path)
    if not path.is_absolute():
        if not (config_dir / path).exists() and path.exists():
            return config_dir, path.resolve().relative_to(config_dir.resolve())
        return config_dir, path
    return config_dir, path.relative_to(config_dir)


def resolve_all(config_dir=DEFAULT_CONFIG_DIR, paths=None):
    """Resolve a list of paths, keeping first-seen order and dropping duplicates."""
    if paths is None:
        paths = [group_path(config_dir, group) for group in list_groups(config_dir)]

    pins = {}
    for path in paths:
        for pin in resolve_path(config_dir, path):
            pins.setdefault(pin.name, pin)
    return list(pins.values())


def applying_extensions(config_dir=DEFAULT_CONFIG_DIR):
    """Every descriptor that asks for its patches, whether or not it is enabled.

    The patch check reasons about the registry as a whole: a patch directory
    that no descriptor accounts for is a bug even for an extension the current
    build does not enable.
    """
    return sorted(
        (pin for pin in load_pins(config_dir).values() if pin.apply_patches),
        key=lambda pin: pin.name,
    )


def patches_directory(config_dir=DEFAULT_CONFIG_DIR):
    """Where the per-extension patch directories live."""
    return Path(config_dir).parent / "patches" / "extensions"
