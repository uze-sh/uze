"""Delivery-mechanics study on Claude Code (observation only).

See `study_mechanics_lib` for the question and the mechanics. Routes:

- direct: `~/.claude/skills/study-<m>/`, `~/.claude/agents/study-agent-<m>.md`
  (mechanic c for agents: a symlinked sub-directory of `agents/`),
  `claude mcp add --scope user study-mcp-<m> -- <delivered server.sh>`;
- plugin: a local marketplace whose `plugins/p-<m>` is built by mechanic m,
  installed with `claude plugin marketplace add` + `claude plugin install`.

One container, one provider in toolcall mode that answers only the turn
carrying `study-shell` with a Bash call; every other turn is plain text.

Run: python3 conformance/lab.py --harness claude --experiment claude/study_mechanics
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
from harnesses.claude.scenarios import generate_certs
from shared import common

HOME = "/work/home"
CACHE = f"{HOME}/.claude/plugins/cache"
AGENT_MECHS = ["a", "b", "c", "d", "e"]
MCP_MECHS = ["a", "b", "c", "d", "e"]

SHELL = (
    "for d in /work/home/.claude/skills/study-* "
    "/work/home/.claude/plugins/cache/*/p-*/*/skills/*; do "
    'echo "DIR $d"; (cd "$d" && cat references/ref.md; ./scripts/run.sh) 2>&1; done'
)

SETUP = f"""
set -e
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin
export HOME={HOME} CLAUDE_CONFIG_DIR={HOME}/.claude UZE_HOME={HOME}/.uze
export ANTHROPIC_API_KEY=uze-conformance-invalid-by-design
export NODE_EXTRA_CA_CERTS=/app/ca.crt
mkdir -p {HOME}/.claude/skills {HOME}/.claude/agents
cp /app/fixtures/claude.json {HOME}/.claude.json
{FIXTURE}
MECHS="{" ".join(MECHS)}"
# --- sources
for m in $MECHS; do
  T=$(UP $m)
  skill_src $STORE/skd-$m/study-$m study-$m SKD_$T
  agent_md $STORE/agd-$m/study-agent-$m.md study-agent-$m AGD_$T
  mcp_src $STORE/mcpd-$m
  P=$STORE/pl-$m
  mkdir -p $P/.claude-plugin
  echo '{{"name":"p-'$m'","version":"1.0.0","description":"study plugin '$m'"}}' > $P/.claude-plugin/plugin.json
  skill_src $P/skills/ps-$m ps-$m SKP_$T
  agent_md $P/agents/pa-$m.md pa-$m AGP_$T
  mcp_src $P/mcp
  echo '{{"mcpServers":{{"pm-'$m'":{{"command":"${{CLAUDE_PLUGIN_ROOT}}/mcp/server.sh"}}}}}}' > $P/.mcp.json
done
for c in skd-e agd-e mcpd-e pl-e; do make_ro $STORE/$c; done
# --- direct route
for m in $MECHS; do
  deliver $m $STORE/skd-$m/study-$m {HOME}/.claude/skills/study-$m
done
for m in {" ".join(AGENT_MECHS)}; do
  if [ $m = c ]; then ln -s $STORE/agd-c {HOME}/.claude/agents/sub-c
  else deliver_file $m $STORE/agd-$m/study-agent-$m.md {HOME}/.claude/agents/study-agent-$m.md; fi
done
for m in {" ".join(MCP_MECHS)}; do
  if [ $m = c ]; then ln -s $STORE/mcpd-c /work/deliv/mcp-c
  else deliver_file $m $STORE/mcpd-$m/server.sh /work/deliv/mcp-$m/server.sh; fi
  claude mcp add --scope user study-mcp-$m -- /work/deliv/mcp-$m/server.sh >/dev/null 2>&1 || echo "mcp add $m failed"
