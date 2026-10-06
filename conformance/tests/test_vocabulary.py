#!/usr/bin/env python3
"""Deterministic tests for the measured tool vocabulary.

Run without the Lab: no docker, no harness binaries.
`python3 conformance/tests/test_vocabulary.py`.
"""

import json
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "shared"))

from shared import vocabulary
from shared.capture import declared_tools


class DeclaredToolsTest(unittest.TestCase):
    def test_anthropic_messages(self):
        body = {
            "tools": [
                {
                    "name": "Bash",
                    "input_schema": {"properties": {"command": {}, "timeout": {}}},
                }
            ],
            "messages": [
                {
                    "role": "assistant",
                    "content": [
                        {"type": "tool_use", "name": "Bash", "input": {"command": "ls"}}
                    ],
                }
            ],
        }
        self.assertEqual(declared_tools(body), {"Bash": ["command", "timeout"]})

    def test_openai_responses_and_namespaces(self):
        body = {
            "tools": [
                {
                    "type": "function",
                    "name": "exec_command",
                    "parameters": {"properties": {"cmd": {}}},
                },
                {
                    "type": "namespace",
                    "name": "collaboration",
                    "tools": [
                        {
                            "type": "function",
                            "name": "spawn_agent",
                            "parameters": {"properties": {"message": {}}},
                        },
                    ],
                },
                {"type": "custom", "name": "apply_patch"},
            ]
        }
        self.assertEqual(
            declared_tools(body),
            {
                "exec_command": ["cmd"],
                "collaboration.spawn_agent": ["message"],
                "apply_patch": [],
            },
        )

    def test_chat_completions(self):
        body = {
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "shell",
                        "parameters": {"properties": {"command": {}}},
                    },
                }
            ]
        }
        self.assertEqual(declared_tools(body), {"shell": ["command"]})

    def test_gemini_function_declarations_under_cloudcode_request(self):
        body = {
            "request": {
                "tools": [
                    {
                        "functionDeclarations": [
                            {
                                "name": "run_command",
                                "parametersJsonSchema": {
                                    "properties": {"CommandLine": {}}
                                },
                            },
                        ]
                    }
                ]
            }
        }
        self.assertEqual(declared_tools(body), {"run_command": ["CommandLine"]})

    def test_a_declaration_without_parameters_is_still_a_tool(self):
        body = {"tools": [{"functionDeclarations": [{"name": "generate_image"}]}]}
        self.assertEqual(declared_tools(body), {"generate_image": []})

    def test_a_chat_tool_call_is_not_a_declaration(self):
        body = {
            "messages": [
                {
                    "role": "assistant",
                    "tool_calls": [
                        {
                            "id": "c1",
                            "type": "function",
                            "function": {"name": "shell", "arguments": "{}"},
                        },
                    ],
                }
            ]
        }
        self.assertEqual(declared_tools(body), {})

    def test_a_tool_call_is_not_a_declaration(self):
        body = {
            "input": [
                {"type": "function_call", "name": "exec_command", "arguments": "{}"}
            ]
        }
        self.assertEqual(declared_tools(body), {})


