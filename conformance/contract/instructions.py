"""What an agent the workspace launched is told, on every harness.

The workspace's instructions (where the agent works, how its work is
named and delivered) reach an agent through the context UZE composes for
its launch, never through the project's `AGENTS.md`, which holds only what
the operator wrote. Each harness carries that context through the channel
its integration declares; what only the real binary can answer is whether
the instructions are in front of the model where the agent works:

    the instructions reach the model, in the agent's own checkout, and
    reach a subagent it spawns.

Two scenes, each the continuity scene's world (a repository, a slot, the
record naming the agent, UZE's own launcher):

- An isolated agent in its slot, whose `AGENTS.md` its own branch changed:
  the primary checkout's carries one marker, the slot's another. One turn
  scripts the harness's own subagent tool. The parent's requests carry
  the instructions, state the slot as the working directory and carry the
  slot's `AGENTS.md`, never the primary's, and hold the instructions to
  their last sentence; the subagent's own request (its
  task in a message of its own, not only in the call that spawned it)
  carries the instructions too.
- An agent in the operator's checkout: its request carries the
  instructions and states the project's root as the working directory.

Both read the provider's structural summary, never the pane and never
UZE's report. What a model does with the instructions is not measured.
"""

import copy
import json

from contract import declared
from shared import vocabulary
from shared.common import (
    check,
    check_absence,
    concurrently,
    describe,
    provider_struct,
    start_provider,
)
from shared.markers import INSTRUCTIONS

from . import continuity

#: What the static provider answers the operator's turn with.
STATIC_ANSWER = "UZE_CONFORMANCE_OK"
SPAWN_PROMPT = f"{INSTRUCTIONS['spawn']} delegate this to a subagent"


def _records():
    return f"{continuity.UZE_HOME}/state/projects/{continuity.project_id(continuity.PROJECT)}"


def slot_prelude(bindings):
    """The continuity scene, with the primary's and the slot's `AGENTS.md`
    told apart, as two branches would carry them."""
    base = continuity.prelude(bindings.launcher_name())
    written = "printf '# Lab project\\n' > AGENTS.md"
    assert written in base, "the continuity prelude no longer writes AGENTS.md"
    primary = (
        "printf '# Lab project\\n\\nEvery agent here follows "
        f"{INSTRUCTIONS['primary_agents_md']}.\\n' > AGENTS.md"
    )
    return (
        base.replace(written, primary)
        + f"""
cd {continuity.SLOT}
printf '# Lab project\\n\\nEvery agent on this branch follows {INSTRUCTIONS["slot_agents_md"]}.\\n' > AGENTS.md
git commit -qam 'the slot changes its instructions'
cd {continuity.PROJECT}
"""
    )


def primary_prelude(bindings):
    """The continuity scene's repository with the agent recorded in the
    operator's own checkout: no checkout of its own."""
    record = {
        "schema_version": 4,
        "agents": [
            {
                "id": continuity.SLOT_NAME,
                "harness": bindings.launcher_name(),
                "label": "in place",
                "created_at_unix": 1,
                "ended_at_unix": None,
                "state": {"state": "running"},
                "launch_key": continuity.LAUNCH_KEY_DIGEST,
                "isolation": None,
            }
        ],
    }
    return (
        continuity.prelude(bindings.launcher_name())
        + f"""
cat > {_records()}/agents.json <<'UZE_EOF'
{json.dumps(record, indent=1)}
UZE_EOF
"""
    )


def _spawn_call(bindings):
    """The harness's own subagent tool, called with a task carrying the
    subagent's marker, from the Lab's template for it."""
    row = vocabulary.load_expectation(bindings.harness)["aliases"]["agent.spawn"]

    def swap(value):
        if isinstance(value, str):
            return value.replace("lab subagent", INSTRUCTIONS["subagent_turn"])
        if isinstance(value, dict):
            return {k: swap(v) for k, v in value.items()}
        if isinstance(value, list):
            return [swap(v) for v in value]
        return value

    rendered = bindings.call({"call": swap(row["call"]), "values": {}})
    return {"tool": vocabulary.call_tool(row), "args": rendered["call"]}


def assert_contract(cfg, prov_ip, bindings):
    with describe("instructions"):
        found = concurrently(
            cfg,
            {
                "instructions-slot": lambda world: _slot(world, bindings),
                "instructions-primary": lambda world: _primary(world, bindings),
            },
        )
        _assert_slot(bindings, found["instructions-slot"])
        _assert_primary(bindings, found["instructions-primary"])
    start_provider(cfg, "static")


def _slot(world, bindings):
    mode, env = bindings.sequence([_spawn_call(bindings)], SPAWN_PROMPT)
    prov_ip = start_provider(world, mode, env)
    out = {"ready": False, "turn": None}
    with bindings.relaunch_in(
        world, prov_ip, continuity.SLOT, slot_prelude(bindings)
    ) as tui:
        plain, ready = bindings.prepare(tui)
        out["ready"] = bool(ready)
        out["plain"] = plain[-160:].replace("\n", " ")
        if ready:
            out["turn"] = bindings.hook_turn(tui, SPAWN_PROMPT)
            tui.snapshot("instructions-slot-turn", out["turn"].plain)
            tui.snapshot("instructions-slot-after", tui.collect(reads=4))
    out["requests"] = [r.get("summary", {}) for r in provider_struct(world)]
    _record(world, "instructions-slot", out["requests"])
    return out


