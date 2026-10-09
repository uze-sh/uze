"""What a project's instructions must do, on every harness.

`AGENTS.md` is the one file a project writes its instructions in, and UZE's
promise is that every harness reads it — natively where the vendor does,
through whatever bridge UZE projects where it does not. Which of the two a
harness needs is the product's business and changes with vendor releases;
what must not change is the outcome: the instructions are in front of the
model.

The isolation contract already sees a projected declaration reach the model,
but in a scene that writes its own `CLAUDE.md` and `GEMINI.md` bridges —
right for what it measures, and exactly what this one must not do. Here the
project is what a person has: an `AGENTS.md`, an `agents.yaml`, and
`uze install` run once to make it ready. Nothing else is laid down, so a
harness that reads no `AGENTS.md` and gets no bridge from UZE fails here.

The project's `.agents/` is authored the same way and read the same way: a
Skill under `.agents/skills/` and an agent under `.agents/agents/` must
reach the model on every harness, launched the way a person launches it,
through UZE's launcher. Some harnesses read a kind themselves; one that
does not is given it through its runtime projection, outside the
repository; a harness that can be given it no other way declares so, with
the measured reason. Either way the checkout is the project's, so the
scene also proves nothing appeared in it.
"""

import json
import os
import subprocess

from contract import declared
from shared.common import (
    check,
    concurrently,
    describe,
    observed_markers,
    provider_struct,
    start_provider,
)
from shared.markers import (
    CONTEXT,
    CONTEXT_PROBE,
    CONTEXT_TAIL,
    PROJECT_AGENT,
    PROJECT_SKILL,
)

PROJECT = "/work/context-project"
AUTHORED_PROJECT = "/work/authored-project"
PROJECT_SKILL_NAME = "house-notes"
PROJECT_AGENT_NAME = "house-reviewer"
UZE_HOME = "/work/home/.uze"

#: Printed around the checkout's status once the turn has ended, so the
#: listing can be read back out of the container's output.
CHECKOUT_BEGIN = "UZE_CHECKOUT_STATUS_BEGIN"
CHECKOUT_END = "UZE_CHECKOUT_STATUS_END"

PRELUDE = f"""
mkdir -p {PROJECT} && cd {PROJECT}
git init -q -b main .
git config user.name lab
git config user.email lab@uze.invalid
cat > AGENTS.md <<'UZE_EOF'
# Lab project

Every answer in this project follows the house rule {CONTEXT}.
UZE_EOF
printf '{{}}\\n' > agents.yaml
git add . && git commit -q -m init
uze install >/work/context-install.log 2>&1 || true
"""


def assert_contract(cfg, prov_ip, bindings):
    with describe("context"):
        # Three projects that share nothing, each turn in a world of its
        # own; the checks read what each one observed, in this order.
        turns = concurrently(
            cfg,
            {
                "context-turn": lambda world: _agents_md_turn(world, bindings),
                "context-project": lambda world: authored_turn(
                    world, bindings, "context-project-skill"
                ),
                "context-long": lambda world: _long_agents_md_turn(world, bindings),
            },
        )
        _assert_agents_md(bindings, *turns["context-turn"])
        _assert_project_directory(bindings, *turns["context-project"])
        _assert_long_agents_md(bindings, *turns["context-long"])
    start_provider(cfg, "static")


LONG_PROJECT = "/work/long-project"

#: Printed around `uze status`'s answer once the turn has ended.
STATUS_BEGIN = "UZE_STATUS_BEGIN"
STATUS_END = "UZE_STATUS_END"

