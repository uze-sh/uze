#!/usr/bin/env python3
"""Deterministic unit tests for reusing a passed leg (`proof.py`).

The key must move exactly when a leg's inputs do: what it drives from outside
the repository, or the content of a file it reads. Run against a throwaway
Git index, no docker, no network. `python3 .github/scripts/test_proof.py`.
"""

import os
import subprocess
import sys
import tempfile
import unittest
from typing import ClassVar

sys.path.insert(0, os.path.dirname(__file__))

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
    "journeys/suites/01-first-run/01-a.yml": "journey",
    "journeys/README.md": "journey prose",
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

    def digests(self):
        lab = {
            h: proof.tree_digest(proof.lab_pathspecs(h), self.root)
            for h in proof.HARNESSES
        }
        return {
            **lab,
            "journeys": proof.tree_digest(proof.journey_pathspecs(), self.root),
        }


class DigestTest(unittest.TestCase):
    def setUp(self):
        self.repo = Repository()
        self.addCleanup(self.repo.dir.cleanup)
        self.before = self.repo.digests()

    def reached_by(self, path):
        self.repo.write(path, "edited")
        after = self.repo.digests()
        return {leg for leg in after if after[leg] != self.before[leg]}

    def test_a_shared_crate_reaches_every_leg(self):
        self.assertEqual(
            self.reached_by("crates/uze-core/src/lib.rs"),
            {*proof.HARNESSES, "journeys"},
        )

    def test_a_vendor_integration_reaches_its_vertical_and_the_journeys(self):
        self.assertEqual(
            self.reached_by("crates/uze-integrations/src/claude/hooks.rs"),
            {"claude", "journeys"},
        )

    def test_a_vendor_scenario_reaches_only_its_vertical(self):
        self.assertEqual(
            self.reached_by("conformance/harnesses/codex/scenarios.py"), {"codex"}
        )

    def test_the_registry_names_every_vendor_so_it_reaches_all(self):
        self.assertEqual(
            self.reached_by("crates/uze-integrations/src/registry.rs"),
            {*proof.HARNESSES, "journeys"},
        )

    def test_the_terminal_ui_reaches_the_journeys_and_no_vertical(self):
        for path in ("src/ui/screen.rs", "crates/uze-extensions/src/lib.rs"):
            with self.subTest(path=path):
                self.assertEqual(self.reached_by(path), {"journeys"})

    def test_a_journey_reaches_only_the_journeys(self):
        self.assertEqual(
            self.reached_by("journeys/suites/01-first-run/01-a.yml"), {"journeys"}
        )

    def test_what_no_leg_reads_reaches_none(self):
        for path in (
            "conformance/tests/test_gate.py",
            "conformance/README.md",
            "journeys/README.md",
            "web/page.tsx",
        ):
            with self.subTest(path=path):
                self.assertEqual(self.reached_by(path), set())

    def test_a_revert_is_the_same_digest(self):
        self.repo.write("src/main.rs", "edited")
        self.repo.write("src/main.rs", FILES["src/main.rs"])
        self.assertEqual(self.repo.digests(), self.before)

    def test_the_working_tree_is_not_an_input(self):
        with open(os.path.join(self.repo.root, "src/main.rs"), "w") as f:
            f.write("unstaged run output")
        self.assertEqual(self.repo.digests(), self.before)


class KeyTest(unittest.TestCase):
    def test_an_unknown_outside_has_no_key(self):
        self.assertEqual(proof.key("lab", "claude-vendor", "unknown", "d"), "")
        self.assertEqual(proof.key("lab", "claude-vendor", "", "d"), "")

    def test_a_harness_release_is_a_new_key(self):
        self.assertNotEqual(
            proof.key("lab", "claude-vendor", "2.1.239 (Claude Code)", "d"),
            proof.key("lab", "claude-vendor", "2.1.240 (Claude Code)", "d"),
        )

    def test_each_leg_is_its_own_proof(self):
        self.assertNotEqual(
            proof.key("lab", "claude-contract", "2.1.239", "d"),
            proof.key("lab", "claude-vendor", "2.1.239", "d"),
        )
        self.assertNotEqual(
            proof.key("journeys", "linux-1of4", "ubuntu24-20260921.1", "d"),
            proof.key("journeys", "macos-1of4", "ubuntu24-20260921.1", "d"),
        )

    def test_a_runner_image_update_is_a_new_key(self):
        self.assertNotEqual(
            proof.key("journeys", "linux-1of4", "ubuntu24-20260921.1", "d"),
            proof.key("journeys", "linux-1of4", "ubuntu24-20260928.1", "d"),
        )

    def test_the_key_names_its_leg(self):
        self.assertIn(
            "-lab-codex-vendor-", proof.key("lab", "codex-vendor", "0.160.0", "d")
        )


class LookupTest(unittest.TestCase):
    ENV: ClassVar[dict] = {
        "GITHUB_REPOSITORY": "uze-sh/uze",
        "GITHUB_SERVER_URL": "https://github.com",
    }

    def artifact(self, run_id, head_repository_id, expired=False):
        return {
            "expired": expired,
            "workflow_run": {"id": run_id, "head_repository_id": head_repository_id},
        }

    def test_a_proof_from_this_repository_names_its_run(self):
        found = proof.first_proof([self.artifact(7, 1)], 1, self.ENV)
        self.assertEqual(found, "https://github.com/uze-sh/uze/actions/runs/7")

    def test_a_proof_from_a_fork_is_never_taken(self):
        self.assertEqual(proof.first_proof([self.artifact(7, 99)], 1, self.ENV), "")

    def test_an_expired_proof_is_never_taken(self):
        self.assertEqual(
            proof.first_proof([self.artifact(7, 1, expired=True)], 1, self.ENV), ""
        )

    def test_no_token_means_the_leg_runs(self):
        self.assertEqual(proof.proven_by("proof-v2-lab-x", env={}), "")


if __name__ == "__main__":
    unittest.main()
