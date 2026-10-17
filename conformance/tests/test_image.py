#!/usr/bin/env python3
"""Deterministic unit tests for the Lab image's source check.

A run never drives an image built from another source: what the digest
covers, that a change to a built file changes it, and that a stale, an
unlabelled or a current image is rebuilt or left alone. Docker is mocked
out. `python3 conformance/tests/test_image.py`.
"""

import os
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from shared import image


def _repository(files):
    root = tempfile.mkdtemp(prefix="uze-lab-image-")
    subprocess.run(["git", "init", "-q", root], check=True)
    for path, text in files.items():
        absolute = os.path.join(root, path)
        os.makedirs(os.path.dirname(absolute), exist_ok=True)
        with open(absolute, "w") as handle:
            handle.write(text)
    return root


class DigestTest(unittest.TestCase):
    def test_what_the_build_reads_is_in_the_digest_and_nothing_else(self):
        self.assertTrue(image._built_from("crates/uze-core/src/lib.rs"))
        self.assertTrue(image._built_from("src/shim.rs"))
        self.assertTrue(image._built_from("conformance/Dockerfile"))
        self.assertTrue(image._built_from("conformance/_fixtures/marketplace/x.json"))
        self.assertFalse(image._built_from("conformance/contract/briefing.py"))
        self.assertFalse(image._built_from("journeys/suites/04-workspace/a.yml"))
        self.assertFalse(image._built_from("web/content/docs/a.mdx"))

    def test_an_uncommitted_change_to_a_built_file_changes_the_digest(self):
        root = _repository({"src/main.rs": "fn main() {}", "web/a.mdx": "a"})
        before = image.source_digest(root)
        with open(os.path.join(root, "web", "a.mdx"), "w") as handle:
            handle.write("b")
        self.assertEqual(
            image.source_digest(root), before, "the site is not built into the image"
        )
        with open(os.path.join(root, "src", "main.rs"), "w") as handle:
            handle.write("fn main() { println!() }")
        self.assertNotEqual(image.source_digest(root), before)


class EnsureCurrentTest(unittest.TestCase):
    def setUp(self):
        os.environ.pop(image.CHECKED, None)
        os.environ.pop("UZE_LAB_IMAGE", None)

    def _ensure(self, labels):
        """`labels`: what the image reports on each inspection."""
        with (
            mock.patch.object(image, "source_digest", return_value="current"),
            mock.patch.object(image, "image_source", side_effect=labels),
            mock.patch.object(image, "build") as build,
        ):
            image.ensure_current()
        return build

    def test_a_stale_image_is_rebuilt_before_anything_runs(self):
        self._ensure(["older", "current"]).assert_called_once()

    def test_an_unlabelled_image_is_rebuilt(self):
        self._ensure([None, "current"]).assert_called_once()

    def test_a_current_image_is_left_alone(self):
        self._ensure(["current"]).assert_not_called()

    def test_a_rebuild_that_does_not_take_stops_the_run(self):
        with self.assertRaises(SystemExit):
            self._ensure(["older", "older"])

    def test_a_pinned_image_is_not_rebuilt(self):
        os.environ["UZE_LAB_IMAGE"] = "conformance-harness:old"
        try:
            self._ensure([]).assert_not_called()
        finally:
            os.environ.pop("UZE_LAB_IMAGE", None)


if __name__ == "__main__":
    unittest.main()