#: An `AGENTS.md` of about 30 KB — past the 24,000 bytes Antigravity reads
#: of a rules file — whose first line carries the house rule and whose last
#: carries the tail marker.
LONG_PRELUDE = f"""
mkdir -p {LONG_PROJECT} && cd {LONG_PROJECT}
git init -q -b main .
{{
  printf '# Lab project\\n\\nEvery answer follows the house rule {CONTEXT}.\\n\\n'
  i=0; while [ $i -lt 600 ]; do printf 'Filler line %04d that only makes this file long enough.\\n' $i; i=$((i+1)); done
  printf '\\nThe last rule of this project is {CONTEXT_TAIL}.\\n'
}} > AGENTS.md
printf '{{}}\\n' > agents.yaml
uze install >/work/long-install.log 2>&1 || true
trap 'echo {STATUS_BEGIN}; uze status --format json 2>/dev/null; echo {STATUS_END}' EXIT
"""


def _long_agents_md_turn(cfg, bindings):
    """One headless turn in a project whose `AGENTS.md` is long; returns
    the context markers its requests carried and the container's output."""
    prov_ip = start_provider(cfg, "static")
    prompt = f"{CONTEXT_PROBE} what is the last rule here?"
    cmd = bindings.headless(cfg, prov_ip, LONG_PRELUDE, prompt, LONG_PROJECT)
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=480
    )
    output = proc.stdout + proc.stderr
    with open(os.path.join(cfg.outdir, "context-long.out"), "w") as f:
        f.write(output)
    return observed_markers(provider_struct(cfg), "context_markers"), output


def _assert_long_agents_md(bindings, seen, output):
    """What UZE says about a long `AGENTS.md` agrees with what the harness
    did with it: a harness that put the last line in front of the model is
    reported as reading it, and one that did not is reported as not.

    UZE's report is the subject; the harness's request is the measure."""
    head = seen.get(CONTEXT, False)
    tail = seen.get(CONTEXT_TAIL, False)
    start = output.rfind(STATUS_BEGIN)
    end = output.rfind(STATUS_END)
    try:
        status = (
            json.loads(output[start + len(STATUS_BEGIN) : end])
            if 0 <= start < end
            else None
        )
    except ValueError:
        status = None
    name = bindings.display_name
    warned = status is not None and any(
        issue.startswith(f"{name}:") and "AGENTS.md" in issue
        for issue in status.get("issues", [])
    )
    check(
        "context-long-head-reaches-model",
        head,
        "the first line of a long AGENTS.md reached the model",
    )
    check(
        "context-long-report-agrees",
        head and status is not None and warned == (not tail),
        f"the harness {'read' if tail else 'did not read'} the last line, "
        f"and UZE {'warned' if warned else 'did not warn'}",
    )


def _agents_md_turn(cfg, bindings):
    """One headless turn in a project with an `AGENTS.md`; returns the
    context markers its requests carried and the container's output."""
    prov_ip = start_provider(cfg, "static")
    prompt = f"{CONTEXT_PROBE} what is the house rule here?"
    cmd = bindings.headless(cfg, prov_ip, PRELUDE, prompt, PROJECT)
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=480
    )
    output = proc.stdout + proc.stderr
    with open(os.path.join(cfg.outdir, "context-turn.out"), "w") as f:
        f.write(output)
    return observed_markers(provider_struct(cfg), "context_markers"), output


def _assert_agents_md(bindings, seen, output):
    reached = seen.get(CONTEXT_PROBE, False)
    check(
        "context-turn-reached-model",
        reached,
        f"{bindings.harness} sent the probe turn to the model"
        if reached
        else f"the probe turn never reached the model: {output[-200:]}".replace(
            "\n", " "
        ),
    )
    if not reached:
        return
    check(
        "context-agents-md-reaches-model",
        seen.get(CONTEXT, False),
        "the project's AGENTS.md is in the model request",
    )


