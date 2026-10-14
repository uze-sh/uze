"""Whether OpenCode honours Claude's `disable-model-invocation` on a skill
it reads from `~/.claude/skills` (anomalyco/opencode f3a23e5, ≈2.0.23).
Two skills there, one carrying the key; the verbatim requests
(`--discovery`) say which ones the model is offered. No UZE in the loop.

python3 conformance/lab.py --harness opencode --experiment opencode/claude-skill-control --discovery
"""

import os
import subprocess

from experiments.relaunch_probe import bindings_for
from shared.common import check, pull_captures, start_provider

CWD = "/work/skill-control"
HIDDEN = "uzehiddenskill7q"
SHOWN = "uzeshownskill7q"


def _skill(name, extra):
    return (
        f"mkdir -p $HOME/.claude/skills/{name} && printf -- '---\\nname: {name}\\n"
        f"description: lab skill {name}\\n{extra}---\\nbody\\n' > $HOME/.claude/skills/{name}/SKILL.md\n"
    )


PRELUDE = (
    f"mkdir -p {CWD} && cd {CWD} && git init -q -b main .\n"
    + _skill(HIDDEN, "disable-model-invocation: true\\n")
    + _skill(SHOWN, "")
)


def run(cfg, prov_ip):
    bindings = bindings_for(cfg.harness)
    prov_ip = start_provider(cfg, "static")
    cmd = bindings.headless(cfg, prov_ip, PRELUDE, "hello there", CWD)
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=600
    )
    with open(os.path.join(cfg.outdir, "claude-skill-control.out"), "w") as f:
        f.write(proc.stdout + proc.stderr)
    pull_captures(cfg)
    path = os.path.join(cfg.outdir, "raw-requests.log")
    requests = open(path, errors="replace").read() if os.path.exists(path) else ""
    check(
        "skill-control-shown-offered",
        SHOWN in requests,
        "the plain ~/.claude skill reached the model",
    )
    check(
        "skill-control-hidden-withheld",
        HIDDEN not in requests,
        "the skill with disable-model-invocation was not offered",
    )
