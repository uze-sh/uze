"""Exploration: what does OpenCode record of a skill, an agent and an MCP
tool being used, where a usage count could read it?

One headless turn (`opencode run --format json`) in which the provider
scripts three calls in the shape the request declares: the skill tool on a
global skill, the subagent tool on a global agent, and the Code Mode
`execute` tool calling a tool of the Lab's MCP fixture. Each is then looked
for in three places, no UZE in the loop:

- a global plugin's `ctx.tool.hook("execute.after")`, which logs the tool's
  name and input — what a usage tracker riding OpenCode's plugin API sees;
- `opencode stats --tools --json`, the harness's published statistics;
- `opencode session export <id>`, the harness's published record of the
  session, for a tool part naming the resource.

OpenCode's database is never opened.

Finding (2026-10-31, OpenCode v2.2.5):

- skill: tool `skill`, input `{"id": "<skill name>"}`;
- agent: tool `subagent`, input `{"agent": "<agent name>", ...}`;
- MCP: the model calls Code Mode's `execute` with
  `tools["<server namespace>"].<tool>(...)` in `code`, and the tool hook
  also fires for the MCP tool itself, named
  `<server namespace>_<tool>` (`uze-mcp-conformance_uze-lab-uze-conformance_uze_conformance`),
  so the server is named without reading the code;
- `execute.after` is handed each of them with its input: a plugin sees
  every use, naming the resource;
- `opencode session export` carries each as a completed `tool` part
  (`name`, `state.input`) naming the resource;
- `opencode stats --tools --json` counts calls per tool only (`skill`,
  `subagent`, `execute`): it never names the skill, agent or server, and
  counts `sessions` and `subagents` apart.

Run: python3 conformance/lab.py --harness opencode --experiment opencode/usage-trace --discovery
"""

import json
import subprocess

from harnesses.opencode.scenarios import opencode_setup
from shared import common

SKILL = "probe-skill"
AGENT = "probe-agent"
SKILL_MARK = "UZE_UT_SKILL_6c1e"
AGENT_MARK = "UZE_UT_AGENT_2d9b"
TURN = "UZE_UT_TURN use the probe things"
HOOK_LOG = "/work/tool-hook.jsonl"
CONFIG = "/work/home/.config/opencode"
#: OpenCode 2.x offers MCP servers through Code Mode: one namespace per
#: server (see the provider's `MCP_NAMESPACE`).
MCP_NAMESPACE = "uze-mcp-conformance_uze-lab-uze-conformance"
SEQUENCE = [
    {"name": "skill", "args": json.dumps({"id": SKILL})},
    {
        "name": "subagent",
        "args": json.dumps(
            {"agent": AGENT, "description": "probe", "prompt": "probe the agent"}
        ),
    },
    {
        "name": "execute",
        "args": json.dumps(
            {"code": f'return await tools["{MCP_NAMESPACE}"].uze_conformance({{}});'}
        ),
    },
]

PLUGIN = (
    r"""// No import: 2.x cannot resolve `@opencode-ai/plugin` from the global
// plugin directory (measured by `opencode/session-start`).
const LOG = "%s";
export default {
  id: "usage-probe",
  async setup(ctx) {
    await ctx.tool.hook("execute.after", async (event) => {
      try {
        const before = await Bun.file(LOG).text().catch(() => "");
        const line = JSON.stringify({ tool: event.tool, input: event.input ?? null });
        await Bun.write(LOG, before + line + "\n");
      } catch {}
    });
  },
};
"""
    % HOOK_LOG
)


def script():
    return f"""
set +e
mkdir -p {CONFIG}/skills/{SKILL} {CONFIG}/agents {CONFIG}/plugins /work/project
cat > {CONFIG}/skills/{SKILL}/SKILL.md <<'EOF'
---
name: {SKILL}
description: Use this skill when the user asks for the probe things. {SKILL_MARK}
---

Do the probe things.
EOF
cat > {CONFIG}/agents/{AGENT}.md <<'EOF'
---
description: Probes things when asked. {AGENT_MARK}
mode: subagent
---

You probe things.
EOF
cat > {CONFIG}/plugins/usage-probe.ts <<'EOF'
{PLUGIN}EOF
cd /work/project && git init -q .
# decision: headless-permissions
timeout 180 opencode run --standalone --auto --format json "{TURN}" </dev/null > /work/run.out 2>&1
echo "run-exit=$?"
sid=$(node -e 'const l=require("fs").readFileSync("/work/run.out","utf8").split("\\n").filter(Boolean).map(s=>{{try{{return JSON.parse(s)}}catch(e){{return {{}}}}}}).map(e=>e.sessionID||e.properties?.sessionID||e.part?.sessionID).filter(Boolean);console.log(l.length?l[0]:"")')
echo "===== sid"
echo "$sid"
echo "===== hook"
cat {HOOK_LOG} 2>/dev/null
echo "===== stats"
timeout 60 opencode stats --days 0 --tools --json 2>&1 | head -c 6000
echo
echo "===== export"
timeout 60 opencode session export "$sid" 2>&1 | head -c 20000
echo
echo "===== end"
"""


def section(out, name):
    start = out.find(f"===== {name}")
    if start < 0:
        return ""
    rest = out[start + len(name) + 6 :]
    end = rest.find("\n===== ")
    return rest if end < 0 else rest[:end]


def run(cfg, prov_ip):
    prov_ip = (
        common.start_provider(
            cfg,
            "toolcall",
            {
                "TOOL_SEQUENCE": json.dumps(SEQUENCE),
                "TOOL_TRIGGER": "UZE_UT_TURN",
                "DISCOVERY": "1",
            },
        )
        or prov_ip
    )
    r = subprocess.run(
        common.docker_base(
            cfg,
            prov_ip,
            opencode_setup(cfg, prov_ip, script(), plugins="flow mcp-plugin"),
            tty=False,
        ),
        capture_output=True,
        text=True,
        errors="replace",
        timeout=500,
    )
    out = r.stdout
    with open(f"{cfg.outdir}/usage_trace.out", "w") as f:
        f.write(out)
        f.write("\n=== stderr\n" + r.stderr)
    print(out[-9000:], flush=True)

    hook, stats, export = (
        section(out, "hook"),
        section(out, "stats"),
        section(out, "export"),
    )
    uses = {
        "skill": ('"tool":"skill"', f'"{SKILL}"'),
        "agent": ('"tool":"subagent"', f'"{AGENT}"'),
        "mcp": ('"tool":"execute"', MCP_NAMESPACE),
    }
    compact = hook.replace(" ", "")
    for use, (tool, name) in uses.items():
        common.check(
            f"{use}-reaches-the-tool-hook",
            tool in compact and name in hook,
            "execute.after is handed the tool and the input naming the resource",
        )
        common.check(
            f"{use}-is-in-the-session-export",
            name in export,
            "opencode session export carries a tool part naming the resource",
        )
        bare = tool.split(":")[1].strip('"')
        common.check(
            f"{use}-stats-count-the-tool-not-the-resource",
            f'"name": "{bare}"' in stats and name not in stats,
            "opencode stats counts calls per tool and never names the resource",
        )
    common.check(
        "mcp-tool-reaches-the-tool-hook-by-its-server",
        f'"tool":"{MCP_NAMESPACE}_uze_conformance"' in compact,
        "the MCP tool itself reaches the hook as <server namespace>_<tool>",
    )
