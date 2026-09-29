"""Observation experiment: can a launch hand OpenCode a project's agents
from outside the project?

OpenCode 2.0.18 reads `./.agents/skills` but not `./.agents/agents`; its
project roots for agents are `.opencode/agent(s)`, inside the checkout,
where UZE never writes. The binary honours three environment variables
that could carry agents from elsewhere, and this records what each does:

- `OPENCODE_CONFIG_DIR` replaces the user's configuration directory, so
  the user's own provider and agents are gone for that launch;
- `OPENCODE_CONFIG_CONTENT` (like `OPENCODE_CONFIG`) is additive, but it
  is read by the process that starts the server. A default launch
  attaches to the shared background service, so the agents given by the
  launch that started it are offered in every other project it serves,
  and a launch finding the service already up has its own ignored;
- `--standalone` gives a launch a private server, where the variable
  holds per launch. That is a mode a person chooses, not one UZE may.

Run:
  python3 conformance/lab.py --harness opencode --experiment opencode/project-agents
"""

import re
import subprocess

from harnesses.opencode.scenarios import opencode_setup
from shared import common

AGENT = '{{"agents":{{"{name}":{{"description":"{mark}","mode":"subagent","prompt":"hi"}}}}}}'

SCRIPT = f"""
set +e
mkdir -p /work/extra/agents /work/a /work/b
(cd /work/a && git init -q .) && (cd /work/b && git init -q .)
printf -- '---\\ndescription: UZE_EXP_DIR_AGENT\\nmode: subagent\\n---\\nhi\\n' > /work/extra/agents/dir-agent.md
cd /work/a && OPENCODE_CONFIG_DIR=/work/extra timeout 120 opencode run "UZE_EXP_TURN_DIR hello"
opencode service stop >/dev/null 2>&1
cd /work/a && OPENCODE_CONFIG_CONTENT='{AGENT.format(name="a-agent", mark="UZE_EXP_A_AGENT")}' timeout 120 opencode run "UZE_EXP_TURN_A hello"
cd /work/b && timeout 120 opencode run "UZE_EXP_TURN_B hello"
cd /work/b && OPENCODE_CONFIG_CONTENT='{AGENT.format(name="late-agent", mark="UZE_EXP_LATE_AGENT")}' timeout 120 opencode run "UZE_EXP_TURN_LATE hello"
opencode service stop >/dev/null 2>&1
cd /work/b && OPENCODE_CONFIG_CONTENT='{AGENT.format(name="own-agent", mark="UZE_EXP_OWN_AGENT")}' timeout 120 opencode run --standalone "UZE_EXP_TURN_OWN hello"
"""


def _turn(requests, turn):
    """The requests carrying `turn`'s prompt."""
    return [r for r in requests if turn in r]


def run(cfg, prov_ip):
    prov_ip = common.start_provider(cfg, "static", {"DISCOVERY": "1"})
    cmd = common.docker_base(
        cfg, prov_ip, opencode_setup(cfg, prov_ip, SCRIPT, plugins="flow"), tty=False
    )
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=600
    )
    with open(f"{cfg.outdir}/probe.out", "w") as f:
        f.write(proc.stdout + proc.stderr)
    raw = subprocess.run(
        ["docker", "exec", cfg.prov_name, "cat", "/app/raw-requests.log"],
        capture_output=True,
        text=True,
        errors="replace",
    ).stdout
    with open(f"{cfg.outdir}/requests.log", "w") as f:
        f.write(raw)
    requests = re.split(r"\n(?=### (?:POST|GET) )", raw)

    common.check(
        "config-dir-replaces-user-config",
        not _turn(requests, "UZE_EXP_TURN_DIR"),
        "with OPENCODE_CONFIG_DIR the launch lost the user's own provider: "
        "none of its requests reached it",
    )
    common.check(
        "content-reaches-the-launch-that-starts-the-service",
        any("UZE_EXP_A_AGENT" in r for r in _turn(requests, "UZE_EXP_TURN_A")),
        "OPENCODE_CONFIG_CONTENT's agent is offered in the project it was given for",
    )
    common.check(
        "content-leaks-into-another-project",
        any("UZE_EXP_A_AGENT" in r for r in _turn(requests, "UZE_EXP_TURN_B")),
        "project a's agent is offered in project b, served by the same service",
    )
    common.check(
        "content-ignored-once-the-service-is-up",
        not any(
            "UZE_EXP_LATE_AGENT" in r for r in _turn(requests, "UZE_EXP_TURN_LATE")
        ),
        "a later launch's agent never reaches the model",
    )
    common.check(
        "content-holds-per-launch-when-standalone",
        any("UZE_EXP_OWN_AGENT" in r for r in _turn(requests, "UZE_EXP_TURN_OWN")),
        "a --standalone launch gets its own agent",
    )
