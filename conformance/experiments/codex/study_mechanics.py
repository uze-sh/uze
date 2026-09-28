"""Delivery-mechanics study on Codex (observation only).

See `experiments/claude/study_mechanics_lib` for the question and the
mechanics. Routes:

- direct: `~/.agents/skills/study-<m>/`, `~/.codex/agents/study-agent-<m>.toml`
  (mechanic c for agents: a symlinked sub-directory of `agents/`),
  `codex mcp add study-mcp-<m> -- <delivered server.sh>`;
- plugin: a local marketplace (`.agents/plugins/marketplace.json`, the
  catalogue shape UZE writes) whose `plugins/p-<m>` is built by mechanic m,
  installed with `codex plugin marketplace add` + `codex plugin add`.

Skill bodies: one `codex exec '$<label>'` per skill. The turn carrying
`study-tools` runs a scripted sequence: the shell probe (`exec_command`),
then `spawn_agent` + `wait_agent` for every agent, so each agent's own
request shows whether its instructions arrived.

Run: python3 conformance/lab.py --harness codex --experiment codex/study_mechanics
"""

import json

from experiments.claude.study_mechanics_lib import (
    EVIDENCE_TAIL,
    FIXTURE,
    MECHS,
    done,
    run_container,
    summarize,
    tree,
)
from harnesses.codex.scenarios import codex_container
from shared import common

HOME = "/work/home"
CACHE = f"{HOME}/.codex/plugins"
AGENT_MECHS = ["a", "b", "c", "d", "e"]
MCP_MECHS = ["a", "b", "c", "d", "e"]

SHELL = (
    "for d in /work/home/.agents/skills/study-* "
    "$(find /work/home/.codex/plugins -name SKILL.md -exec dirname {} \\;); do "
    'echo "DIR $d"; (cd "$d" && cat references/ref.md; ./scripts/run.sh) 2>&1; done'
)

AGENT_LABELS = [f"study-agent-{m}" for m in AGENT_MECHS] + [
    f"p-{m}:pa-{m}" for m in MECHS
]

AGENT_TOML = r"""
agent_toml() { # file name tag
  mkdir -p "$(dirname "$1")"
  cat > "$1" <<EOF
name = "$2"
description = "Study agent $2 for delivery-mechanic probing."
developer_instructions = "STUDY_AGENT_$3 cpr=\${CLAUDE_PLUGIN_ROOT} pr=\${PLUGIN_ROOT} end"
EOF
}
"""

