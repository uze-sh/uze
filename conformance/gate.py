"""Declared-limitation gate for the Conformance Lab (ADR-035).

Deterministic gate logic over verdict entries. A checked-in registry
(`conformance/evidence/expected.json`) lists every check that may record a
*declared* limitation: a harness measured, in this run, to lack a control
the contract covers. The gate turns the registry into the
anti-false-positive contract:

- a declaration without a registry entry FAILS the run;
- an entry naming a wildcard version FAILS: a version list is the record of
  where the limitation was measured, and `*` is a claim nobody measured;
- a declaration whose measurement did not run FAILS as unproven;
- a declaration whose measurement shows the control now exists FAILS as
  escalated, until the check is promoted and the entry removed — as does
  a registered check that records an ordinary pass;
- a declaration that reproduced on a version its entry does not name yet
  PASSES as `repin`, and the run publishes the registry with that version
  added (`next_registry`). The measurement is what makes it safe: a
  limitation re-observed on the new release is evidence, and failing on
  every vendor release would train re-pinning without looking.

Pure functions only — no docker, no harness knowledge — so the semantics
are unit-tested without the Lab (ADR-035: a silent change can never pass).
"""

from __future__ import annotations

import copy
import json
import os
from typing import Any

#: Refused in an entry's `versions`. Kept named so the refusal can say why.
ANY_VERSION = "*"

#: The one kind a declared limitation is recorded with (`common.declare`).
DECLARED = "declared"

REGISTRY_PATH = os.path.join(os.path.dirname(__file__), "evidence", "expected.json")


def load_registry(path: str | None = None) -> dict[tuple[str, str], dict[str, Any]]:
    """Reads the registry into a {(harness, check): entry} map.

    A missing or unreadable registry is an error, never an empty gate: the
    safe default is that every declaration is unexpected (fails), not that
    everything passes.
    """
    with open(path or REGISTRY_PATH) as f:
        document = json.load(f)
    return {
        (entry["harness"], entry["check"]): entry
        for entry in document.get("adaptive", [])
    }


def evaluate(
    harness: str,
    results: list[dict[str, Any]],
    registry: dict[tuple[str, str], dict[str, Any]],
) -> list[dict[str, Any]]:
    """Applies the gate to verdict entries in place and returns them.

    Each verdict gains `gate`: `{"adjudication": ..., "reason": ...}`.
    Adjudications: `asserted` (ordinary pass/fail), `known_declared`,
    `repin`, `unregistered`, `wildcard`, `unproven`, `escalated`.

    A declaration carries `holds`: True when this run measured the
    limitation, False when the measurement found the control, None when no
    measurement ran.
    """
    for result in results:
        key = (result.get("harness") or harness, result["check"])
        entry = registry.get(key)
        adjudication, reason = "asserted", None
        if result.get("kind") == DECLARED:
            adjudication, reason = _declaration(result, entry)
            result["pass"] = adjudication in ("known_declared", "repin")
        elif entry is not None and result["pass"]:
            result["pass"] = False
            adjudication = "escalated"
            reason = (
                "a registered declaration now records an ordinary pass — promote the "
                "check and remove its entry from expected.json"
            )
        result["gate"] = {"adjudication": adjudication, "reason": reason}
    return results


def _declaration(result, entry):
    if entry is None:
        return "unregistered", (
            "unregistered declaration — measure the limitation and register it in "
            "conformance/evidence/expected.json with the versions it was observed on"
        )
    versions = entry.get("versions") or []
    if ANY_VERSION in versions or not versions:
        return "wildcard", (
            f"the entry names {versions!r}; name the harness versions the limitation "
            "was measured on"
        )
    holds = result.get("holds")
    if holds is None:
        return "unproven", "the measurement behind this declaration did not run"
    if not holds:
        return "escalated", (
            "the measurement found the control this entry says is missing — promote "
            "the check and remove the entry"
        )
    version = result.get("harness_version")
    if version and version not in versions:
        return (
            "repin",
            f"reproduced on {version}; re-pin the entry (see expected.next.json)",
        )
    return "known_declared", None


def next_registry(
    results: list[dict[str, Any]], path: str | None = None
) -> dict | None:
    """The registry with every `repin` version added, or None when no
    declaration reproduced on a version its entry does not name yet."""
    repins = {
        (r.get("harness"), r["check"]): r.get("harness_version")
        for r in results
        if r.get("gate", {}).get("adjudication") == "repin"
    }
    if not repins:
        return None
    with open(path or REGISTRY_PATH) as f:
        document = json.load(f)
    updated = copy.deepcopy(document)
    for entry in updated.get("adaptive", []):
        version = repins.get((entry["harness"], entry["check"]))
        if version and version not in entry["versions"]:
            entry["versions"].append(version)
    return updated


def gate_failures(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """The verdict entries the gate itself failed (not ordinary assertion
    failures)."""
    return [
        r
        for r in results
        if not r["pass"] and r.get("gate", {}).get("adjudication") != "asserted"
    ]
