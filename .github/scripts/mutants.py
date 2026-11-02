#!/usr/bin/env python3
"""The arithmetic of a mutation run: `.github/workflows/mutants.yml`.

A run is cut into shards that know nothing of each other, so what they found
only means something once it is joined, and only against how many mutants were
planned: a shard that ran out of time has no entry for what it never tried, and
without the plan a partial run reads as a complete one.
"""

import argparse
import json
import statistics
import sys
from collections import Counter
from pathlib import Path

MOST_SHARDS = 256
FILES_SHOWN = 25
CAUGHT = "CaughtMutant"
MISSED = "MissedMutant"
TIMEOUT = "Timeout"
UNVIABLE = "Unviable"


def shard_matrix(shards: int) -> list[int]:
    if not 1 <= shards <= MOST_SHARDS:
        raise ValueError(f"shards must be between 1 and {MOST_SHARDS}, got {shards}")
    return list(range(shards))


def mutant_outcomes(document: dict) -> list[dict]:
    """A run that measured its own baseline lists it among the outcomes too."""
    return [
        outcome
        for outcome in document["outcomes"]
        if isinstance(outcome["scenario"], dict) and "Mutant" in outcome["scenario"]
    ]


def read_shards(directory: Path) -> tuple[list[dict], int]:
    outcomes = []
    documents = sorted(directory.glob("**/outcomes.json"))
    for path in documents:
        outcomes.extend(mutant_outcomes(json.loads(path.read_text())))
    return outcomes, len(documents)


def seconds_spent(outcome: dict) -> float:
    return sum(phase["duration"] for phase in outcome.get("phase_results", []))


def detection(counts: Counter) -> float | None:
    """Unviable mutants never ran, so they count for nothing either way."""
    detected = counts[CAUGHT] + counts[TIMEOUT]
    judged = detected + counts[MISSED]
    return detected / judged if judged else None


def missed_names(outcomes: list[dict]) -> list[str]:
    return sorted(
        outcome["scenario"]["Mutant"]["name"]
        for outcome in outcomes
        if outcome["summary"] == MISSED
    )


def render(
    package: str, commit: str, planned: int, outcomes: list[dict], shards: int
) -> str:
    counts = Counter(outcome["summary"] for outcome in outcomes)
    tried = len(outcomes)
    score = detection(counts)
    lines = [
        f"## Mutants: `{package}` at `{commit[:12]}`",
        "",
        (
            f"**Detected: {'n/a' if score is None else f'{score:.1%}'}**"
            " (caught and hung, of those that compiled)"
        ),
        "",
        "| Tried | Caught | Hung | Survived | Did not compile |",
        "|---|---|---|---|---|",
        (
            f"| {tried} of {planned} | {counts[CAUGHT]} | {counts[TIMEOUT]}"
            f" | {counts[MISSED]} | {counts[UNVIABLE]} |"
        ),
        "",
    ]
    if tried < planned:
        lines += [
            (
                f"> **Incomplete.** {planned - tried} mutants were never tried, so this"
                " score describes part of the crate. Add shards, or re-run the ones"
                " that failed."
            ),
            "",
        ]
    if outcomes:
        durations = [seconds_spent(outcome) for outcome in outcomes]
        lines += [
            (
                f"Measured over {shards} shards: {statistics.median(durations):.0f}s median per"
                f" mutant, {sum(durations) / 3600:.1f} runner-hours in all."
            ),
            "",
        ]
    survivors = Counter(
        outcome["scenario"]["Mutant"]["file"]
        for outcome in outcomes
        if outcome["summary"] == MISSED
    )
    if survivors:
        lines += ["| File | Survived |", "|---|---|"]
        lines += [
            f"| `{file}` | {count} |"
            for file, count in survivors.most_common(FILES_SHOWN)
        ]
        if len(survivors) > FILES_SHOWN:
            lines.append(f"| {len(survivors) - FILES_SHOWN} more files | |")
        lines += ["", "Every survivor is in this run's `missed.txt` artifact."]
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    matrix = commands.add_parser("matrix", help="print the shard numbers as JSON")
    matrix.add_argument("shards", type=int)
    report = commands.add_parser("report", help="join the shards and print a summary")
    report.add_argument("directory", type=Path)
    report.add_argument("--package", required=True)
    report.add_argument("--commit", required=True)
    report.add_argument("--planned", type=int, required=True)
    report.add_argument("--missed", type=Path, required=True)
    arguments = parser.parse_args()

    if arguments.command == "matrix":
        try:
            print(json.dumps(shard_matrix(arguments.shards)))
        except ValueError as error:
            print(error, file=sys.stderr)
            return 1
        return 0

    outcomes, shards = read_shards(arguments.directory)
    names = missed_names(outcomes)
    arguments.missed.write_text("".join(f"{name}\n" for name in names))
    print(
        render(
            arguments.package, arguments.commit, arguments.planned, outcomes, shards
        ),
        end="",
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