def _authored_prelude(launcher):
    """A project that authored a Skill in `.agents/skills/` and an agent in
    `.agents/agents/`, made ready with `uze install` and committed, so
    anything that appears in the checkout afterwards was written by the
    launch. The harness is launched through UZE's launcher on `PATH`, which
    is where a project's runtime projection is made. The checkout's status
    is printed on exit, whatever the turn did."""
    return f"""
mkdir -p {AUTHORED_PROJECT}/.agents/skills/{PROJECT_SKILL_NAME} {AUTHORED_PROJECT}/.agents/agents
cd {AUTHORED_PROJECT}
git init -q -b main .
git config user.name lab
git config user.email lab@uze.invalid
printf '# Lab project\\n' > AGENTS.md
cat > .agents/skills/{PROJECT_SKILL_NAME}/SKILL.md <<'UZE_EOF'
---
name: {PROJECT_SKILL_NAME}
description: The house notes of this project, {PROJECT_SKILL}.
---

Answer with the house notes.
UZE_EOF
cat > .agents/agents/{PROJECT_AGENT_NAME}.md <<'UZE_EOF'
---
name: {PROJECT_AGENT_NAME}
description: Reviews against the house rules of this project, {PROJECT_AGENT}.
---

Review the change against the house rules.
UZE_EOF
printf '{{}}\\n' > agents.yaml
uze install >/work/authored-install.log 2>&1 || true
git add -A && git commit -q -m init
mkdir -p {UZE_HOME}/shims
ln -sf "$(command -v uze)" {UZE_HOME}/shims/{launcher}
export PATH={UZE_HOME}/shims:$PATH
trap 'echo {CHECKOUT_BEGIN}; git -C {AUTHORED_PROJECT} status --porcelain --ignored --untracked-files=all; echo {CHECKOUT_END}' EXIT
"""


def checkout_status(output):
    """The status lines printed between the markers, or `None` when the
    listing never ran."""
    start = output.rfind(CHECKOUT_BEGIN)
    end = output.rfind(CHECKOUT_END)
    if start < 0 or end < start:
        return None
    body = output[start + len(CHECKOUT_BEGIN) : end]
    return [line for line in body.splitlines() if line.strip()]


def authored_turn(cfg, bindings, evidence):
    """One headless turn in the project that authored its `.agents/`,
    launched through UZE's launcher, in a project the person has opened
    once (`project_turn`). Returns the context markers the model
    requests carried and the container's output; the output is kept as
    `<evidence>.out`. The turn is a delegating one, so a harness that offers
    agents only to a dispatcher offers the project's here."""
    prov_ip = start_provider(cfg, "static")
    prompt = f"{CONTEXT_PROBE} which notes does this project keep?"
    prelude = _authored_prelude(bindings.launcher_name())
    output = bindings.project_turn(
        cfg, prov_ip, prelude, prompt, AUTHORED_PROJECT, delegating=True
    )
    with open(os.path.join(cfg.outdir, f"{evidence}.out"), "w") as f:
        f.write(output)
    return observed_markers(provider_struct(cfg), "context_markers"), output


def _assert_project_directory(bindings, seen, output):
    reached = seen.get(CONTEXT_PROBE, False)
    check(
        "context-project-skill-turn-reached-model",
        reached,
        f"{bindings.harness} sent the probe turn to the model"
        if reached
        else f"the probe turn never reached the model: {output[-200:]}".replace(
            "\n", " "
        ),
    )
    if not reached:
        return
    check(
        "context-project-skill-reaches-model",
        seen.get(PROJECT_SKILL, False),
        "the Skill the project authored in .agents/skills is in the model request",
    )
    written = checkout_status(output)
    check(
        "context-project-skill-checkout-untouched",
        written == [],
        "nothing appeared in the checkout"
        if written == []
        else f"the checkout changed: {written}"
        if written
        else "the checkout's status was never printed",
    )
    _assert_project_agent(bindings, seen)


def _assert_project_agent(bindings, seen):
    """The agent the project authored is offered to the model by every
    harness that is claimed to receive it."""
    name = "context-project-agent-reaches-model"
    declared.presence(
        bindings,
        name,
        name,
        seen.get(PROJECT_AGENT, False),
        "the agent the project authored in .agents/agents is offered to the model",
    )
