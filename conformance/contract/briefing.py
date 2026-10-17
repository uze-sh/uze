"""What an agent continuing on another harness is told, on every harness.

When an agent moves to another harness, UZE records a briefing on the
agent's conversation record, and every launch of that harness is given it
as part of the agent's context, through the channel the integration
declares: no turn the operator did not type. Whether the channel places it
before the model, keeps it out of the pane, and gives the current one again
on a resume is something only the real binary can answer:

    the briefing reaches the model, is not shown, and holds on a resume.

The scene is the continuity scene's, with a briefing recorded for this
harness before each launch. A marker in the briefing's text is the
briefing: the request that carries it beside the turn the operator typed is
the channel having delivered it. Between the launches the record's
briefing is rewritten with a second marker, so the resumed process carrying
that one is the channel giving the context composed for its own launch.
"""

from shared.common import check, describe, provider_struct

from . import continuity

#: In the briefing's text, first as recorded and then as rewritten.
FRESH_MARKER = "UZE_CONFORMANCE_PECAN"
RESUMED_MARKER = "UZE_CONFORMANCE_ALMOND"
#: Typed into each process: the turn a briefed model answers.
TYPED_MARKER = "UZE_CONFORMANCE_CASHEW"


def _records():
    return f"{continuity.UZE_HOME}/state/projects/{continuity.project_id(continuity.PROJECT)}"


def _record():
    return f"{_records()}/conversations/{continuity.SLOT_NAME}.json"


def prelude(bindings):
    """The continuity scene, plus a briefing recorded for this harness."""
    return (
        continuity.prelude(bindings.launcher_name())
        + f"""
mkdir -p {_records()}/conversations
cat > {_record()} <<'UZE_EOF'
{{
  "schema_version": 3,
  "agent": "{continuity.SLOT_NAME}",
  "harnesses": {{}},
  "briefing": {{ "integration": "{bindings.integration}", "text": "What the agent was doing: {FRESH_MARKER}." }}
}}
UZE_EOF
"""
    )


def between(bindings):
    """Run after the first process ends: the briefing on the record the
    first launch rewrote, changed. Edited in place with `sed`, because the
    image carries neither Python nor jq."""
    return f"sed -i 's|{FRESH_MARKER}|{RESUMED_MARKER}|' {_record()}"


def assert_contract(cfg, prov_ip, bindings):
    with describe("briefing"):
        _assert_the_briefing_reaches_the_model(cfg, prov_ip, bindings)


def _assert_the_briefing_reaches_the_model(cfg, prov_ip, bindings):
    with bindings.relaunch_in(
        cfg, prov_ip, continuity.SLOT, prelude(bindings), between(bindings)
    ) as tui:
        plain, matched = bindings.prepare(tui)
        check(
            "briefing-first-process-ready",
            bool(matched),
            f"{bindings.harness} opened waiting for the operator"
            if matched
            else plain[-160:].replace("\n", " "),
        )
        if not matched:
            return
        bindings.await_input(tui)
        tui.type(f"{TYPED_MARKER}: carry on")
        tui.submit()
        reached = _arrives(cfg, tui, "briefing-first-turn", TYPED_MARKER, FRESH_MARKER)
        check(
            "briefing-reaches-the-model",
            reached,
            "the request answering the operator's first turn carries the briefing"
            + (f", in a {_placed(cfg, FRESH_MARKER)} part" if reached else ""),
        )
        seen = tui.collect(reads=4)
        tui.snapshot("briefing-first-pane", seen)
        check(
            "briefing-is-not-shown",
            FRESH_MARKER not in plain + seen,
            "the briefing never appeared in the pane"
            if FRESH_MARKER not in plain + seen
            else "the pane shows the briefing's text",
        )

        plain, ended = continuity.end_the_process(tui, bindings)
        check(
            "briefing-first-process-ended",
            bool(ended),
            "the first process exited and the shell went on to the next"
            if ended
            else f"it is still running: {plain[-160:]}".replace("\n", " "),
        )
        if not ended:
            return
        plain, matched = bindings.rejoin(
            tui, plain.split(continuity.ENDED_MARKER, 1)[-1]
        )
        check(
            "briefing-second-process-ready",
            bool(matched),
            "a second process started, resuming the conversation"
            if matched
            else plain[-160:].replace("\n", " "),
        )
        if not matched:
            return
        bindings.await_input(tui)
        tui.type("and then?")
        tui.submit()
        check(
            "briefing-current-on-a-resume",
            _arrives(cfg, tui, "briefing-resumed-turn", TYPED_MARKER, RESUMED_MARKER),
            "the resumed conversation is given the briefing composed for its own launch",
        )


def _arrives(cfg, tui, snapshot, *markers, reads=12):
    """Whether a request carrying every marker reaches the model within
    `reads` reads of the screen. A harness may hold the line until it has
    finished opening — Codex answers its folder-trust dialog first — so the
    question is asked as the screen moves rather than once."""
    seen = ""
    for _ in range(reads):
        seen += tui.collect(reads=1)
        if _carried(cfg, *markers):
            tui.snapshot(snapshot, seen)
            return True
    tui.snapshot(snapshot, seen)
    return False


def _carried(cfg, *markers):
    """One request holding every marker, as `continuity` reads it."""
    for record in provider_struct(cfg):
        found = record.get("summary", {}).get("continuity_markers", {})
        if all(found.get(marker) for marker in markers):
            return True
    return False


def _placed(cfg, marker):
    """Where the provider saw `marker`, for the check's account: evidence of
    how the channel placed the context, never the verdict, because
    `AGENTS.md` itself rides in a user part on two harnesses."""
    roles = sorted(
        {
            role
            for record in provider_struct(cfg)
            for role in record.get("summary", {})
            .get("context_placement", {})
            .get(marker, [])
        }
    )
    return " / ".join(roles) or "unrecorded"
