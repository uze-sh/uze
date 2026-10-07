#!/usr/bin/env python3
"""Deterministic unit tests for how a hook scene's turn is judged to end.

No docker, no harness binaries: a scripted TUI hands the driver its reads.
`python3 conformance/tests/test_hook_turn.py`.
"""

import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from contract.bindings import Bindings
from shared.common import ansi_strip, render_screen


class ScriptedTui:
    """Replays `reads` one wait at a time, recording them as the session's
    record the way `CastRecorder` does."""

    def __init__(self, reads):
        self.reads = list(reads)
        self.record = ""

    def type(self, text):
        pass

    def submit(self):
        pass

    def wait_for(self, markers, tries, stop_on_death, squash_spaces):
        raw = self.reads.pop(0) if self.reads else ""
        self.record += raw
        plain = ansi_strip(raw)
        matched = next((m for m in markers if m in plain), None)
        return raw, plain, matched

    def shown(self):
        return render_screen(self.record)

    def quiet(self):
        return True

    def transcript(self):
        return ansi_strip(self.record)


class HookTurnTest(unittest.TestCase):
    def test_a_final_text_split_across_reads_ends_the_turn(self):
        """Antigravity 1.3.0: the answer's first half, a spinner redraw,
        then `cursor up` + `cursor forward` and the rest, each in a read of
        its own — so no read ever holds the marker whole."""
        tui = ScriptedTui(
            [
                "  UZE_CONFORMA\x1b[K\r\n⣯  Running command...\r\n\r\n>\x1b[K\r\n",
                "\x1b[4A\x1b[14CNCE_PASS\r\n\x1b[K\r\n? for shortcuts",
            ]
        )
        turn = Bindings().hook_turn(tui, "run the lab checks", tries=4)
        self.assertTrue(turn.settled, turn.detail)
        self.assertEqual(tui.reads, [], "the turn ended on the read that finished it")

    def test_a_final_text_from_an_earlier_turn_does_not_end_this_one(self):
        tui = ScriptedTui(["still working\r\n", "still working\r\n"])
        tui.record = "UZE_CONFORMANCE_PASS\r\n"
        turn = Bindings().hook_turn(tui, "run the lab checks", tries=2)
        self.assertFalse(turn.settled, turn.detail)


if __name__ == "__main__":
    unittest.main()
