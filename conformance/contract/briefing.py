"""What an agent continuing on another harness is told, on every harness.

When an agent moves to another harness, UZE writes it a briefing and leaves
it pending on the agent's conversation record; the launcher's next launch
of that harness gives it one first turn pointing at the file. Whether the
harness accepts that turn on its command line — alone, and behind its own
resume argument — and sends it to the model is something only the real
binary can answer:

    the first turn reaches the model, on a fresh start and on a resume.

The scene is the continuity scene's, with a briefing pending for this
harness before each launch. The turn names the briefing's path, so a marker
in the path is in the turn: the request that carries it is the harness
having taken the line. The second launch resumes the conversation the
first started, so one request holding both markers is the resumed
conversation and the new turn together.
"""

from shared.common import check, describe, provider_struct

from . import continuity

#: In the briefing's file name, so the one line pointing at it carries it.
FRESH_MARKER = "UZE_CONFORMANCE_PECAN"
RESUMED_MARKER = "UZE_CONFORMANCE_ALMOND"
#: Typed into the first process, so the conversation the second resumes
#: holds a turn whether or not the first launch's line reached the model.
TYPED_MARKER = "UZE_CONFORMANCE_CASHEW"


def _briefing(marker):
    return f"/work/briefing-{marker}.md"


def _records():
    return f"{continuity.UZE_HOME}/state/projects/{continuity.project_id(continuity.PROJECT)}"


def prelude(bindings):
    """The continuity scene, plus a briefing pending for this harness's
    first launch."""
    record = f"{_records()}/conversations/{continuity.SLOT_NAME}.json"
    return (
        continuity.prelude(bindings.launcher_name())
        + f"""
printf 'What the agent was doing.\\n' > {_briefing(FRESH_MARKER)}
mkdir -p {_records()}/conversations
cat > {record} <<'UZE_EOF'
{{
  "schema_version": 2,
  "agent": "{continuity.SLOT_NAME}",
  "harnesses": {{}},
  "briefing": {{ "integration": "{bindings.integration}", "path": "{_briefing(FRESH_MARKER)}" }}
}}
UZE_EOF
"""
    )


def between(bindings):
    """Run after the first process ends: a second briefing, pending on the
    record the first launch rewrote. Edited in place with `sed`, because
    the image carries neither Python nor jq."""
    record = f"{_records()}/conversations/{continuity.SLOT_NAME}.json"
    pending = (
        f'"briefing": {{ "integration": "{bindings.integration}", '
        f'"path": "{_briefing(RESUMED_MARKER)}" }},'
    )
    return (
        f"printf 'What happened since.\\n' > {_briefing(RESUMED_MARKER)}; "
        f'sed -i \'s|"schema_version": 2,|"schema_version": 2, {pending}|\' {record}'
    )


def assert_contract(cfg, prov_ip, bindings):
    with describe("briefing"):
        _assert_the_first_turn_reaches_the_model(cfg, prov_ip, bindings)


def _assert_the_first_turn_reaches_the_model(cfg, prov_ip, bindings):
    with bindings.relaunch_in(
        cfg, prov_ip, continuity.SLOT, prelude(bindings), between(bindings)
    ) as tui:
        plain, matched = bindings.prepare(tui)
        # A session opened on a turn never draws the empty prompt some
        # harnesses are recognised by: the turn having reached the model
        # is the process being up just as much.
        matched = matched or _carried(cfg, FRESH_MARKER)
        check(
            "briefing-first-process-ready",
            bool(matched),
            f"{bindings.harness} started with a first turn on its command line"
            if matched
            else plain[-160:].replace("\n", " "),
        )
        if not matched:
            return
        check(
            "briefing-first-turn-reaches-the-model",
            _arrives(cfg, tui, "briefing-first-turn", FRESH_MARKER),
            "a request carries the line pointing at the briefing",
        )
        bindings.await_input(tui)
        tui.type(f"{TYPED_MARKER}: remember this word")
        tui.submit()
        tui.snapshot("briefing-typed-turn", tui.collect(reads=6))

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
        check(
            "briefing-first-turn-follows-a-resume",
            _arrives(cfg, tui, "briefing-resumed-turn", TYPED_MARKER, RESUMED_MARKER),
            "one request carries the resumed conversation and the new line together",
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
