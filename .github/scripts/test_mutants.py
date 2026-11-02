#!/usr/bin/env python3
"""Deterministic unit tests for joining a mutation run's shards (`mutants.py`).

The shape of each `outcomes.json` is the one cargo-mutants 27.1.0 writes, so a
version bump that changes it fails here rather than in a run hours long.
`python3 .github/scripts/test_mutants.py`.
"""

import json
import os
import sys
import tempfile
import unittest
from collections import Counter
from pathlib import Path

sys.path.insert(0, os.path.dirname(__file__))

import mutants


def outcome(summary: str, file: str = "crates/a/src/lib.rs", line: int = 1) -> dict:
    return {
        "scenario": {
            "Mutant": {
                "name": f"{file}:{line}:9: replace f -> bool with true",
                "package": "a",
                "file": file,
            }
        },
        "summary": summary,
        "phase_results": [
            {"phase": "Build", "duration": 2.0},
            {"phase": "Test", "duration": 8.0},
        ],
    }


BASELINE = {"scenario": "Baseline", "summary": "Success", "phase_results": []}


def write_shard(root: Path, shard: int, outcomes: list[dict]) -> None:
    directory = root / f"mutants-shard-{shard}"
    directory.mkdir()
    (directory / "outcomes.json").write_text(json.dumps({"outcomes": outcomes}))


class Matrix(unittest.TestCase):
    def test_a_shard_is_numbered_from_zero(self):
        self.assertEqual(mutants.shard_matrix(3), [0, 1, 2])

    def test_a_count_github_would_refuse_is_refused_here(self):
        for count in (0, -1, mutants.MOST_SHARDS + 1):
            with self.assertRaises(ValueError):
                mutants.shard_matrix(count)
        self.assertEqual(len(mutants.shard_matrix(mutants.MOST_SHARDS)), 256)


class Detection(unittest.TestCase):
    def test_a_hang_is_detected_and_what_did_not_compile_is_not_judged(self):
        counts = Counter(
            {
                mutants.CAUGHT: 2,
                mutants.TIMEOUT: 1,
                mutants.MISSED: 1,
                mutants.UNVIABLE: 50,
            }
        )
        self.assertEqual(mutants.detection(counts), 0.75)

    def test_nothing_judged_has_no_score_rather_than_a_perfect_one(self):
        self.assertIsNone(mutants.detection(Counter({mutants.UNVIABLE: 3})))


class Joining(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)

    def report(self, planned: int) -> tuple[str, list[str]]:
        outcomes, shards = mutants.read_shards(self.root)
        text = mutants.render("uze-core", "abcdef0123456789", planned, outcomes, shards)
        return text, mutants.missed_names(outcomes)

    def test_every_shard_is_counted_and_a_baseline_is_not_a_mutant(self):
        write_shard(
            self.root, 0, [BASELINE, outcome(mutants.CAUGHT), outcome(mutants.MISSED)]
        )
        write_shard(self.root, 1, [outcome(mutants.CAUGHT), outcome(mutants.CAUGHT)])
        text, _ = self.report(planned=4)
        self.assertIn("4 of 4", text)
        self.assertIn("75.0%", text)
        self.assertIn("over 2 shards", text)
        self.assertNotIn("Incomplete", text)

    def test_a_shard_that_never_reported_makes_the_run_incomplete(self):
        write_shard(self.root, 0, [outcome(mutants.CAUGHT)])
        text, _ = self.report(planned=10)
        self.assertIn("1 of 10", text)
        self.assertIn("**Incomplete.** 9 mutants", text)

    def test_no_shard_at_all_reports_no_score(self):
        text, names = self.report(planned=5)
        self.assertIn("Detected: n/a", text)
        self.assertIn("**Incomplete.** 5 mutants", text)
        self.assertEqual(names, [])

    def test_survivors_are_listed_whole_and_grouped_by_file_worst_first(self):
        write_shard(
            self.root,
            0,
            [
                outcome(mutants.MISSED, "b.rs", 2),
                outcome(mutants.MISSED, "a.rs", 7),
                outcome(mutants.MISSED, "b.rs", 1),
                outcome(mutants.CAUGHT, "c.rs"),
            ],
        )
        text, names = self.report(planned=4)
        self.assertLess(text.index("| `b.rs` | 2 |"), text.index("| `a.rs` | 1 |"))
        self.assertNotIn("c.rs", text)
        self.assertEqual(
            names,
            [
                "a.rs:7:9: replace f -> bool with true",
                "b.rs:1:9: replace f -> bool with true",
                "b.rs:2:9: replace f -> bool with true",
            ],
        )

    def test_the_summary_names_files_past_the_limit_only_as_a_count(self):
        write_shard(
            self.root,
            0,
            [
                outcome(mutants.MISSED, f"{n}.rs")
                for n in range(mutants.FILES_SHOWN + 2)
            ],
        )
        text, names = self.report(planned=mutants.FILES_SHOWN + 2)
        self.assertIn("| 2 more files | |", text)
        self.assertEqual(len(names), mutants.FILES_SHOWN + 2)


if __name__ == "__main__":
    unittest.main()
