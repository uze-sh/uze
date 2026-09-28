"""Delivery-mechanics study on Antigravity CLI (observation only).

See `experiments/claude/study_mechanics_lib` for the question and the
mechanics. Routes:

- direct: `~/.gemini/antigravity-cli/skills/study-<m>/` (the global skills
  root UZE links into), `~/.gemini/antigravity-cli/agents/study-agent-<m>.md`
  (mechanic c: a symlinked sub-directory of `agents/`),
  `agy mcp add study-mcp-<m> <delivered server.sh>`;
- plugin: `agy plugin install <dir>` of a directory built by mechanic m
  (plugin.json, skills/, agents/, mcp_config.json with a `${PLUGIN_ROOT}`
  server and an absolute-path server into the delivered source).

The provider scripts a call on every turn declaring the tool, so each tool
is its own phase (fresh container, fresh provider): `static` (inventory,
listing, `/study-<m>` turns, one `--agent` turn per agent), `shell`
(`run_command` reading every copy's extras by relative path).

Run: python3 conformance/lab.py --harness antigravity --experiment antigravity/study_mechanics
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
from harnesses.antigravity.scenarios import agy_setup
from shared import common

HOME = "/work/home"
AGY = f"{HOME}/.gemini/antigravity-cli"
PLUGINS = f"{HOME}/.gemini/config/plugins"
AGENT_MECHS = ["a", "b", "c", "d", "e"]
MCP_MECHS = ["a", "b", "c", "d", "e"]
PRINT = "timeout 150 agy --dangerously-skip-permissions --print-timeout 100s"

SHELL = (
    f"for d in {AGY}/skills/study-* $(find {HOME}/.gemini/config/plugins -name SKILL.md "
    "-exec dirname {} \\;); do echo DIR $d; (cd $d && cat references/ref.md; "
    "./scripts/run.sh) 2>&1; done"
)

BUILD = f"""
{FIXTURE}
MECHS="{" ".join(MECHS)}"
mkdir -p {AGY}/skills {AGY}/agents
for m in $MECHS; do
  T=$(UP $m)
  skill_src $STORE/skd-$m/study-$m study-$m SKD_$T
  agent_md $STORE/agd-$m/study-agent-$m.md study-agent-$m AGD_$T
  mcp_src $STORE/mcpd-$m
  P=$STORE/pl-$m
  mkdir -p $P
  echo '{{"name":"p-'$m'","description":"study plugin '$m'"}}' > $P/plugin.json
  skill_src $P/skills/ps-$m ps-$m SKP_$T
  agent_md $P/agents/pa-$m.md pa-$m AGP_$T
  mcp_src $P/mcp
  echo '{{"mcpServers":{{"pm-'$m'":{{"command":"${{PLUGIN_ROOT}}/mcp/server.sh"}},"pmabs-'$m'":{{"command":"/work/src/p-'$m'/mcp/server.sh"}}}}}}' > $P/mcp_config.json
done
for c in skd-e agd-e mcpd-e pl-e; do make_ro $STORE/$c; done
for m in $MECHS; do
  deliver $m $STORE/skd-$m/study-$m {AGY}/skills/study-$m
  deliver $m $STORE/pl-$m /work/src/p-$m
done
for m in {" ".join(AGENT_MECHS)}; do
  if [ $m = c ]; then ln -s $STORE/agd-c {AGY}/agents/sub-c
  else deliver_file $m $STORE/agd-$m/study-agent-$m.md {AGY}/agents/study-agent-$m.md; fi
done
set +e
for m in {" ".join(MCP_MECHS)}; do
  if [ $m = c ]; then ln -s $STORE/mcpd-c /work/deliv/mcp-c
  else deliver_file $m $STORE/mcpd-$m/server.sh /work/deliv/mcp-$m/server.sh; fi
  agy mcp add study-mcp-$m /work/deliv/mcp-$m/server.sh >/dev/null 2>&1 || echo "mcp add $m failed"
done
echo '=== plugin-validate'
for m in $MECHS; do echo "-- p-$m"; agy plugin validate /work/src/p-$m 2>&1 | tail -6; done
echo '=== plugin-install'
for m in $MECHS; do echo "-- p-$m"; agy plugin install /work/src/p-$m 2>&1 | tail -4; echo "exit $?"; done
touch /work/stamp; sleep 1
WATCH="/work/src {AGY}/skills {AGY}/agents {PLUGINS}"
cd /work
"""

STATIC = f"""
echo '=== plugin-list'
agy plugin list 2>&1 | head -40
echo '=== mcp-list'
timeout 60 agy mcp list 2>&1 | head -40
echo '=== plugins-tree'
{tree(PLUGINS)}
echo '=== direct-tree'
{tree(AGY + "/skills")}
{tree(AGY + "/agents")}
{tree("/work/deliv")}
echo '=== turn-listing'
{PRINT} --print 'study-listing' 2>&1 | tail -5
for m in $MECHS; do
  echo "=== turn-skill-direct-$m"; {PRINT} --print "/study-$m" 2>&1 | tail -4
  echo "=== turn-skill-plugin-$m"; {PRINT} --print "/ps-$m" 2>&1 | tail -4
done
for a in study-agent-a study-agent-b study-agent-c sub-c/study-agent-c study-agent-d study-agent-e pa-a p-a:pa-a; do
  echo "=== turn-agent-$a"; {PRINT} --agent $a --print 'agent probe' 2>&1 | tail -4
done
echo '=== plugins-tree-after'
{tree(PLUGINS)}
{EVIDENCE_TAIL}
echo '=== uninstall'
for m in $MECHS; do echo "-- p-$m"; agy plugin uninstall p-$m 2>&1 | tail -3; echo "exit $?"; done
{tree(PLUGINS)}
echo '=== reinstall-e'
agy plugin install /work/src/p-e 2>&1 | tail -3; echo "exit $?"
"""

SHELL_PHASE = f"""
echo '=== turn-shell'
{PRINT} --print 'study-shell' 2>&1 | tail -60
{EVIDENCE_TAIL}
"""


def run(cfg, prov_ip):
    phases = os.environ.get("STUDY_PHASES", "static,shell").split(",")
    for phase in phases:
        env = {"DISCOVERY": "1"}
        if phase == "shell":
            mode = "toolcall"
            env.update(
                {
                    "TOOL_NAME": "run_command",
                    "FC_ARGS": json.dumps(
                        {
                            "CommandLine": SHELL,
                            "Cwd": "/work",
                            "WaitMsBeforeAsync": 5000,
                            "toolSummary": "study shell",
                            "toolAction": "Running command",
                        }
                    ),
                }
            )
        else:
            mode = "static"
        prov_ip = common.start_provider(cfg, mode, env) or prov_ip
        body = STATIC if phase == "static" else SHELL_PHASE
        setup = agy_setup(
            cfg, prov_ip, include_mcp=False, plugins="", final_cmd=BUILD + body
        )
        cmd = common.docker_base(cfg, prov_ip, setup, tty=False)
        out, log = run_container(cfg, cmd, f"agy-{phase}", timeout=2400)
        summarize(cfg, f"agy-{phase}", out, log, listing_prompt="study-listing")
    done(cfg)
