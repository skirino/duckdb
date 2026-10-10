#!/usr/bin/env python3

import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from scripts import check_extension_patches, extension_config
from scripts.ci.registry_fixtures import write_descriptor


class CheckExtensionPatchesTest(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp_dir.cleanup)
        self.root = Path(self.temp_dir.name)
        self.config_dir = self.root / "config"
        self.patch_dir = self.root / "patches" / "extensions"

    def descriptor(self, name, **kwargs):
        write_descriptor(self.config_dir, name, **kwargs)

    def patch(self, name, filename="0001-fix.patch"):
        directory = self.patch_dir / name
        directory.mkdir(parents=True, exist_ok=True)
        (directory / filename).write_text("", encoding="utf-8")

    def test_no_patches_no_errors(self):
        self.descriptor("httpfs")
        self.assertEqual(check_extension_patches.check(self.config_dir, self.patch_dir), [])

    def test_patch_without_apply_patches_is_an_error(self):
        self.descriptor("httpfs")
        self.patch("httpfs", "0001-a.patch")
        self.patch("httpfs", "0002-b.patch")
        (errors,) = check_extension_patches.check(self.config_dir, self.patch_dir)
        self.assertIn("httpfs: 2 patches on disk", errors)
        self.assertIn("0001-a.patch", errors)
        self.assertIn("0002-b.patch", errors)

    def test_apply_patches_without_patch_is_an_error(self):
        self.descriptor("httpfs", apply_patches=True)
        (error,) = check_extension_patches.check(self.config_dir, self.patch_dir)
        self.assertIn("httpfs: no patches", error)

    def test_matching_descriptor_and_patch_pass(self):
        self.descriptor("httpfs", apply_patches=True)
        self.patch("httpfs")
        self.assertEqual(check_extension_patches.check(self.config_dir, self.patch_dir), [])

    def test_disabled_descriptor_still_has_to_account_for_its_patches(self):
        # vortex is disabled, but its patch directory exists, so the flag still
        # has to be there: otherwise the patches go unapplied unnoticed.
        self.descriptor("vortex", apply_patches=True, enabled=False)
        self.patch("vortex")
        self.assertEqual(check_extension_patches.check(self.config_dir, self.patch_dir), [])

    def test_non_patch_files_do_not_count(self):
        self.descriptor("httpfs")
        (self.patch_dir / "httpfs").mkdir(parents=True)
        (self.patch_dir / "httpfs" / "README.md").write_text("", encoding="utf-8")
        self.assertEqual(check_extension_patches.check(self.config_dir, self.patch_dir), [])

    def test_the_checked_in_registry_is_consistent(self):
        # The real thing, not a fixture: every patch directory under
        # .github/patches/extensions has a descriptor that applies it.
        config_dir = extension_config.DEFAULT_CONFIG_DIR
        patch_dir = extension_config.patches_directory()
        self.assertEqual(check_extension_patches.check(config_dir, patch_dir), [])


if __name__ == "__main__":
    unittest.main()