done
# --- plugin route
mkdir -p /work/mkt/.claude-plugin /work/mkt/plugins
PL=""
for m in $MECHS; do
  deliver $m $STORE/pl-$m /work/mkt/plugins/p-$m
  PL="$PL{{\\"name\\":\\"p-$m\\",\\"source\\":\\"./plugins/p-$m\\"}},"
done
echo "{{\\"name\\":\\"study\\",\\"owner\\":{{\\"name\\":\\"lab\\"}},\\"plugins\\":[${{PL%,}}]}}" > /work/mkt/.claude-plugin/marketplace.json
set +e
echo '=== marketplace-add'
claude plugin marketplace add /work/mkt 2>&1
echo '=== plugin-install'
for m in $MECHS; do echo "-- p-$m"; claude plugin install p-$m@study 2>&1; echo "exit $?"; done
touch /work/stamp; sleep 1
WATCH="/work/mkt {HOME}/.claude/skills {HOME}/.claude/agents {CACHE}"
echo '=== plugin-list'
claude plugin list --json 2>&1
echo '=== mcp-list'
timeout 60 claude mcp list 2>&1
echo '=== cache-tree'
{tree(CACHE)}
echo '=== mkt-tree'
{tree("/work/mkt/plugins")}
echo '=== direct-tree'
{tree(HOME + "/.claude/skills")}
{tree(HOME + "/.claude/agents")}
{tree("/work/deliv")}
echo '=== turn-listing'
timeout 150 claude -p 'study-listing' --permission-mode bypassPermissions --output-format json 2>&1 | head -c 1500
echo
for m in $MECHS; do
  echo "=== turn-skill-direct-$m"
  timeout 150 claude -p "/study-$m" --permission-mode bypassPermissions --output-format json 2>&1 | head -c 600; echo
  echo "=== turn-skill-plugin-$m"
  timeout 150 claude -p "/p-$m:ps-$m" --permission-mode bypassPermissions --output-format json 2>&1 | head -c 600; echo
  echo "=== turn-agent-plugin-$m"
  timeout 150 claude --agent p-$m:pa-$m -p "agent probe" --permission-mode bypassPermissions --output-format json 2>&1 | head -c 600; echo
done
for m in {" ".join(AGENT_MECHS)}; do
  echo "=== turn-agent-direct-$m"
  timeout 150 claude --agent study-agent-$m -p "agent probe" --permission-mode bypassPermissions --output-format json 2>&1 | head -c 600; echo
done
echo '=== turn-shell'
timeout 150 claude -p 'study-shell' --permission-mode bypassPermissions --output-format json 2>&1 | head -c 4000
echo
echo '=== cache-tree-after'
{tree(CACHE)}
{EVIDENCE_TAIL}
echo '=== uninstall'
for m in $MECHS; do echo "-- p-$m"; claude plugin uninstall p-$m@study 2>&1; echo "exit $?"; done
echo '=== cache-after-uninstall'
{tree(CACHE)}
echo '=== reinstall-e'
claude plugin install p-e@study 2>&1; echo "exit $?"
{tree(CACHE)}
"""


def run(cfg, prov_ip):
    env = {
        "DISCOVERY": "1",
        "TOOL_NAME": "Bash",
        "TOOL_ARGS": json.dumps({"command": SHELL, "description": "study shell"}),
        "TOOL_TRIGGER": "study-shell",
        "RESPONSE_TEXT": "STUDY_DONE",
        "FINAL_TEXT": "STUDY_DONE",
    }
    prov_ip = common.start_provider(cfg, "toolcall", env) or prov_ip
    cmd = common.docker_base(cfg, prov_ip, SETUP, tty=False)
    ca_crt, _, _ = generate_certs(cfg)
    i = cmd.index(common.HARNESS_IMAGE)
    cmd = cmd[:i] + ["-v", f"{ca_crt}:/app/ca.crt:ro"] + cmd[i:]
    out, log = run_container(cfg, cmd, "claude", timeout=1800)
    summarize(cfg, "claude", out, log, listing_prompt="study-listing")
    done(cfg)
