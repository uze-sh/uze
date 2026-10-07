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
                   `tools: Read, Grep`, and a
                   `harness:` block with a model
                   per harness                        → flow:scout

The vendor-fields agent's dispatch must also run on the model its
`harness.<id>` block gives this harness (`shared.markers.BLOCK_MODELS`),
read off the request the harness sent for it.

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
import re
import subprocess

from contract import declared
from shared.common import (
    check,
    describe,
    observed_markers,
    provider_struct,
    start_provider,
)
from shared.markers import AGENT_PROBE, AGENTS, BLOCK_MODELS

#: Where every headless turn of this contract runs.
PROJECT = "/work/agents-project"

PRELUDE = f"""
mkdir -p {PROJECT} && cd {PROJECT}
git init -q -b main .
"""


def assert_contract(cfg, prov_ip, bindings):
    with describe("agent"):
        exposed = _assert_exposure(cfg, bindings)
        _assert_dispatch(cfg, bindings, exposed)
    start_provider(cfg, "static")


def _assert_block_model(cfg, bindings, label, body):
    """The agent ran on the model its `harness.<id>` block gave this harness.

    Read off the request that carried its body: the model is what the
    harness asked the provider for, never what UZE says it delivered.
    """
    expected = BLOCK_MODELS[bindings.harness]
    models = sorted(
        {
            request_model(request)
            for request in provider_struct(cfg)
            if request.get("summary", {}).get("agent_markers", {}).get(body)
        }
        - {None}
    )
    declared.presence(
        bindings,
        "agent-vendor-fields-block-model",
        "agent-vendor-fields-block-model",
        any(expected in model for model in models),
        f"`{label}` ran on {models} (its `harness` block asks for `{expected}`)",
        measured=bool(models),
    )


def request_model(request):
    """The model a recorded request asked for: named in its body, or in its
    path where the API puts it there (`/models/<id>:generateContent`)."""
    model = request.get("summary", {}).get("model")
    if model:
        return model
    found = re.search(r"/models/([^:/?]+)", request.get("path", ""))
    return found.group(1) if found else None


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
        exposed[shape] = seen.get(label, False)
        declared.presence(
            bindings,
            f"agent-{shape}-exposed",
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
        if not exposed.get(shape):
            # Dispatching a label the harness never offered cannot tell a
            # broken dispatch from a missing agent, and it is the missing
            # agent the exposure check already reported. A harness that
            # declares it cannot dispatch is measured by that same absence.
            declared.presence(
                bindings,
                name,
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
        declared.presence(
            bindings,
            name,
            name,
            arrived,
            f"dispatching `{label}` put its body in a model request"
            if arrived
            else f"dispatched `{label}`, but its body never reached the model: "
            f"{output[-200:]}".replace("\n", " "),
        )
        if shape == "vendor-fields" and arrived:
            _assert_block_model(cfg, bindings, label, body)
