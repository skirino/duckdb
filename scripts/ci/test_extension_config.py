#!/usr/bin/env python3

import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from scripts import extension_config
from scripts.ci.registry_fixtures import write_descriptor, write_group, write_json


class ExtensionConfigTest(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp_dir.cleanup)
        self.config_dir = Path(self.temp_dir.name)

    def descriptor(self, name, **kwargs):
        write_descriptor(self.config_dir, name, **kwargs)

    def group(self, name, names):
        write_group(self.config_dir, name, names)

    def test_loads_every_field(self):
        self.descriptor(
            "spatial",
            submodules=["extension-ci-tools"],
            apply_patches=True,
        )
        pin = extension_config.load_pin(self.config_dir, "spatial")
        self.assertEqual(pin.name, "spatial")
        self.assertEqual(pin.git_url, "https://example.com/spatial")
        self.assertEqual(pin.git_tag, "0" * 40)
        self.assertEqual(pin.submodules, ["extension-ci-tools"])
        self.assertTrue(pin.apply_patches)
        self.assertTrue(pin.enabled)

    def test_missing_descriptor_is_an_error(self):
        with self.assertRaisesRegex(FileNotFoundError, "no extension descriptor"):
            extension_config.load_pin(self.config_dir, "nope")

    def test_descriptor_without_a_tag_is_an_error(self):
        write_json(
            self.config_dir / "extensions" / "broken.json",
            {"name": "broken", "git_url": "u"},
        )
        with self.assertRaisesRegex(ValueError, "missing 'git_tag'"):
            extension_config.load_pin(self.config_dir, "broken")

    def test_group_resolves_its_pins(self):
        self.descriptor("inet")
        self.descriptor("httpfs")
        self.group("release", ["inet", "httpfs"])
        pins = extension_config.resolve_path(self.config_dir, "release.json")
        self.assertEqual([pin.name for pin in pins], ["inet", "httpfs"])

    def test_group_skips_in_tree_members(self):
        # icu is an in-tree extension: a group can name it, but there is nothing
        # to pin, so it contributes no descriptor.
        self.descriptor("inet")
        self.group("release", ["icu", "inet"])
        pins = extension_config.resolve_path(self.config_dir, "release.json")
        self.assertEqual([pin.name for pin in pins], ["inet"])

    def test_group_skips_disabled_members(self):
        self.descriptor("vortex", enabled=False)
        self.descriptor("inet")
        self.group("external", ["vortex", "inet"])
        pins = extension_config.resolve_path(self.config_dir, "external.json")
        self.assertEqual([pin.name for pin in pins], ["inet"])

    def test_group_keeps_first_of_a_duplicate(self):
        self.descriptor("inet")
        self.group("release", ["inet", "inet"])
        pins = extension_config.resolve_path(self.config_dir, "release.json")
        self.assertEqual([pin.name for pin in pins], ["inet"])

    def test_resolves_a_descriptor_path(self):
        self.descriptor("inet")
        for path in ("extensions/inet.json", "inet.json"):
            with self.subTest(path=path):
                pins = extension_config.resolve_path(self.config_dir, path)
                self.assertEqual([pin.name for pin in pins], ["inet"])

    def test_a_missing_group_is_an_error(self):
        with self.assertRaisesRegex(FileNotFoundError, "no extension descriptor or group"):
            extension_config.resolve_path(self.config_dir, "nonexistent.json")

    def test_applying_extensions_ignores_enabled(self):
        # A disabled extension still owns its patch directory, so the flag is
        # what the patch check reads -- not whether the build enables it.
        self.descriptor("vortex", apply_patches=True, enabled=False)
        self.descriptor("inet")
        self.descriptor("fts", apply_patches=True)
        applying = [pin.name for pin in extension_config.applying_extensions(self.config_dir)]
        self.assertEqual(applying, ["fts", "vortex"])


class CheckedInRegistryTest(unittest.TestCase):
    """The registry as it is checked in, not a fixture."""

    config_dir = extension_config.DEFAULT_CONFIG_DIR

    def test_every_group_member_exists(self):
        # Either the extension lives in this repository or it has a descriptor.
        pins = extension_config.load_pins(self.config_dir)
        for group in extension_config.list_groups(self.config_dir):
            for name in extension_config.load_group(self.config_dir, group):
                with self.subTest(group=group, extension=name):
                    self.assertTrue(
                        name in pins or name in extension_config.IN_TREE_EXTENSIONS,
                        f"{group}.json names '{name}', which is neither in-tree nor has a descriptor",
                    )

    def test_in_tree_extensions_are_still_in_tree(self):
        for name in extension_config.IN_TREE_EXTENSIONS:
            with self.subTest(extension=name):
                self.assertTrue((REPO_ROOT / "extension" / name).is_dir())

    def test_no_descriptor_is_orphaned(self):
        # Every descriptor is named by at least one group, so none of them can
        # fall out of the registry unnoticed.
        pins = extension_config.load_pins(self.config_dir)
        named = set()
        for group in extension_config.list_groups(self.config_dir):
            named.update(extension_config.load_group(self.config_dir, group))
        for name in sorted(pins):
            if name in ("test_utils",):
                # Named by the BWC test harness rather than by a build config.
                continue
            with self.subTest(extension=name):
                self.assertIn(name, named)

    def test_every_patch_directory_has_a_descriptor_that_applies_it(self):
        patches = extension_config.patches_directory()
        applying = {pin.name for pin in extension_config.applying_extensions(self.config_dir)}
        for entry in sorted(patches.iterdir()):
            if not entry.is_dir() or not list(entry.glob("*.patch")):
                continue
            with self.subTest(extension=entry.name):
                self.assertIn(entry.name, applying)

    def test_submodule_lists_are_read_as_lists(self):
        # The reader used to scrape SUBMODULES out of the call text, where a
        # quoted path arrived with its quotes attached.
        self.assertEqual(
            extension_config.load_pin(self.config_dir, "avro").submodules,
            ["third_party/avro-c"],
        )


if __name__ == "__main__":
    unittest.main()