class EvaluateTest(unittest.TestCase):
    def setUp(self):
        self.results = []
        self.declared = []
        self.expectation = {
            "events": ["pre_tool_use"],
            "effects": ["deny"],
            "aliases": {
                "shell": {"tools": ["Bash"], "fields": {"command": "command"}},
                "agent.message": None,
            },
        }
        self.snapshot = {
            "harness_version": "1.0",
            "tools": {"Bash": ["command"]},
            "hook_tools": {"Bash": ["command"]},
        }
        for name, value in (
            ("load_expectation", self.expectation),
            ("load_snapshot", self.snapshot),
        ):
            self.addCleanup(setattr, vocabulary, name, getattr(vocabulary, name))
            setattr(vocabulary, name, lambda harness, value=value: value)

    def check(self, name, ok, detail=""):
        self.results.append((name, bool(ok), detail))

    def declare(self, name, holds, reason, evidence=""):
        self.declared.append((name, holds))

    def verdict(self, name):
        return next(ok for check, ok, _ in self.results if check == name)

    def test_an_empty_capture_fails_and_stops(self):
        vocabulary.evaluate("claude", {}, {}, self.check, self.declare)
        self.assertEqual(
            [(n, ok) for n, ok, _ in self.results], [("vocabulary-captured", False)]
        )

    def test_a_tool_the_model_is_not_offered_cannot_be_scripted(self):
        vocabulary.evaluate(
            "claude", {"Shell": ["command"]}, {}, self.check, self.declare
        )
        self.assertFalse(self.verdict("vocabulary-shell-scriptable"))

    def test_a_renamed_hook_side_tool_fails_its_alias(self):
        vocabulary.evaluate(
            "claude",
            {"Bash": ["command"]},
            {"Shell": ["command"]},
            self.check,
            self.declare,
        )
        self.assertFalse(self.verdict("vocabulary-shell-hooked"))

    def test_a_renamed_hook_side_field_fails_its_alias(self):
        vocabulary.evaluate(
            "claude", {"Bash": ["command"]}, {"Bash": ["cmd"]}, self.check, self.declare
        )
        self.assertFalse(self.verdict("vocabulary-shell-hooked"))

    def test_a_run_without_the_census_says_nothing_of_the_hook_side(self):
        vocabulary.evaluate(
            "claude", {"Bash": ["command"]}, {}, self.check, self.declare
        )
        self.assertNotIn(
            "vocabulary-shell-hooked", [name for name, _, _ in self.results]
        )

    def test_a_vendor_change_to_a_bound_field_asks_for_a_new_snapshot(self):
        self.snapshot["hook_tools"] = {"Bash": ["cmd"]}
        vocabulary.evaluate(
            "claude",
            {"Bash": ["command"]},
            {"Bash": ["command"]},
            self.check,
            self.declare,
        )
        self.assertFalse(self.verdict("vocabulary-snapshot-current"))

    def test_an_unbound_field_varying_does_not_ask_for_a_new_snapshot(self):
        vocabulary.evaluate(
            "claude",
            {"Bash": ["command", "sandbox"]},
            {"Bash": ["command", "x"]},
            self.check,
            self.declare,
        )
        self.assertTrue(self.verdict("vocabulary-snapshot-current"))

    def test_a_matching_capture_passes(self):
        vocabulary.evaluate(
            "claude",
            {"Bash": ["command"], "Other": []},
            {"Bash": ["command"]},
            self.check,
            self.declare,
        )
        self.assertTrue(all(ok for _, ok, _ in self.results))

    def test_an_optional_tool_not_offered_is_declared_not_failed(self):
        self.expectation["aliases"]["shell"]["optional"] = "offered only on opt-in"
        vocabulary.evaluate(
            "claude", {"Other": []}, {"Other": []}, self.check, self.declare
        )
        self.assertEqual(self.declared, [("vocabulary-shell-scriptable", True)])
        self.assertNotIn("vocabulary-shell-scriptable", [n for n, _, _ in self.results])

    def test_an_optional_tool_offered_escalates_its_declaration(self):
        self.expectation["aliases"]["shell"]["optional"] = "offered only on opt-in"
        vocabulary.evaluate(
            "claude",
            {"Bash": ["command"]},
            {"Bash": ["command"]},
            self.check,
            self.declare,
        )
        self.assertEqual(self.declared, [("vocabulary-shell-scriptable", False)])

    def test_an_optional_tool_another_session_was_offered_is_still_declared(self):
        self.expectation["aliases"]["shell"]["optional"] = "offered only on opt-in"
        vocabulary.evaluate(
            "claude",
            {"Bash": ["command"]},
            {"Other": []},
            self.check,
            self.declare,
            refused=frozenset({"Bash"}),
        )
        self.assertEqual(self.declared, [("vocabulary-shell-scriptable", True)])

    def test_an_optional_tool_says_nothing_without_the_rows_scene(self):
        self.expectation["aliases"]["shell"]["optional"] = "offered only on opt-in"
        vocabulary.evaluate(
            "claude", {"Bash": ["command"]}, {}, self.check, self.declare
        )
        self.assertEqual(self.declared, [])
        self.assertNotIn("vocabulary-shell-scriptable", [n for n, _, _ in self.results])

    def test_the_census_reads_native_names_and_input_fields(self):
        records = [
            {
                "HOOK_TOOL_NATIVE": "Bash",
                "HOOK_INPUT": '{"command": "ls", "description": "x"}',
            }
        ]
        self.assertEqual(
            vocabulary.census(records), {"Bash": ["command", "description"]}
        )


class RowsFixtureTest(unittest.TestCase):
    def test_every_alias_a_harness_knows_has_a_row_group(self):
        """The rows scene reads each alias's own group; one the fixture
        lacks records nothing, which reads as the harness not firing."""
        root = os.path.join(os.path.dirname(__file__), "..")
        fixture = os.path.join(
            root, "_fixtures", "marketplace", "plugins", "hook-rows", "hooks.json"
        )
        with open(fixture) as f:
            groups = {
                group.get("id"): group.get("matcher")
                for group in json.load(f)["hooks"]["PreToolUse"]
            }
        harnesses = os.path.join(root, "harnesses")
        missing = []
        for harness in sorted(os.listdir(harnesses)):
            path = os.path.join(harnesses, harness, "vocabulary.json")
            if not os.path.isfile(path):
                continue
            with open(path) as f:
                aliases = json.load(f)["aliases"]
            for alias, row in aliases.items():
                label = "row-" + alias.replace(".", "-")
                if row and groups.get(label) != alias:
                    missing.append(f"{harness}: {label} matching `{alias}`")
        self.assertEqual(missing, [])


if __name__ == "__main__":
    unittest.main()
