#!/usr/bin/env python3
"""Deterministic unit tests for the provider-observation helpers.

Run without the Lab: no docker, no harness binaries.
`python3 conformance/tests/test_markers.py`.
"""

import json
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from shared.common import observed_markers


def request(path, **markers):
    return {"path": path, "summary": {"hook_markers": markers}}


class ObservedMarkersTest(unittest.TestCase):
    def test_a_marker_seen_once_stays_seen(self):
        """The regression: a model call carried the denial, then a telemetry
        batch without it arrived — the denial must survive the batch."""
        turn = [
            request("/v1/messages", **{"blocked by protect-env": False}),
            request("/v1/messages", **{"blocked by protect-env": True}),
            request("/api/event_logging/v2/batch", **{"blocked by protect-env": False}),
        ]
        self.assertTrue(
            observed_markers(turn, "hook_markers")["blocked by protect-env"]
        )

    def test_a_marker_never_seen_is_reported_false(self):
        turn = [request("/v1/messages", **{"second-handler-ran": False})]
        self.assertFalse(observed_markers(turn, "hook_markers")["second-handler-ran"])

    def test_requests_without_the_field_are_ignored(self):
        turn = [{"path": "/api/hello", "summary": {}}, {"path": "/v1/messages"}]
        self.assertEqual(observed_markers(turn, "hook_markers"), {})


class ContextPlacementTest(unittest.TestCase):
    """Where an agent's context lands, per dialect: evidence for the
    briefing contract, recorded beside the substring flags."""

    def test_each_dialect_names_its_own_part(self):
        from shared import markers

        anthropic = json.dumps(
            {
                "system": [{"type": "text", "text": "UZE_CONFORMANCE_PECAN"}],
                "messages": [{"role": "user", "content": "hello"}],
            }
        )
        openai = json.dumps(
            {
                "instructions": "base",
                "input": [
                    {
                        "role": "developer",
                        "content": [{"text": "UZE_CONFORMANCE_PECAN"}],
                    }
                ],
            }
        )
        gemini = json.dumps(
            {"systemInstruction": {"parts": [{"text": "UZE_CONFORMANCE_PECAN"}]}}
        )
        claude_md = json.dumps(
            {
                "messages": [
                    {"role": "user", "content": [{"text": "UZE_CONFORMANCE_PECAN"}]}
                ]
            }
        )
        marker = "UZE_CONFORMANCE_PECAN"
        self.assertEqual(markers.placement(anthropic, marker), ["system"])
        self.assertEqual(markers.placement(openai, marker), ["developer"])
        self.assertEqual(markers.placement(gemini, marker), ["systemInstruction"])
        self.assertEqual(markers.placement(claude_md, marker), ["user"])

    def test_a_request_without_the_marker_records_nothing(self):
        from shared import markers

        self.assertEqual(markers.summary("{}")["context_placement"], {})


if __name__ == "__main__":
    unittest.main()
