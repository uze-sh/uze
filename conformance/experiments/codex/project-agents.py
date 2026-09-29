"""Observation experiment: a project's `.agents/agents` reaches an
interactive Codex session launched through UZE's launcher.

Codex 0.158 reads agent roles from `~/.codex/agents` and, in a trusted
project, `.codex/agents`; never from `./.agents/agents`. UZE's launcher
hands them over as a `-c agents={...}` override, a configuration layer of
its own. The contract's context scene proves that for `codex exec`; what
only an interactive launch can show is whether the layer still reaches the
model when the session is served by a shared app-server daemon that was
already running without it, which is the state every launch after a
person's first one starts from. The user's own agents must stay offered
beside the project's.

Run:
  python3 conformance/lab.py --harness codex --experiment codex/project-agents
"""

import subprocess
import time

from harnesses.codex.bindings import CodexBindings
from shared import common

AGENT_MARK = "UZE_EXPERIMENT_PROJECT_AGENT"
PROMPT_MARK = "UZE_EXPERIMENT_PROMPT"
UZE_HOME = "/work/home/.uze"

PRELUDE = f"""
mkdir -p /work/project/.agents/agents && cd /work/project && git init -q .
cat > .agents/agents/house-reviewer.md <<'AGENT_EOF'
---
name: house-reviewer
description: Reviews against the house rules, {AGENT_MARK}.
---
Review the change against the house rules.
AGENT_EOF
codex app-server daemon start >/dev/null 2>&1
mkdir -p {UZE_HOME}/shims
ln -sf "$(command -v uze)" {UZE_HOME}/shims/codex
export PATH={UZE_HOME}/shims:$PATH
"""


def run(cfg, prov_ip):
    prov_ip = common.start_provider(cfg, "static", {"DISCOVERY": "1"})
    bindings = CodexBindings()
    with bindings.session_in(cfg, prov_ip, "/work/project", PRELUDE) as tui:
        plain, ready = bindings.prepare(tui)
        common.check("tui-ready", bool(ready), plain[-160:].replace("\n", " "))
        time.sleep(bindings.warmup)
        tui.type(f"{PROMPT_MARK} which agents can you delegate to?")
        tui.submit()
        tui.collect(reads=10)
    requests = subprocess.run(
        ["docker", "exec", cfg.prov_name, "cat", "/app/raw-requests.log"],
        capture_output=True,
        text=True,
        errors="replace",
    ).stdout
    with open(f"{cfg.outdir}/requests.log", "w") as f:
        f.write(requests)
    turn = [r for r in requests.split("\n### ") if PROMPT_MARK in r]
    common.check("turn-reached-model", bool(turn), f"{len(turn)} requests")
    common.check(
        "project-agent-offered",
        any(AGENT_MARK in r for r in turn),
        "the project's agent is in the roster of the turn's request",
    )
    common.check(
        "user-agents-kept",
        any("flow:auditor" in r for r in turn),
        "the user's own ~/.codex/agents roles are still offered",
    )
