"""What an agent definition must do, on every harness.

A plugin's `agents/` directory is delivered to four harnesses that disagree
about almost everything an agent file is: where it lives, whether its name
comes from the file or the frontmatter, which characters a name may carry,
which fields make the harness drop it without a word. None of that may leak
into the outcome. UZE commits to one label per agent everywhere —
`<plugin>:<subdirs...>:<name>`, the name being the frontmatter `name` and
the file stem only when there is none, the label Claude Code gives a
plugin's agent natively — and to projecting only the portable subset of an
agent's fields, so an agent written for one vendor still reaches the rest.

The `flow` fixture carries one agent of each shape (`shared.markers.AGENTS`):

    flat           agents/auditor.md                  → flow:auditor
    nested         agents/checks/security.md          → flow:checks:security
    renamed        agents/style.md, `name: linter`    → flow:linter
    vendor-fields  agents/scout.md, `model: haiku`,
                   `tools: Read, Grep`                → flow:scout

Both halves are read off the model's own requests, never off a screen and
never off UZE's report. *Exposed* means the label is in a request the
harness sent while offering its agents to the model — the roster in its
dispatch tool. *Dispatched* means the harness ran the agent: the provider
scripts the harness's own dispatch call naming the label, and the agent's
body — which only an agent that ran puts in a request — arrives. Each
fixture body ends with its own marker, so neither can be produced by a
listing, a description, or the provider's canned text.

Every check here is gated on a turn that demonstrably reached the model, so
a harness that never started reads as that, not as a missing agent.
"""

import os
import subprocess

from shared.common import (
    check,
    describe,
    observed_markers,
    provider_struct,
    start_provider,
)
from shared.markers import AGENT_PROBE, AGENTS

#: Where every headless turn of this contract runs.
PROJECT = "/work/agents-project"

PRELUDE = f"""
mkdir -p {PROJECT} && cd {PROJECT}
git init -q -b main .
"""


def _declined(bindings, prop):
    reason = bindings.unsupported(f"agent-{prop}")
    if reason:
        check(
            f"agent-{prop}",
            True,
            f"{bindings.harness} cannot: {reason}",
            kind="adapt",
        )
    return reason


def assert_contract(cfg, prov_ip, bindings):
    with describe("agent"):
        exposed = _assert_exposure(cfg, bindings)
        _assert_dispatch(cfg, bindings, exposed)
    start_provider(cfg, "static")


def _turn(cfg, prov_ip, bindings, tag, prompt):
    """Runs one headless turn and keeps what the container printed."""
    cmd = bindings.headless(
        cfg, prov_ip, PRELUDE, prompt, PROJECT, plugins="flow", delegating=True
    )
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=480
    )
    output = proc.stdout + proc.stderr
    with open(os.path.join(cfg.outdir, f"agent-{tag}.out"), "w") as f:
        f.write(output)
    return output


def _reached_model(cfg):
    return observed_markers(provider_struct(cfg), "agent_markers")


def _assert_exposure(cfg, bindings):
    """One turn in which the harness offers its agents to the model."""
    prov_ip = start_provider(cfg, "static")
    prompt = f"{AGENT_PROBE} list the agents you can delegate to"
    output = _turn(cfg, prov_ip, bindings, "exposure", prompt)
    seen = _reached_model(cfg)
    reached = seen.get(AGENT_PROBE, False)
    check(
        "agent-turn-reached-model",
        reached,
        f"{bindings.harness} sent the probe turn to the model"
        if reached
        else f"the probe turn never reached the model: {output[-200:]}".replace(
            "\n", " "
        ),
    )
    if not reached:
        return {}

    exposed = {}
    for shape, (label, _) in AGENTS.items():
        if _declined(bindings, f"{shape}-exposed"):
            continue
        exposed[shape] = seen.get(label, False)
        check(
            f"agent-{shape}-exposed",
            exposed[shape],
            f"`{label}` is offered to the model"
            if exposed[shape]
            else f"no request offered `{label}` to the model (see agent-exposure.out)",
        )
    return exposed


def _assert_dispatch(cfg, bindings, exposed):
    """One turn per agent in which the model dispatches it by its label."""
    for shape, (label, body) in AGENTS.items():
        name = f"agent-{shape}-dispatch-delivers-body"
        if _declined(bindings, f"{shape}-dispatch-delivers-body"):
            continue
        if not exposed.get(shape):
            # Dispatching a label the harness never offered cannot tell a
            # broken dispatch from a missing agent, and it is the missing
            # agent the exposure check already reported.
            check(
                name,
                False,
                f"not dispatched: `{label}` was never offered to the model",
            )
            continue
        prompt = f"{AGENT_PROBE} delegate to {label}"
        mode, env = bindings.dispatch(label, prompt)
        prov_ip = start_provider(cfg, mode, env)
        output = _turn(cfg, prov_ip, bindings, f"dispatch-{shape}", prompt)
        arrived = _reached_model(cfg).get(body, False)
        check(
            name,
            arrived,
            f"dispatching `{label}` put its body in a model request"
            if arrived
            else f"dispatched `{label}`, but its body never reached the model: "
            f"{output[-200:]}".replace("\n", " "),
        )
