"""The tool vocabulary, measured from the real harness.

A portable hook names a tool by alias (`shell`, `file.write`); what it
intercepts is the native tool each harness's hook system reports for that
kind of call. That native name is not always the one the model is offered:
Codex offers its model a code-mode `exec` and reports shell calls to hooks
under a name of its own. So the vocabulary has two sides, and both are
measured, never written from memory:

- the **model side**: the tools the harness declares to the model, which
  every provider records (`capture.declared_tools`) and which bound what a
  scene may script (`capture.scriptable`);
- the **hook side**: the name and input fields a call reaches a hook with,
  recorded by the hooks contract's census group (a hook with no matcher in
  `hook-rows`, whose handler writes `HOOK_TOOL_NATIVE` and `HOOK_INPUT`).

Two lists claim to know the hook side, written independently on purpose:
UZE's binding tables (`crates/uze-integrations/src/<h>/hooks.rs`) and the
Lab's own expectation (`harnesses/<h>/vocabulary.json`). The run checks the
Lab's against the measurement; the deterministic suite checks UZE's against
the committed snapshot (`evidence/tools/<h>.json`) and against the Lab's. A
vendor that renames a tool turns the run red the night it ships, and a table
edited from memory turns `cargo test` red before it reaches the Lab.

Expectation format:

    {"events": ["pre_tool_use", ...], "effects": ["deny", ...],
     "aliases": {"shell": {"tools": ["Bash"], "fields": {"command": "command"},
                           "call_tool": "Bash", "call": {"command": "..."}},
                 "agent.message": null}}

A row may also carry `load`, a `{"tool", "args"}` call that makes a
deferred tool callable (Claude's `ToolSearch`), scripted before the call.

A row may carry `unsupported`, the measured reason a tool the harness has
cannot carry the alias's portable fields; the alias then stays unbound.

A row may carry `optional`, the measured reason a harness offers the tool
only in some sessions; a session that does not offer it records a
declaration rather than a failure. Whether it was offered is asked of the
session that scripted the call, never of the run's union: an agent whose
own definition names the tool is offered it, which says nothing of the
session a person starts.

`tools` lists every hook-side name the alias answers to, primary first;
`fields` maps each portable field to the hook-side input field; `call_tool`
is the model-side tool a scene scripts for it (the primary name when
omitted) and `call` its input. `null` says the harness has no tool of that
kind. `events` and `effects` are what the hooks contract exercises and must
equal what the integration claims.
"""

from __future__ import annotations

import json
import os
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

#: The run's record of the hook side, beside `declared-tools.json`.
HOOKED_TOOLS = "hooked-tools.json"


def expectation_path(harness: str) -> str:
    return os.path.join(ROOT, "harnesses", harness, "vocabulary.json")


def snapshot_path(harness: str) -> str:
    return os.path.join(ROOT, "evidence", "tools", f"{harness}.json")


def load_expectation(harness: str) -> dict:
    with open(expectation_path(harness)) as f:
        return json.load(f)


def load_snapshot(harness: str) -> dict | None:
    try:
        with open(snapshot_path(harness)) as f:
            return json.load(f)
    except FileNotFoundError:
        return None


def call_tool(row: dict) -> str:
    return row.get("call_tool") or row["tools"][0]


def census(records: list[dict]) -> dict[str, list[str]]:
    """{native name: input fields} from the census group's records."""
    hooked: dict[str, set[str]] = {}
    for record in records:
        native = record.get("HOOK_TOOL_NATIVE")
        if not native:
            continue
        try:
            fields = json.loads(record.get("HOOK_INPUT") or "{}")
        except ValueError:
            fields = {}
        hooked.setdefault(native, set()).update(
            fields if isinstance(fields, dict) else {}
        )
    return {name: sorted(fields) for name, fields in hooked.items()}


def record_hooked(outdir: str, hooked: dict[str, list[str]]) -> None:
    path = os.path.join(outdir, HOOKED_TOOLS)
    known = read_hooked(outdir)
    for name, fields in hooked.items():
        known[name] = sorted(set(known.get(name, [])) | set(fields))
    with open(path, "w") as f:
        json.dump(known, f, indent=1, sort_keys=True)


def read_hooked(outdir: str) -> dict[str, list[str]]:
    try:
        with open(os.path.join(outdir, HOOKED_TOOLS)) as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def evaluate_calls(harness: str, refused: list[dict], check) -> None:
    """Every call a scenario scripted named a tool the harness declared.

    A refused call is the Lab not speaking the real wire: the scene asked
    for a tool this harness does not have, and whatever it then asserted
    was measured on a turn that never made the call. A tool the expectation
    marks `optional` is the exception, and is judged where it is declared.
    """
    try:
        aliases = load_expectation(harness)["aliases"].values()
    except FileNotFoundError:
        aliases = []
    optional = {call_tool(row) for row in aliases if row and row.get("optional")}
    refused = [call for call in refused if call["tool"] not in optional]
    tools = sorted({call["tool"] for call in refused})
    check(
        "vocabulary-scripted-calls-declared",
        not refused,
        "every scripted call named a declared tool"
        if not refused
        else f"scripted undeclared tools {tools}; "
        f"the harness declared {refused[-1]['declared']}",
    )


