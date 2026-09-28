"""Observation probe: how a harness names, accepts and dispatches an agent.

The `agent` contract commits UZE to one label per agent on every harness,
and to projecting only the portable subset of an agent's frontmatter. What
each vendor accepts for that — which characters a name may carry, whether
the name comes from the file or the frontmatter, which fields make an
agent silently disappear, whether a symlinked file is followed, which tool
the model is given to dispatch one — is a fact about the binary under
test, so it is measured here rather than remembered.

One headless turn in a fresh, provisioned container. `PROBE_SCRIPT` is a
host path to a shell fragment run after the vendor's own setup (it lays
the agent files down and runs the turn); `PROBE_MODE` is the provider mode
(`static` answers with text, `toolcall` scripts `PROBE_TOOL` with
`PROBE_ARGS`); `PROBE_ENV` is extra provider environment as JSON. Every
raw request the harness sent is saved beside the container's output, and
nothing is asserted — the facts are read off the evidence.

  PROBE_SCRIPT=... python3 conformance/lab.py --harness codex \\
      --experiment codex/agents
"""

import json
import os
import subprocess

from shared import common


def _provider_env():
    env = {"DISCOVERY": "1"}
    env.update(json.loads(os.environ.get("PROBE_ENV", "{}")))
    tool = os.environ.get("PROBE_TOOL")
    if tool:
        env["TOOL_NAME"] = tool
        env["TOOL_ARGS"] = os.environ.get("PROBE_ARGS", "{}")
    return env


def run_with(cfg, prov_ip, container):
    """`container(cfg, prov_ip, script)` builds the vendor's non-TTY run."""
    with open(os.environ["PROBE_SCRIPT"]) as f:
        script = f.read()
    mode = os.environ.get("PROBE_MODE", "static")
    prov_ip = common.start_provider(cfg, mode, _provider_env()) or prov_ip
    proc = subprocess.run(
        container(cfg, prov_ip, script),
        capture_output=True,
        text=True,
        errors="replace",
        timeout=int(os.environ.get("PROBE_TIMEOUT", "400")),
    )
    with open(os.path.join(cfg.outdir, "probe.out"), "w") as f:
        f.write(proc.stdout)
        f.write("\n=== stderr\n")
        f.write(proc.stderr)
    requests = subprocess.run(
        ["docker", "exec", cfg.prov_name, "cat", "/app/raw-requests.log"],
        capture_output=True,
        text=True,
        errors="replace",
    ).stdout
    with open(os.path.join(cfg.outdir, "requests.log"), "w") as f:
        f.write(requests)
    print(proc.stdout[-4000:], flush=True)
    print(
        f"[agent-probe] exit {proc.returncode}; {requests.count('### ')} requests; "
        f"evidence in {cfg.outdir}",
        flush=True,
    )
    common.check("agent-probe-ran", True, "observation only")