FINAL = f"""
{FIXTURE}
{AGENT_TOML}
MECHS="{" ".join(MECHS)}"
mkdir -p {HOME}/.agents/skills {HOME}/.codex/agents
for m in $MECHS; do
  T=$(UP $m)
  skill_src $STORE/skd-$m/study-$m study-$m SKD_$T
  agent_toml $STORE/agd-$m/study-agent-$m.toml study-agent-$m AGD_$T
  mcp_src $STORE/mcpd-$m
  P=$STORE/pl-$m
  mkdir -p $P/.codex-plugin
  echo '{{"name":"p-'$m'","version":"1.0.0","description":"study plugin '$m'","skills":"./skills/","mcpServers":"./.mcp.json"}}' > $P/.codex-plugin/plugin.json
  skill_src $P/skills/ps-$m ps-$m SKP_$T
  agent_toml $P/agents/pa-$m.toml pa-$m AGP_$T
  mcp_src $P/mcp
  echo '{{"mcpServers":{{"pm-'$m'":{{"command":"${{PLUGIN_ROOT}}/mcp/server.sh"}}}}}}' > $P/.mcp.json
done
for c in skd-e agd-e mcpd-e pl-e; do make_ro $STORE/$c; done
for m in $MECHS; do
  deliver $m $STORE/skd-$m/study-$m {HOME}/.agents/skills/study-$m
done
for m in {" ".join(AGENT_MECHS)}; do
  if [ $m = c ]; then ln -s $STORE/agd-c {HOME}/.codex/agents/sub-c
  else deliver_file $m $STORE/agd-$m/study-agent-$m.toml {HOME}/.codex/agents/study-agent-$m.toml; fi
done
set +e
for m in {" ".join(MCP_MECHS)}; do
  if [ $m = c ]; then ln -s $STORE/mcpd-c /work/deliv/mcp-c
  else deliver_file $m $STORE/mcpd-$m/server.sh /work/deliv/mcp-$m/server.sh; fi
  codex mcp add study-mcp-$m -- /work/deliv/mcp-$m/server.sh >/dev/null 2>&1 || echo "mcp add $m failed"
done
mkdir -p /work/mkt/.agents/plugins /work/mkt/plugins
PL=""
for m in $MECHS; do
  deliver $m $STORE/pl-$m /work/mkt/plugins/p-$m
  PL="$PL{{\\"name\\":\\"p-$m\\",\\"source\\":{{\\"source\\":\\"local\\",\\"path\\":\\"./plugins/p-$m\\"}},\\"policy\\":{{\\"installation\\":\\"AVAILABLE\\",\\"authentication\\":\\"ON_INSTALL\\"}},\\"category\\":\\"Developer tools\\"}},"
done
echo "{{\\"name\\":\\"study\\",\\"interface\\":{{\\"displayName\\":\\"study\\"}},\\"plugins\\":[${{PL%,}}]}}" > /work/mkt/.agents/plugins/marketplace.json
echo '=== marketplace-add'
codex plugin marketplace add /work/mkt 2>&1
echo '=== plugin-install'
for m in $MECHS; do echo "-- p-$m"; codex plugin add p-$m@study 2>&1 | tail -5; echo "exit $?"; done
touch /work/stamp; sleep 1
WATCH="/work/mkt {HOME}/.agents/skills {HOME}/.codex/agents {CACHE}"
echo '=== plugin-list'
codex plugin list --json 2>&1 | head -c 6000; echo
echo '=== mcp-list'
codex mcp list 2>&1
echo '=== cache-tree'
{tree(CACHE)}
echo '=== direct-tree'
{tree(HOME + "/.agents/skills")}
{tree(HOME + "/.codex/agents")}
{tree("/work/deliv")}
cd /work
echo '=== turn-listing'
timeout 150 codex exec --skip-git-repo-check 'study-listing' 2>&1 | tail -15
for m in $MECHS; do
  echo "=== turn-skill-direct-$m"
  timeout 150 codex exec --skip-git-repo-check "\\$study-$m study-skill" 2>&1 | tail -6
  echo "=== turn-skill-plugin-$m"
  timeout 150 codex exec --skip-git-repo-check "\\$p-$m:ps-$m study-skill" 2>&1 | tail -6
done
echo '=== turn-tools'
timeout 400 codex exec --skip-git-repo-check 'study-tools' 2>&1 | tail -80
echo '=== cache-tree-after'
{tree(CACHE)}
{EVIDENCE_TAIL}
echo '=== uninstall'
for m in $MECHS; do echo "-- p-$m"; codex plugin remove p-$m@study 2>&1 | tail -3; echo "exit $?"; done
echo '=== cache-after-uninstall'
{tree(CACHE)}
echo '=== reinstall-e'
codex plugin add p-e@study 2>&1 | tail -3; echo "exit $?"
{tree(CACHE)}
"""


def run(cfg, prov_ip):
    sequence = [{"name": "exec_command", "args": {"cmd": SHELL}}]
    for label in AGENT_LABELS:
        sequence += [
            {
                "name": "spawn_agent",
                "namespace": "collaboration",
                "args": {
                    "task_name": "study_" + label.replace(":", "_").replace("-", "_"),
                    "message": "Run your checks.",
                    "agent_type": label,
                    "fork_turns": "none",
                },
            },
            {
                "name": "wait_agent",
                "namespace": "collaboration",
                "args": {"timeout_ms": 20000},
            },
        ]
    env = {
        "DISCOVERY": "1",
        "TOOL_SEQUENCE": json.dumps(sequence),
        "TOOL_TRIGGER": "study-tools",
        "RESPONSE_TEXT": "STUDY_DONE",
    }
    prov_ip = common.start_provider(cfg, "toolcall", env) or prov_ip
    cmd = codex_container(cfg, prov_ip, FINAL, plugins="", tty=False)
    out, log = run_container(cfg, cmd, "codex", timeout=2400)
    summarize(cfg, "codex", out, log, listing_prompt="study-listing")
    done(cfg)
