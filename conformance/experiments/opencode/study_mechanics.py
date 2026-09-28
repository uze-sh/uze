"""Delivery-mechanics study on OpenCode (observation only).

See `experiments/claude/study_mechanics_lib` for the question and the
mechanics. OpenCode has no plugin envelope, so there is only the direct
route: `~/.agents/skills/study-<m>/` (where UZE links skills),
`~/.config/opencode/agents/study-agent-<m>.md` (mechanic c: a symlinked
sub-directory of `agents/`), and `mcp.servers.study-mcp-<m>` in
`opencode.json` naming the delivered `server.sh` by absolute path.

Every `opencode run` is `--standalone`: the background service answers its
first query empty and caches configuration.

Phases (STUDY_PHASES, comma-separated), each a fresh container and a fresh
provider: `static` (inventory, listing turn, `/study-<m>` turns, one
`--agent` turn per agent), `shell` (a scripted shell call reading every
skill's extras by relative path), `skill-<m>` (a scripted `skill` tool call).

Run: python3 conformance/lab.py --harness opencode --experiment opencode/study_mechanics
"""

import json
import os

from experiments.claude.study_mechanics_lib import (
    EVIDENCE_TAIL,
    FIXTURE,
    MECHS,
    done,
    run_container,
    summarize,
    tree,
)
from harnesses.opencode.scenarios import opencode_container
from shared import common

HOME = "/work/home"
CFG = f"{HOME}/.config/opencode"
AGENT_MECHS = ["a", "b", "c", "d", "e"]
MCP_MECHS = ["a", "b", "c", "d", "e"]
RUN = "timeout 150 opencode run --standalone --auto"

SHELL = (
    "for d in /work/home/.agents/skills/study-*; do "
    'echo "DIR $d"; (cd "$d" && cat references/ref.md; ./scripts/run.sh) 2>&1; done'
)

AGENT_OC = r"""
agent_oc() { # file tag
  mkdir -p "$(dirname "$1")"
  cat > "$1" <<EOF
---
description: Study agent for delivery-mechanic probing.
mode: all
---
STUDY_AGENT_$2 cpr=\${CLAUDE_PLUGIN_ROOT} pr=\${PLUGIN_ROOT} end
You are a study agent.
EOF
}
"""

BUILD = f"""
{FIXTURE}
{AGENT_OC}
MECHS="{" ".join(MECHS)}"
mkdir -p {HOME}/.agents/skills {CFG}/agents
for m in $MECHS; do
  T=$(UP $m)
  skill_src $STORE/skd-$m/study-$m study-$m SKD_$T
  agent_oc $STORE/agd-$m/study-agent-$m.md AGD_$T
  mcp_src $STORE/mcpd-$m
done
for c in skd-e agd-e mcpd-e; do make_ro $STORE/$c; done
for m in $MECHS; do
  deliver $m $STORE/skd-$m/study-$m {HOME}/.agents/skills/study-$m
done
for m in {" ".join(AGENT_MECHS)}; do
  if [ $m = c ]; then ln -s $STORE/agd-c {CFG}/agents/sub-c
  else deliver_file $m $STORE/agd-$m/study-agent-$m.md {CFG}/agents/study-agent-$m.md; fi
done
for m in {" ".join(MCP_MECHS)}; do
  if [ $m = c ]; then ln -s $STORE/mcpd-c /work/deliv/mcp-c
  else deliver_file $m $STORE/mcpd-$m/server.sh /work/deliv/mcp-$m/server.sh; fi
done
node -e '
const fs=require("fs"); const p="{CFG}/opencode.json";
const d=JSON.parse(fs.readFileSync(p,"utf8")); d.mcp=d.mcp||{{}}; d.mcp.servers=d.mcp.servers||{{}};
for (const m of "{" ".join(MCP_MECHS)}".split(" ")) d.mcp.servers["study-mcp-"+m]={{type:"local",command:["/work/deliv/mcp-"+m+"/server.sh"]}};
fs.writeFileSync(p, JSON.stringify(d,null,1));'
set +e
touch /work/stamp; sleep 1
WATCH="{HOME}/.agents/skills {CFG}"
cd /work
"""

STATIC = f"""
echo '=== direct-tree'
{tree(HOME + "/.agents/skills")}
{tree(CFG + "/agents")}
{tree("/work/deliv")}
echo '=== mcp-list'
pkill -x opencode; timeout 60 opencode mcp list >/dev/null 2>&1; sleep 3; timeout 60 opencode mcp list 2>&1 | tail -20
pkill -x opencode
echo '=== debug-agents'
timeout 60 opencode debug agents 2>&1 | grep -o '"[a-z:-]*study[a-z:-]*"\\|study-agent-[a-z0-9]*\\|sub-c[a-z/-]*' | sort -u
echo '=== turn-listing'
{RUN} 'study-listing' 2>&1 | tail -5
for m in $MECHS; do
  echo "=== turn-skill-slash-$m"
  {RUN} "/study-$m" 2>&1 | tail -4
done
for m in study-agent-a study-agent-b study-agent-d study-agent-e sub-c/study-agent-c study-agent-c; do
  echo "=== turn-agent-$m"
  {RUN} --agent $m 'agent probe' 2>&1 | tail -4
done
{EVIDENCE_TAIL}
"""


def tool_phase(name):
    return f"""
echo '=== turn-{name}'
{RUN} 'study-{name}' 2>&1 | tail -40
{EVIDENCE_TAIL}
"""


def phase_env(phase):
    if phase == "static":
        return "static", {}
    if phase == "shell":
        return "toolcall", {
            "TOOL_NAME": os.environ.get("STUDY_SHELL_TOOL", "shell"),
            "TOOL_ARGS": json.dumps({"command": SHELL, "description": "study shell"}),
        }
    if phase == "mcp":
        code = 'return JSON.stringify(search({query: "uze_conformance", limit: 50}));'
        return "toolcall", {
            "TOOL_NAME": "execute",
            "TOOL_ARGS": json.dumps({"code": code}),
        }
    m = phase.split("-", 1)[1]
    return "toolcall", {
        "TOOL_NAME": "skill",
        "TOOL_ARGS": json.dumps({"id": f"study-{m}"}),
    }


def run(cfg, prov_ip):
    phases = os.environ.get("STUDY_PHASES", "static,shell").split(",")
    for phase in phases:
        mode, env = phase_env(phase)
        env.update({"DISCOVERY": "1", "TOOL_TRIGGER": f"study-{phase}"})
        prov_ip = common.start_provider(cfg, mode, env) or prov_ip
        body = STATIC if phase == "static" else tool_phase(phase)
        cmd = opencode_container(cfg, prov_ip, BUILD + body, plugins="", tty=False)
        out, log = run_container(cfg, cmd, f"opencode-{phase}", timeout=1500)
        summarize(cfg, f"opencode-{phase}", out, log, listing_prompt="study-listing")
    done(cfg)
