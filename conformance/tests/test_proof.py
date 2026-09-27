#!/usr/bin/env python3
"""Deterministic unit tests for the reuse of a passed vertical (`proof.py`).

The key must move exactly when the vertical's inputs do: its harness version,
or the content of a file it reads. Run against a throwaway Git index, no
docker. `python3 conformance/tests/test_proof.py`.
"""

import os
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import proof

FILES = {
    "crates/uze-core/src/lib.rs": "core",
    "crates/uze-integrations/src/claude.rs": "claude",
    "crates/uze-integrations/src/claude/hooks.rs": "claude hooks",
    "crates/uze-integrations/src/codex.rs": "codex",
    "crates/uze-integrations/src/registry.rs": "registry",
    "crates/uze-extensions/src/lib.rs": "extensions",
    "src/main.rs": "cli",
    "src/ui/screen.rs": "tui",
    "conformance/harnesses/codex/scenarios.py": "codex lab",
    "conformance/tests/test_gate.py": "unit",
    "conformance/README.md": "prose",
    "web/page.tsx": "site",
}


class Repository:
    def __init__(self):
        self.dir = tempfile.TemporaryDirectory()
        self.root = self.dir.name
        self.git("init", "-q")
        for path, content in FILES.items():
            self.write(path, content)

    def git(self, *args):
        subprocess.run(["git", "-C", self.root, *args], check=True, capture_output=True)

    def write(self, path, content):
        full = os.path.join(self.root, path)
        os.makedirs(os.path.dirname(full), exist_ok=True)
        with open(full, "w") as f:
            f.write(content)
        self.git("add", path)

    def digest(self, harness):
        return proof.tree_digest(harness, self.root)


class DigestTest(unittest.TestCase):
    def setUp(self):
        self.repo = Repository()
        self.addCleanup(self.repo.dir.cleanup)
        self.before = {h: self.repo.digest(h) for h in proof.HARNESSES}

    def changed_after(self, path):
        self.repo.write(path, "edited")
        return {h for h in proof.HARNESSES if self.repo.digest(h) != self.before[h]}

    def test_a_shared_crate_reaches_every_vertical(self):
        self.assertEqual(
            self.changed_after("crates/uze-core/src/lib.rs"), set(proof.HARNESSES)
        )

    def test_a_vendor_integration_reaches_only_its_vertical(self):
        self.assertEqual(
            self.changed_after("crates/uze-integrations/src/claude/hooks.rs"),
            {"claude"},
        )

    def test_a_vendor_scenario_reaches_only_its_vertical(self):
        self.assertEqual(
            self.changed_after("conformance/harnesses/codex/scenarios.py"), {"codex"}
        )

    def test_the_registry_names_every_vendor_so_it_reaches_all(self):
        self.assertEqual(
            self.changed_after("crates/uze-integrations/src/registry.rs"),
            set(proof.HARNESSES),
        )

    def test_what_no_vertical_reads_reaches_none(self):
        for path in (
            "src/ui/screen.rs",
            "crates/uze-extensions/src/lib.rs",
            "conformance/tests/test_gate.py",
            "conformance/README.md",
            "web/page.tsx",
        ):
            with self.subTest(path=path):
                self.assertEqual(self.changed_after(path), set())

    def test_a_revert_is_the_same_digest(self):
        self.repo.write("src/main.rs", "edited")
        self.repo.write("src/main.rs", FILES["src/main.rs"])
        self.assertEqual(self.repo.digest("claude"), self.before["claude"])

    def test_the_working_tree_is_not_an_input(self):
        with open(os.path.join(self.repo.root, "src/main.rs"), "w") as f:
            f.write("unstaged run output")
        self.assertEqual(self.repo.digest("claude"), self.before["claude"])


class KeyTest(unittest.TestCase):
    def test_an_unknown_version_has_no_key(self):
        self.assertEqual(proof.key("claude", "unknown", "d"), "")
        self.assertEqual(proof.key("claude", "", "d"), "")

    def test_a_harness_release_is_a_new_key(self):
        self.assertNotEqual(
            proof.key("claude", "2.1.239 (Claude Code)", "d"),
            proof.key("claude", "2.1.240 (Claude Code)", "d"),
        )

    def test_each_half_of_a_vertical_is_its_own_proof(self):
        self.assertNotEqual(
            proof.key("claude", "2.1.239", "d", "contract"),
            proof.key("claude", "2.1.239", "d", "vendor"),
        )

    def test_the_key_names_its_vertical(self):
        self.assertIn("-codex-", proof.key("codex", "codex-cli 0.160.0", "d"))


if __name__ == "__main__":
    unittest.main()
