#!/usr/bin/env python3
"""Deterministic unit tests for the declared-limitation gate (ADR-035).

Run without the Lab: no docker, no harness binaries — pure registry and
adjudication semantics. `python3 conformance/tests/test_gate.py`.
"""

import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import gate

ENTRY = {
    "harness": "codex",
    "check": "skill-user-only-is-not-model-invocable",
    "suite": "skill",
    "reason": "no documented control",
    "measurement": "the model invoked the skill",
    "versions": ["0.150.0"],
    "observed_at": "2026-10-05",
}


def registry_file(entries):
    fd, path = tempfile.mkstemp(suffix=".json")
    with open(fd, "w") as f:
        json.dump({"adaptive": entries}, f)
    return path


def registry(*entries):
    return gate.load_registry(registry_file(list(entries)))


def declared(name=ENTRY["check"], holds=True, harness="codex", version="0.150.0"):
    return {
        "check": name,
        "suite": "skill",
        "pass": holds is True,
        "detail": "",
        "kind": gate.DECLARED,
        "holds": holds,
        "harness": harness,
        "harness_version": version,
    }


def asserted(name, ok=True, harness="codex", version="0.150.0"):
    return {
        "check": name,
        "suite": "skill",
        "pass": ok,
        "detail": "",
        "kind": "assert",
        "harness": harness,
        "harness_version": version,
    }


def adjudicate(result, *entries):
    [out] = gate.evaluate("codex", [result], registry(*entries))
    return out["pass"], out["gate"]["adjudication"]


class LoadRegistryTest(unittest.TestCase):
    def test_missing_registry_is_an_error_not_an_empty_gate(self):
        with self.assertRaises(OSError):
            gate.load_registry("/nonexistent/expected.json")

    def test_parses_entries_into_harness_check_map(self):
        self.assertIn(("codex", ENTRY["check"]), registry(ENTRY))


class DeclarationTest(unittest.TestCase):
    def test_a_measured_registered_declaration_passes(self):
        self.assertEqual(adjudicate(declared(), ENTRY), (True, "known_declared"))

    def test_an_unregistered_declaration_fails(self):
        self.assertEqual(adjudicate(declared()), (False, "unregistered"))

    def test_another_harness_entry_never_matches(self):
        entry = {**ENTRY, "harness": "antigravity"}
        self.assertEqual(adjudicate(declared(), entry), (False, "unregistered"))

    def test_a_wildcard_entry_fails(self):
        entry = {**ENTRY, "versions": ["*"]}
        self.assertEqual(adjudicate(declared(), entry), (False, "wildcard"))

    def test_an_entry_with_no_versions_fails(self):
        entry = {**ENTRY, "versions": []}
        self.assertEqual(adjudicate(declared(), entry), (False, "wildcard"))

    def test_a_declaration_whose_measurement_did_not_run_fails(self):
        self.assertEqual(adjudicate(declared(holds=None), ENTRY), (False, "unproven"))

    def test_a_declaration_that_found_the_control_escalates(self):
        self.assertEqual(adjudicate(declared(holds=False), ENTRY), (False, "escalated"))

    def test_a_reproduced_limitation_on_a_new_version_passes_as_repin(self):
        result = declared(version="0.160.0")
        self.assertEqual(adjudicate(result, ENTRY), (True, "repin"))

    def test_escalation_wins_over_a_new_version(self):
        result = declared(holds=False, version="0.160.0")
        self.assertEqual(adjudicate(result, ENTRY), (False, "escalated"))


class AssertionTest(unittest.TestCase):
    def test_a_plain_assertion_is_untouched(self):
        self.assertEqual(
            adjudicate(asserted("plain", ok=False), ENTRY), (False, "asserted")
        )

    def test_a_registered_check_recording_an_ordinary_pass_escalates(self):
        self.assertEqual(
            adjudicate(asserted(ENTRY["check"]), ENTRY), (False, "escalated")
        )


class NextRegistryTest(unittest.TestCase):
    def test_repins_append_the_observed_version(self):
        path = registry_file([ENTRY])
        results = gate.evaluate(
            "codex", [declared(version="0.160.0")], gate.load_registry(path)
        )
        updated = gate.next_registry(results, path)
        self.assertEqual(updated["adaptive"][0]["versions"], ["0.150.0", "0.160.0"])

    def test_no_repin_writes_nothing(self):
        path = registry_file([ENTRY])
        results = gate.evaluate("codex", [declared()], gate.load_registry(path))
        self.assertIsNone(gate.next_registry(results, path))


if __name__ == "__main__":
    unittest.main()
