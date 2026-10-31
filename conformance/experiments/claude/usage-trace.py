"""Exploration: what does Claude Code record of a skill, an agent and an
MCP tool being used — to a hook, and in its own transcript?

A plain global skill (`~/.claude/skills/usage-probe/SKILL.md`) and a plain
global agent (`~/.claude/agents/usage-probe-agent.md`), each carrying a
nonce in its description and in its body, beside the Lab's MCP fixture
plugin. One turn: the provider scripts, in the tools' declared shapes, a
`Skill` call naming the skill, an `Agent` call naming the agent, and the
fixture's MCP tool (loaded with `ToolSearch` first, as Claude defers it). A
global `PostToolUse` hook with no matcher logs every payload it is handed.
No UZE in the hook: what is measured is the harness.

Per use, two channels: the hook's log (`tool_name` and the input field that
names the resource), and the session's transcript under
`~/.claude/projects/<dir>/<session>.jsonl` (a `tool_use` block). With
`--discovery`, how the request offers the skill: its description before any
use, its body only after.

Finding (2026-10-31, 2.1.314), 6/6, every call answered for real:

- skill: the request lists "The following skills are available for use with
  the Skill tool:" as `- <name>: <description>`; the body reaches the model
  only in the request after the call. A use is the tool `Skill` with
  `input.skill` = the name (response `{"success": true, "commandName": ...}`).
  The hook gets `tool_name: "Skill"`, `tool_input.skill`; the transcript has
  a `tool_use` block `name: "Skill"`, `input.skill`.
- agent: its description is listed; a use is the tool `Agent` with
  `input.subagent_type` = the name (launched async). Hook: `tool_name:
  "Agent"`, `tool_input.subagent_type`; transcript: `tool_use` `Agent`.
- MCP: a use is the tool `mcp__<server>__<tool>`, a plugin's as
  `mcp__plugin_<plugin>_<server>__<tool>`, after a `ToolSearch` that loads
  it. Hook and transcript name it the same way; the server is the part
  between the first `__` and the last.

Run: python3 conformance/lab.py --harness claude --experiment claude/usage-trace --discovery
"""

import json
import subprocess

from harnesses.claude.bindings import ClaudeBindings
from shared import common

SKILL = "usage-probe"
AGENT = "usage-probe-agent"
SKILL_DESC = "UZE_UT_SKILL_DESC_3c1e"
SKILL_BODY = "UZE_UT_SKILL_BODY_8d20"
AGENT_DESC = "UZE_UT_AGENT_DESC_51ab"
PROMPT = "UZE_UT_TURN use the probe skill, the probe agent and the mcp tool"
HOOK_LOG = "/work/hooklog"
HOOK = "/work/usage-hook"
CLAUDE = "/work/home/.claude"


def _before():
    hooks = json.dumps(
        {"PostToolUse": [{"hooks": [{"type": "command", "command": HOOK}]}]},
        separators=(",", ":"),
    )
    return f"""
mkdir -p {CLAUDE}/skills/{SKILL} {CLAUDE}/agents
cat > {CLAUDE}/skills/{SKILL}/SKILL.md <<'EOF'
---
name: {SKILL}
description: Use when the user asks for the probe skill. {SKILL_DESC}
---

Follow the probe procedure. {SKILL_BODY}
EOF
cat > {CLAUDE}/agents/{AGENT}.md <<'EOF'
---
name: {AGENT}
description: Use for the probe agent. {AGENT_DESC}
---

You are the probe agent. Answer briefly.
EOF
printf '#!/bin/sh\\ncat >> {HOOK_LOG}\\necho >> {HOOK_LOG}\\n' > {HOOK}
chmod +x {HOOK}
settings={CLAUDE}/settings.json
if grep -q '"' "$settings" 2>/dev/null; then
  sed -i '0,/{{/s|{{|{{"hooks":{hooks},|' "$settings"
else
  printf '%s' '{{"hooks":{hooks}}}' > "$settings"
fi
"""


def _exec(cfg, command):
    r = subprocess.run(
        ["docker", "exec", cfg.harness_container, "sh", "-c", command],
        capture_output=True,
        text=True,
        errors="replace",
        timeout=60,
    )
    return r.stdout


def _uses(text, needle_pairs):
    """Whether every `(a, b)` pair appears together on one line of `text`."""
    lines = text.splitlines()
    return all(any(a in line and b in line for line in lines) for a, b in needle_pairs)


def run(cfg, prov_ip):
    bindings = ClaudeBindings()
    calls = [
        {"tool": "Skill", "args": {"skill": SKILL}},
        {
            "tool": "Agent",
            "args": {
                "subagent_type": AGENT,
                "description": "lab",
                "prompt": "UZE_UT_SUBAGENT answer",
            },
        },
        *bindings.mcp_calls(),
    ]
    mode, env = bindings.sequence(calls, PROMPT)
    env["DISCOVERY"] = "1"
    prov_ip = common.start_provider(cfg, mode, env)
    with bindings.hook_session(
        cfg, prov_ip, "flow mcp-plugin", "usage-trace", before=_before()
    ) as tui:
        plain, ready = bindings.prepare(tui)
        if not ready:
            common.check("usage-trace-ready", ready, plain[-160:].replace("\n", " "))
            return
        turn = bindings.hook_turn(tui, PROMPT)
        tui.snapshot("usage-trace", turn.plain)
        hook_log = _exec(cfg, f"cat {HOOK_LOG} 2>/dev/null")
        transcripts = _exec(
            cfg,
            f"find {CLAUDE}/projects -name '*.jsonl' -exec cat {{}} + 2>/dev/null",
        )
    with open(f"{cfg.outdir}/usage_trace_hook.log", "w") as f:
        f.write(hook_log)
    with open(f"{cfg.outdir}/usage_trace_transcripts.jsonl", "w") as f:
        f.write(transcripts)

    mcp = bindings.MCP_TOOL
    common.check(
        "skill-use-reaches-post-tool-use-hook",
        _uses(hook_log, [('"tool_name":"Skill"', f'"skill":"{SKILL}"')]),
        "tool_name Skill, tool_input.skill names it",
    )
    common.check(
        "skill-use-is-in-the-transcript",
        _uses(transcripts, [('"name":"Skill"', f'"skill":"{SKILL}"')]),
        "a tool_use block Skill with input.skill",
    )
    common.check(
        "agent-use-reaches-post-tool-use-hook",
        _uses(hook_log, [('"tool_name":"Agent"', f'"subagent_type":"{AGENT}"')]),
        "tool_name Agent, tool_input.subagent_type names it",
    )
    common.check(
        "agent-use-is-in-the-transcript",
        _uses(transcripts, [('"name":"Agent"', f'"subagent_type":"{AGENT}"')]),
        "a tool_use block Agent with input.subagent_type",
    )
    common.check(
        "mcp-use-reaches-post-tool-use-hook",
        f'"tool_name":"{mcp}"' in hook_log,
        f"tool_name {mcp}",
    )
    common.check(
        "mcp-use-is-in-the-transcript",
        f'"name":"{mcp}"' in transcripts,
        f"a tool_use block {mcp}",
    )
