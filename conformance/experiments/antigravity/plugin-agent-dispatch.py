"""Whether Antigravity dispatches an agent UZE delivers inside a generated
plugin (`agents/` declared by the plugin's root `agents.json`).

The agent contract dispatches `flow`'s agents, and `flow` is delivered
capability by capability (a Skill with a non-default invocation policy), so
its agents never ride in a plugin. `lifecycle-plugin` is delivered as a
generated plugin: the model dispatches its `keeper` by label, and the
agent's body reaching a model request is the proof it ran.

Run: python3 conformance/lab.py --harness antigravity --experiment antigravity/plugin-agent-dispatch --discovery
"""

import os
import subprocess

from experiments.relaunch_probe import bindings_for
from shared.common import check, pull_captures, start_provider

LABEL = "lifecycle-plugin:keeper"
BODY = "UZE_LIFECYCLE_AGENT_V1"
CWD = "/work/plugin-agent"
PRELUDE = f"mkdir -p {CWD} && cd {CWD} && git init -q -b main .\n"
PROMPT = f"delegate this to {LABEL}"


def run(cfg, prov_ip):
    bindings = bindings_for(cfg.harness)
    mode, env = bindings.dispatch(LABEL, PROMPT)
    prov_ip = start_provider(cfg, mode, env)
    cmd = bindings.headless(
        cfg, prov_ip, PRELUDE, PROMPT, CWD, plugins="lifecycle-plugin", delegating=True
    )
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=480
    )
    with open(os.path.join(cfg.outdir, "plugin-agent-dispatch.out"), "w") as f:
        f.write(proc.stdout + proc.stderr)
    pull_captures(cfg)
    path = os.path.join(cfg.outdir, "raw-requests.log")
    requests = open(path, errors="replace").read() if os.path.exists(path) else ""
    check(
        "plugin-agent-offered",
        LABEL in requests,
        "the plugin's agent was offered to the model",
    )
    check(
        "plugin-agent-dispatched",
        BODY in requests,
        "dispatching it put its body in a model request",
    )
