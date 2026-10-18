"""What an agent UZE launched remembers, on every harness.

UZE keeps facts outside every checkout and gives each launched agent an
index of them as part of its context, through the same channel the
briefing travels. Two things only the real binary can answer:

    a fact kept before the launch reaches the model, and an agent can keep
    one itself with `uze agent memory remember`, from the shell its harness
    gives it.

The scene is the continuity scene's, with two project facts seeded before
the launch: a `feedback` fact, which the index gives a line with its
description, and a `user` fact, which it names alone. The request carrying
the first's marker, the second's name and the instruction saying how to look
a fact up is the index having arrived. The provider then scripts the model's shell call to `remember`, and
the request after it carrying the command's own answer (the file it
names, `<name>.md`) is the fact kept — or, under a sandbox that refuses the write,
the refusal the command printed, which is then the harness's measured
limitation rather than a silent absence.
"""

from shared import vocabulary
from shared.common import check, describe, provider_struct

from . import continuity

#: In the seeded `feedback` fact's description, so its index line carries it.
INDEXED_MARKER = "UZE_CONFORMANCE_PISTACHIO"
#: The seeded `user` fact, which the index names without describing.
NAMED_FACT = "uze-lab-named-fact"
#: What the instruction every launched agent is given says to run.
READ_MARKER = "uze agent memory list <words>"
#: The name the agent's own `remember` keeps, as the command's answer says it.
KEPT_NAME = "uze-lab-memory-kept"
#: The file the agent's own `remember` writes, read off the harness's disk:
#: a harness's shell need not hand the command's output back to the model
#: (the Lab's Codex call is a script that discards it), so the fact on disk
#: is the evidence, not what the model was told.
KEPT_FILE = f"{KEPT_NAME}.md"
#: The prompt the provider scripts the shell call for.
PROMPT = "UZE_CONFORMANCE_MEMORY: keep a fact for later agents"
#: The shell line the template's own command is replaced with.
REMEMBER = (
    f"uze agent memory remember {KEPT_NAME} --type project "
    "--description lab-kept-fact < /dev/null"
)
#: What every harness's shell template runs, which the line above stands in
#: for (`harnesses/<h>/vocabulary.json`).
TEMPLATE_COMMAND = "mkdir -p {dir} && printf ran > {side} # {mark}"


def _records():
    return f"{continuity.UZE_HOME}/state/projects/{continuity.project_id(continuity.PROJECT)}"


def prelude(bindings):
    """The continuity scene, plus one project fact kept before the launch."""
    return (
        continuity.prelude(bindings.launcher_name())
        + f"""
mkdir -p {_records()}/memory
cat > {_records()}/memory/lab-fact.md <<'UZE_EOF'
---
shape: 1
name: lab-fact
description: "kept before this launch: {INDEXED_MARKER}"
type: feedback
---

The body is read on demand.
UZE_EOF
cat > {_records()}/memory/{NAMED_FACT}.md <<'UZE_EOF'
---
shape: 1
name: {NAMED_FACT}
description: "given by name alone"
type: user
---

Found with `uze agent memory list`.
UZE_EOF
"""
    )


def _remember_call(bindings):
    """The harness's own shell call, its command replaced by `remember`."""
    shell = vocabulary.load_expectation(bindings.harness)["aliases"]["shell"]

    def swap(value):
        if isinstance(value, str):
            return value.replace(TEMPLATE_COMMAND, REMEMBER)
        if isinstance(value, dict):
            return {key: swap(child) for key, child in value.items()}
        if isinstance(value, list):
            return [swap(child) for child in value]
        return value

    template = {"call": swap(shell["call"]), "values": swap(shell.get("values", {}))}
    rendered = bindings.call(template, side="lab-side/memory")
    return {"tool": vocabulary.call_tool(shell), "args": rendered["call"]}


def assert_contract(cfg, prov_ip, bindings):
    with describe("memory"):
        _assert_memory(cfg, bindings)


def _assert_memory(cfg, bindings):
    from shared import common

    mode, env = bindings.sequence([_remember_call(bindings)], PROMPT)
    prov_ip = common.start_provider(cfg, mode, env)
    with bindings.relaunch_in(cfg, prov_ip, continuity.SLOT, prelude(bindings)) as tui:
        plain, matched = bindings.prepare(tui)
        check(
            "memory-process-ready",
            bool(matched),
            f"{bindings.harness} opened waiting for the operator"
            if matched
            else plain[-160:].replace("\n", " "),
        )
        if not matched:
            return
        turn = bindings.hook_turn(tui, PROMPT)
        tui.snapshot("memory-turn", turn.plain)
        check(
            "memory-index-reaches-the-model",
            _carried(cfg, INDEXED_MARKER),
            "a request carries the index of the facts kept before the launch",
        )
        check(
            "memory-named-fact-reaches-the-model",
            _carried(cfg, NAMED_FACT),
            "a request names the fact the index gives no line, so the model knows to ask for it",
        )
        check(
            "memory-read-instruction-reaches-the-model",
            _carried(cfg, READ_MARKER),
            "a request carries how to look a fact up with `uze agent memory list`",
        )
        kept = common.harness_files(cfg, f"{_records()}/memory").get(KEPT_FILE, "")
        check(
            "memory-kept-by-the-agent",
            "by: agent" in kept,
            "`uze agent memory remember` kept the fact from inside the harness's shell, "
            "written by the agent"
            if "by: agent" in kept
            else f"no fact `{KEPT_FILE}` under UZE's state: {kept[:160]!r}",
        )


def _carried(cfg, *markers):
    """One request holding every marker, as `continuity` reads it."""
    for record in provider_struct(cfg):
        found = record.get("summary", {}).get("continuity_markers", {})
        if all(found.get(marker) for marker in markers):
            return True
    return False
