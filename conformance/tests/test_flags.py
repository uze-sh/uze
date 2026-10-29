#!/usr/bin/env python3
"""Deterministic unit tests for reading the flags a `--help` defines, and
for the snapshots each vertical declares.

Run without the Lab: no docker, no harness binaries.
`python3 conformance/tests/test_flags.py`.
"""

import importlib
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, ROOT)

from contract.bindings import Bindings
from contract.flags import defined_flags, describe_drift, drift

HELP = """Usage: x [--last]

Options:
  -c, --continue <id>    resume; see --last
      --enable <F>
          like --other
  --session, -s string   a session
"""


class DefinedFlagsTest(unittest.TestCase):
    def test_only_an_options_own_line_defines_a_flag(self):
        """A flag named in a usage line or a description is not defined by
        it: counting those would keep a removed flag alive."""
        self.assertEqual(
            defined_flags(HELP), {"-c", "--continue", "--enable", "--session", "-s"}
        )

    def test_a_reworded_description_is_not_drift(self):
        reworded = HELP.replace("a session", "the session to open")
        self.assertEqual(drift(HELP, reworded), ([], []))

    def test_drift_names_what_appeared_and_what_vanished(self):
        release = HELP.replace("--enable <F>", "--brand-new <F>")
        appeared, vanished = drift(HELP, release)
        self.assertEqual((appeared, vanished), (["--brand-new"], ["--enable"]))
        detail = describe_drift("help.txt", appeared, vanished)
        self.assertIn("appeared: --brand-new", detail)
        self.assertIn("vanished: --enable", detail)


class SnapshotsTest(unittest.TestCase):
    def test_every_vertical_declares_snapshots_that_exist_and_define_flags(self):
        repository = os.path.dirname(ROOT)
        for harness in ("claude", "codex", "opencode", "antigravity"):
            module = importlib.import_module(f"harnesses.{harness}.bindings")
            bindings = next(
                value
                for value in vars(module).values()
                if isinstance(value, type)
                and issubclass(value, Bindings)
                and value is not Bindings
            )
            self.assertTrue(bindings.help_snapshots, harness)
            for _, snapshot in bindings.help_snapshots:
                with open(os.path.join(repository, snapshot)) as file:
                    self.assertTrue(defined_flags(file.read()), snapshot)


if __name__ == "__main__":
    unittest.main()
