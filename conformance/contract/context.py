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
from shared.markers import CONTEXT, CONTEXT_PROBE

PROJECT = "/work/context-project"

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
        _assert_agents_md(cfg, bindings)
    start_provider(cfg, "static")


def _assert_agents_md(cfg, bindings):
    prov_ip = start_provider(cfg, "static")
    prompt = f"{CONTEXT_PROBE} what is the house rule here?"
    cmd = bindings.headless(cfg, prov_ip, PRELUDE, prompt, PROJECT)
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=480
    )
    output = proc.stdout + proc.stderr
    with open(os.path.join(cfg.outdir, "context-turn.out"), "w") as f:
        f.write(output)

    seen = observed_markers(provider_struct(cfg), "context_markers")
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