def evaluate(
    harness: str,
    declared: dict[str, list[str]],
    hooked: dict[str, list[str]],
    check,
    declare,
    refused: frozenset[str] = frozenset(),
) -> None:
    """Records the vocabulary checks of one run through `check`.

    An empty capture is a Lab defect, not a vendor fact: no harness talks to
    a model without declaring a tool, so a run that recorded none failed to
    parse its dialect, and every check after it would be measuring nothing.
    The hook side is measured only by a run that played the hooks contract's
    rows scene; a run that did not has nothing to say about it. `refused`
    names the tools a scripted call asked for in a request that did not
    declare them.
    """
    check(
        "vocabulary-captured",
        bool(declared),
        f"{len(declared)} declared tools"
        if declared
        else "the providers recorded no tool declaration",
    )
    if not declared:
        return
    try:
        expectation = load_expectation(harness)
    except FileNotFoundError:
        check(
            "vocabulary-expectation-present",
            False,
            f"no expectation at {expectation_path(harness)}",
        )
        return
    rows = {alias: row for alias, row in expectation["aliases"].items() if row}
    for alias, row in sorted(rows.items()):
        tool = call_tool(row)
        # A deferred tool is declared only once loaded; what a session must
        # offer from its first request is the tool that loads it.
        if row.get("load") and tool not in declared:
            tool = row["load"]["tool"]
        detail = (
            f"`{tool}` is declared to the model"
            if tool in declared
            else f"`{tool}` is not declared; declared: {', '.join(sorted(declared))}"
        )
        if row.get("optional"):
            # A tool the harness offers only in some sessions: whether the
            # session that scripted it was offered it is measured, and not
            # offering it is the declared case. A run that played no rows
            # scene scripted it nowhere, and has nothing to say.
            if not hooked:
                continue
            offered = tool in declared and tool not in refused
            declare(
                f"vocabulary-{alias}-scriptable",
                not offered,
                row["optional"],
                detail
                if offered or tool not in declared
                else f"`{tool}` is declared in this run, but not to the session that scripted it",
            )
            if not offered:
                continue
        else:
            check(f"vocabulary-{alias}-scriptable", tool in declared, detail)
        if not hooked:
            continue
        native = row["tools"][0]
        if row.get("unsupported"):
            # Measured to reach hooks without the alias's fields: what is
            # checked is that the tool still reaches them at all.
            check(
                f"vocabulary-{alias}-hooked",
                native in hooked,
                f"a `{tool}` call reaches hooks as `{native}`"
                if native in hooked
                else f"no hook saw `{native}`; hooks saw {sorted(hooked)}",
            )
            continue
        bound = sorted(row.get("fields", {}).values())
        missing = [field for field in bound if field not in hooked.get(native, [])]
        reached = native in hooked and not missing
        check(
            f"vocabulary-{alias}-hooked",
            reached,
            f"a `{tool}` call reaches hooks as `{native}` with {bound}"
            if reached
            else f"expected `{native}` with {bound}; hooks saw {hooked}",
        )
    _snapshot_current(harness, rows, declared, hooked, check)


def _snapshot_current(harness, rows, declared, hooked, check):
    """The committed snapshot still says what this run measured about every
    tool the vocabulary binds — the facts `cargo test` reads from it. Only
    bound tools and bound fields are compared: the rest of a declaration
    varies from one session to the next (Claude's `Agent` schema does) and
    nothing depends on it. So does whether an `optional` tool is declared at
    all, which depends on what the run's agents asked for."""
    snapshot = load_snapshot(harness)
    if snapshot is None:
        check(
            "vocabulary-snapshot-current",
            False,
            f"no snapshot at {snapshot_path(harness)}",
        )
        return
    moved = []
    for alias, row in rows.items():
        tool = call_tool(row)
        if row.get("load"):
            tool = row["load"]["tool"]
        if not row.get("optional") and (tool in declared) != (
            tool in snapshot.get("tools", {})
        ):
            moved.append(f"{alias}: `{tool}` declared now={tool in declared}")
        native = row["tools"][0]
        if not hooked:
            continue
        recorded = snapshot.get("hook_tools", {}).get(native, [])
        for field in row.get("fields", {}).values():
            if (field in hooked.get(native, [])) != (field in recorded):
                moved.append(f"{alias}: `{native}.{field}`")
    check(
        "vocabulary-snapshot-current",
        not moved,
        "the snapshot agrees with this run on every bound tool"
        if not moved
        else f"re-record the snapshot ({snapshot['harness_version']}): "
        f"{'; '.join(moved)}",
    )


def record_snapshot(
    harness: str,
    harness_version: str,
    declared: dict[str, list[str]],
    hooked: dict[str, list[str]],
) -> str:
    """Writes `evidence/tools/<h>.json` from a run's capture. A maintainer
    commits it; the diff is the review of what the vendor changed."""
    if not declared:
        raise RuntimeError("refusing to record an empty vocabulary snapshot")
    path = snapshot_path(harness)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    previous = load_snapshot(harness) or {}
    # A run that did not play the rows scene keeps the hook side it did not
    # measure, rather than erasing it.
    hook_tools = hooked or previous.get("hook_tools", {})
    with open(path, "w") as f:
        json.dump(
            {
                "harness": harness,
                "harness_version": harness_version,
                "recorded_at": time.strftime("%Y-%m-%d"),
                "tools": dict(sorted(declared.items())),
                "hook_tools": dict(sorted(hook_tools.items())),
            },
            f,
            indent=1,
        )
        f.write("\n")
    return path
