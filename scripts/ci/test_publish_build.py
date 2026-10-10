#!/usr/bin/env python3

import os
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from scripts import publish_build


class PublishBuildTest(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp_dir.cleanup)
        self.root = Path(self.temp_dir.name)
        self.bazel_bin = self.root / "bazel-bin"
        self.bazel_bin.mkdir()
        self.output_dir = self.root / "build" / "release"

    def make(self, name, *, directory=None):
        path = (directory or self.bazel_bin) / name
        path.write_text(name, encoding="utf-8")
        return path

    def test_binaries_land_where_the_repository_looks_for_them(self):
        self.make("shell")
        self.make("unittest")
        self.make("benchmark_runner")

        published = publish_build.publish(self.bazel_bin, self.output_dir)

        self.assertEqual(
            published,
            ["duckdb", "test/unittest", "benchmark/benchmark_runner"],
        )
        self.assertEqual((self.output_dir / "duckdb").read_text(), "shell")
        self.assertEqual((self.output_dir / "test" / "unittest").read_text(), "unittest")
        self.assertEqual(
            (self.output_dir / "benchmark" / "benchmark_runner").read_text(),
            "benchmark_runner",
        )

    def test_missing_binaries_are_skipped_rather_than_failing(self):
        self.make("shell")
        self.assertEqual(publish_build.publish(self.bazel_bin, self.output_dir), ["duckdb"])

    def test_a_stale_entry_is_replaced(self):
        self.output_dir.mkdir(parents=True)
        stale = self.output_dir / "duckdb"
        stale.write_text("an older build", encoding="utf-8")
        self.make("shell")

        publish_build.publish(self.bazel_bin, self.output_dir)

        self.assertEqual(stale.read_text(), "shell")

    def test_published_entries_are_the_files_not_links_into_the_bazel_cache(self):
        shell = self.make("shell")
        extension_dir = self.bazel_bin / "test" / "extension"
        extension_dir.mkdir(parents=True)
        self.make("loadable_extension_demo.duckdb_extension", directory=extension_dir)

        publish_build.publish(self.bazel_bin, self.output_dir)
        publish_build.publish_loadable_extensions(self.bazel_bin, self.output_dir)

        # The artifact scripts copy build/<type> with `cp -a`/`cp -P` and tar it,
        # so anything that is a link ships as one -- and an absolute symlink into
        # the machine-local Bazel cache resolves to nothing on the runner that
        # unpacks the artifact.
        cli = self.output_dir / "duckdb"
        extension = self.output_dir / "test" / "extension" / "loadable_extension_demo.duckdb_extension"
        self.assertFalse(cli.is_symlink())
        self.assertFalse(extension.is_symlink())
        self.assertEqual(cli.stat().st_ino, shell.stat().st_ino)

    def test_the_shared_library_is_published_under_its_platform_name(self):
        self.make("libduckdb.so.1.4.3")

        published = publish_build.publish_shared_library(self.bazel_bin, self.output_dir)

        self.assertEqual(published, ["libduckdb.so.1.4.3"])
        self.assertTrue((self.output_dir / "src" / "libduckdb.so.1.4.3").exists())

    def test_no_shared_library_is_not_an_error(self):
        self.assertEqual(publish_build.publish_shared_library(self.bazel_bin, self.output_dir), [])

    def test_loadable_extensions_are_published_by_path(self):
        extension_dir = self.bazel_bin / "test" / "extension"
        extension_dir.mkdir(parents=True)
        self.make("cpp_api_demo.duckdb_extension", directory=extension_dir)
        self.make("loadable_extension_demo.duckdb_extension", directory=extension_dir)

        published = publish_build.publish_loadable_extensions(self.bazel_bin, self.output_dir)

        self.assertEqual(
            published,
            [
                "cpp_api_demo.duckdb_extension",
                "loadable_extension_demo.duckdb_extension",
            ],
        )
        self.assertTrue((self.output_dir / "test" / "extension" / "cpp_api_demo.duckdb_extension").exists())

    def test_the_run_wrapper_points_at_the_unittest_next_to_it(self):
        run = publish_build.write_run_wrapper(self.output_dir, "unittest")

        text = run.read_text()
        self.assertIn(str(REPO_ROOT), text)
        self.assertNotIn("@PROJECT_SOURCE_DIR@", text)
        self.assertNotIn("@RUN_WRAPPER_UNITTEST_NAME@", text)
        self.assertIn('unittest_name="unittest"', text)
        self.assertTrue(os.access(run, os.X_OK))

    def test_the_wrapper_template_has_no_placeholders_left_after_expansion(self):
        template = publish_build.WRAPPER_TEMPLATE.read_text()
        self.assertIn("@PROJECT_SOURCE_DIR@", template)
        self.assertIn("@RUN_WRAPPER_UNITTEST_NAME@", template)

    def test_the_template_is_where_the_script_says_it_is(self):
        self.assertTrue(publish_build.WRAPPER_TEMPLATE.is_file())


if __name__ == "__main__":
    unittest.main()
