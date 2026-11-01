"""Exploration: what Antigravity records when a skill, an agent and an MCP
tool are used, where a usage count could read it.

One headless turn in which the provider makes three calls in the shape the
request declares: the read of a global skill's `SKILL.md` (how the harness
tells a model to use a skill, `antigravity/skill-activation`), an
`invoke_subagent` of a global agent, and a `call_mcp_tool` of the Lab's MCP
fixture. A global `PostToolUse` hook with no matcher logs every payload, and
the conversation's own `transcript.jsonl` is read back. No UZE hook in the
loop: what is measured is the harness.

Finding (2026-10-31, 1.4.12): all three reach both channels, each with no
error and the MCP fixture's proof in its result.

- hook (`PostToolUse`, payload `toolCall.{name,args}`): `read_file` with
  `Path` = the skill's listed `SKILL.md`; `invoke_subagent` with `TypeName`
  = the agent's name (`Role`, `Prompt` beside it); `call_mcp_tool` with
  `ServerName` = `<plugin>_<server>` and `ToolName`.
- transcript (`brain/<id>/.system_generated/logs/transcript.jsonl`):
  the same calls as `tool_calls[{name,args}]` entries of the conversation.

Run: python3 conformance/lab.py --harness antigravity --experiment antigravity/usage-trace --discovery
"""

import json
import subprocess

from harnesses.antigravity.scenarios import agy_setup
from shared import common

HOME = "/work/home/.gemini"
SKILL = "probe-skill"
SKILL_FILE = f"{HOME}/antigravity-cli/skills/{SKILL}/SKILL.md"
AGENT = "probe_agent"
AGENT_FILE = f"{HOME}/antigravity-cli/agents/{AGENT}.md"
HOOK_LOG = "/work/hooklog"
#: The provider's own default MCP call (harnesses/antigravity/provider.py):
#: since 1.2.2 a plugin's server is named `<plugin>_<server>`.
MCP_ARGS = {
    "ServerName": "uze-mcp-conformance_uze-conformance",
    "ToolName": "uze_conformance",
    "Arguments": {},
}
GLOBAL_HOOKS = f"{HOME}/config/hooks.json"

SEQUENCE = [
    {"name": "read_file", "args": {"Path": SKILL_FILE}},
    {
        "name": "invoke_subagent",
        "args": {"TypeName": AGENT, "Role": "Lab Probe", "Prompt": "Report back."},
    },
    {"name": "call_mcp_tool", "args": MCP_ARGS},
]

PRELUDE = f"""
mkdir -p $(dirname {SKILL_FILE}) $(dirname {AGENT_FILE}) {HOME}/config
cat > {SKILL_FILE} <<'EOF'
---
name: {SKILL}
description: Use this skill when the user asks for the probe report.
---

Write the probe report.
EOF
cat > {AGENT_FILE} <<'EOF'
---
name: {AGENT}
description: Answers probe requests.
---

Answer in one line.
EOF
cat > {GLOBAL_HOOKS} <<'EOF'
{{
  "usage-probe": {{
    "PostToolUse": [
      {{
        "matcher": "*",
        "hooks": [
          {{ "type": "command", "command": "cat >> {HOOK_LOG}; echo >> {HOOK_LOG}; printf '{{}}'" }}
        ]
      }}
    ]
  }}
}}
EOF
"""


def final_script():
    # decision: experiment-isolation
    return f"""
cd /work && mkdir -p project && cd project
# decision: experiment-isolation
agy --print "write the probe report" --output-format stream-json \\
  --dangerously-skip-permissions --print-timeout 120s --log-file /work/agy.log >/dev/null 2>&1 || true
echo '===== hook log ====='
cat {HOOK_LOG} 2>/dev/null | head -c 12000
echo
echo '===== transcripts ====='
for t in {HOME}/antigravity-cli/brain/*/.system_generated/logs/transcript.jsonl; do
  echo "== $t"; head -c 12000 "$t"; echo
done
echo '===== end ====='
"""


def _section(out, start, end):
    return out.split(start, 1)[-1].split(end, 1)[0]


def _calls(text, tool):
    """Every JSON object in `text` that names `tool` as a call, by line."""
    found = []
    for line in text.splitlines():
        line = line.strip()
        if not line.startswith("{") or tool not in line:
            continue
        try:
            found.append(json.loads(line))
        except ValueError:
            continue
    return found


def run(cfg, prov_ip):
    common.start_provider(
        cfg,
        "toolcall",
        {
            "TOOL_SEQUENCE": json.dumps(SEQUENCE),
            "FINAL_TEXT": "UZE_UT_DONE",
            "DISCOVERY": "1",
        },
    )
    setup = agy_setup(
        cfg,
        prov_ip,
        include_mcp=True,
        final_cmd=final_script(),
        plugins="mcp-plugin",
        prelude=PRELUDE,
    )
    r = subprocess.run(
        common.docker_base(cfg, prov_ip, setup, tty=False),
        capture_output=True,
        text=True,
        errors="replace",
        timeout=400,
    )
    out = r.stdout
    with open(f"{cfg.outdir}/usage_trace.out", "w") as f:
        f.write(out)
    print(out[-6000:], flush=True)
    if r.stderr:
        print(r.stderr[-1500:], flush=True)

    hooks = _section(out, "===== hook log =====", "===== transcripts =====")
    transcripts = _section(out, "===== transcripts =====", "===== end =====")
    server = MCP_ARGS["ServerName"]
    for channel, text in (("hook", hooks), ("transcript", transcripts)):
        common.check(
            f"skill-use-reaches-the-{channel}",
            "read_file" in text and SKILL_FILE in text,
            "read_file of the skill's SKILL.md, path in the args",
        )
        common.check(
            f"agent-use-reaches-the-{channel}",
            "invoke_subagent" in text and AGENT in text,
            "invoke_subagent naming the agent in TypeName",
        )
        common.check(
            f"mcp-use-reaches-the-{channel}",
            "call_mcp_tool" in text and server in text,
            f"call_mcp_tool naming the server ({server}) and the tool",
        )