def _primary(world, bindings):
    driver = copy.copy(bindings)
    driver.final_markers = (STATIC_ANSWER,)
    prov_ip = start_provider(world, "static")
    out = {"ready": False, "turn": None}
    with driver.relaunch_in(
        world, prov_ip, continuity.PROJECT, primary_prelude(bindings)
    ) as tui:
        plain, ready = driver.prepare(tui)
        out["ready"] = bool(ready)
        out["plain"] = plain[-160:].replace("\n", " ")
        if ready:
            out["turn"] = driver.hook_turn(tui, f"{INSTRUCTIONS['turn']} carry on")
            tui.snapshot("instructions-primary-turn", out["turn"].plain)
    out["requests"] = [r.get("summary", {}) for r in provider_struct(world)]
    _record(world, "instructions-primary", out["requests"])
    return out


def _record(world, name, requests):
    """What each request carried, for the run's evidence."""
    with open(f"{world.outdir}/{name}-requests.json", "w") as f:
        json.dump(
            [
                {
                    "markers": sorted(
                        k for k, v in r.get("instruction_markers", {}).items() if v
                    ),
                    "turns": r.get("instruction_turns", {}),
                    "working_directories": r.get("working_directories", []),
                }
                for r in requests
            ],
            f,
            indent=1,
        )


def _carries(request, key):
    return bool(request.get("instruction_markers", {}).get(INSTRUCTIONS[key]))


def _own_turn(request, key):
    return bool(request.get("instruction_turns", {}).get(INSTRUCTIONS[key]))


def _directories(requests):
    return sorted({p for r in requests for p in r.get("working_directories", [])})


def _assert_ready(name, out):
    check(
        f"{name}-ready",
        out["ready"],
        "the harness opened waiting for the operator" if out["ready"] else out["plain"],
    )
    return out["ready"]


def _assert_slot(bindings, out):
    if not _assert_ready("instructions-slot", out):
        return
    requests = out["requests"]
    parent = [
        r
        for r in requests
        if _own_turn(r, "spawn") and not _own_turn(r, "subagent_turn")
    ]
    subagent = [r for r in requests if _own_turn(r, "subagent_turn")]
    told = any(_carries(r, "lead") for r in parent)
    check(
        "instructions-reach-the-model",
        told,
        f"{len(parent)} requests of the agent's own turn; the instructions are in "
        f"{sum(_carries(r, 'lead') for r in parent)}",
    )
    whole = any(_carries(r, "lead") and _carries(r, "tail") for r in parent)
    check(
        "instructions-arrive-whole",
        whole,
        "a request of the agent's own carries the instructions to their last sentence"
        if whole
        else "the instructions arrived without their last sentence",
    )
    stated = _directories(parent)
    check(
        "instructions-working-directory-is-the-checkout",
        bool(parent) and stated == [continuity.SLOT],
        f"the agent's requests state {stated} as the working directory; its checkout "
        f"is {continuity.SLOT}",
    )
    own = any(_carries(r, "slot_agents_md") for r in parent)
    check(
        "instructions-slot-reads-its-own-agents-md",
        own,
        "the slot's AGENTS.md is in the agent's requests"
        if own
        else "no request of the agent carries its own branch's AGENTS.md",
    )
    leaked = [i for i, r in enumerate(requests) if _carries(r, "primary_agents_md")]
    check_absence(
        "instructions-slot-never-given-the-primary-agents-md",
        not leaked,
        bool(out["turn"] and out["turn"].settled),
        proof=own,
        detail="no request of the slot's agent or its subagent carries the primary "
        "checkout's AGENTS.md"
        if not leaked
        else f"requests {leaked} carry the primary checkout's AGENTS.md",
    )
    reached = bool(subagent) and all(_carries(r, "lead") for r in subagent)
    declared.presence(
        bindings,
        "instructions-reach-a-subagent",
        "instructions-reach-a-subagent",
        reached,
        f"{out['turn'].detail}; {len(subagent)} requests of the subagent's own, "
        f"{sum(_carries(r, 'lead') for r in subagent)} carrying the instructions, "
        f"{sum(_carries(r, 'slot_agents_md') for r in subagent)} the slot's AGENTS.md",
        measured=bool(subagent),
    )


def _assert_primary(bindings, out):
    if not _assert_ready("instructions-primary", out):
        return
    turn = [r for r in out["requests"] if _carries(r, "turn")]
    check(
        "instructions-in-place-reach-the-model",
        any(_carries(r, "lead") for r in turn),
        f"{len(turn)} requests of the operator's turn; the instructions are in "
        f"{sum(_carries(r, 'lead') for r in turn)}",
    )
    stated = _directories(turn)
    check(
        "instructions-in-place-working-directory-is-the-project",
        bool(turn) and stated == [continuity.PROJECT],
        f"the agent's requests state {stated} as the working directory; it works in "
        f"{continuity.PROJECT}",
    )
