"""Exploration: what does Codex record of a skill, an agent and an MCP tool
being used, where a usage count could read it?

One interactive turn, the hook review answered on screen the way a person
answers it, with UZE's `hook-rows` census group installed (a hook with no
matcher, recording every `HOOK_*` value its handler is handed) beside the
`mcp-plugin` server. The world holds a plain global skill in
`~/.agents/skills/` and a plain global agent in `~/.codex/agents/`; the
person's prompt mentions the skill as `$<name>`. The provider answers the
turn by reading the skill's `SKILL.md` through the shell, dispatching the
agent with `spawn_agent` (and waiting for it), and calling the MCP server's
tool inside code mode's `exec`.

Measured, per use, on two channels: what a hook is handed (the census
records) and what the session's rollout under `~/.codex/sessions/` holds.
And, from the requests (`--discovery`), how Codex offers skills to the
model, which is what a "use" can be.

Finding (2026-10-31, codex-cli 0.166.0, code mode, the default):

- Skills are offered as `- <name>: <description> (file: r0/<name>/SKILL.md)`
  under "### Available skills", with aliases (`r0`) mapped to roots; the
  body is not in the request until used. An implicit use is the model
  reading that `SKILL.md` through the shell: a hook is handed
  `HOOK_TOOL_NATIVE=Bash`, `HOOK_TOOL=shell`, `HOOK_COMMAND=cat
  <path>/SKILL.md`; the rollout holds a `custom_tool_call` `functions.exec`
  whose `input` (JavaScript) carries the command. An explicit `$<name>`
  mention is injected as a user message `<skill><name>…</name><path>…</path>`
  and recorded in the rollout; no hook event Codex fires carries it.
- An agent's dispatch reaches a hook as `HOOK_TOOL_NATIVE=
  collaborationspawn_agent` (`HOOK_TOOL=agent.spawn`) with
  `HOOK_INPUT.agent_type`; the rollout holds a `function_call`
  `collaboration.spawn_agent` whose `arguments.agent_type` names it.
- An MCP tool called inside `exec` reaches a hook as
  `HOOK_TOOL_NATIVE=mcp__<server>__<tool>` (`mcp__uze_conformance__
  uze_conformance`), `HOOK_TOOL` empty. The rollout names it nowhere: it
  keeps only the `exec` script, which may reach the tool through `ALL_TOOLS`
  without spelling it, so the rollout cannot count MCP use.

Run: python3 conformance/lab.py --harness codex --experiment codex/usage-trace --discovery
"""

import json
import re
import subprocess

from contract import hooks
from harnesses.codex.bindings import CodexBindings
from shared import common

SKILL = "probe-skill"
SKILL_FILE = f"/work/home/.agents/skills/{SKILL}/SKILL.md"
AGENT = "probe_agent"
AGENT_FILE = f"/work/home/.codex/agents/{AGENT}.toml"
PROMPT = f"${SKILL} {hooks.PROMPT}"
#: The fixture server's tool as Codex names it natively.
MCP_NATIVE = "mcp__uze_conformance__uze_conformance"

BEFORE = f"""
mkdir -p $(dirname {SKILL_FILE}) $(dirname {AGENT_FILE})
cat > {SKILL_FILE} <<'EOF'
---
name: {SKILL}
description: Use this skill when the user asks for the probe report. UZE_UT_DESCRIPTION_3c1e
---

Write the probe report. UZE_UT_BODY_8d20
EOF
cat > {AGENT_FILE} <<'EOF'
name = "{AGENT}"
description = "Answers the probe."
developer_instructions = "Answer the probe in one word."
EOF
"""


def _calls(bindings):
    read = bindings.read_call(None, SKILL_FILE)
    spawn = {
        "tool": "collaboration.spawn_agent",
        "args": {
            "task_name": "lab_usage",
            "message": "Answer the probe.",
            "agent_type": AGENT,
            "fork_turns": "none",
        },
    }
    wait = {"tool": "collaboration.wait_agent", "args": {"timeout_ms": 30000}}
    return [read, spawn, wait, *bindings.mcp_calls()]


def _rollouts(cfg):
    """Every rollout line naming a call, reduced to its type, name and the
    keys of what it carries — and whether it names each resource."""
    script = "cat /work/home/.codex/sessions/*/*/*/rollout-*.jsonl 2>/dev/null"
    raw = subprocess.run(
        ["docker", "exec", cfg.harness_container, "sh", "-c", script],
        capture_output=True,
        text=True,
        errors="replace",
    ).stdout
    calls = []
    for line in raw.splitlines():
        try:
            entry = json.loads(line)
        except ValueError:
            continue
        payload = entry.get("payload") or {}
        kind = str(payload.get("type") or "")
        if "mcp" in kind:
            calls.append(
                {
                    "type": kind,
                    "name": json.dumps(
                        {
                            key: payload.get(key)
                            for key in ("server", "tool", "invocation", "name")
                            if payload.get(key) is not None
                        }
                    ),
                    "namespace": None,
                    "carried": "",
                }
            )
        if kind in ("function_call", "custom_tool_call"):
            calls.append(
                {
                    "type": payload["type"],
                    "name": payload.get("name"),
                    "namespace": payload.get("namespace"),
                    "carried": payload.get("arguments") or payload.get("input") or "",
                }
            )
    return calls, raw


def run(cfg, prov_ip):
    bindings = CodexBindings()

    def probe(cfg):
        return _rollouts(cfg)

    found, _, turn, (calls, raw) = hooks._scene(
        cfg,
        bindings,
        "usage",
        "flow hook-rows mcp-plugin",
        _calls(bindings),
        prompt=PROMPT,
        probe=probe,
        before=BEFORE,
    )
    census = found.get("census", [])
    summary = [{k: v[:200] for k, v in record.items()} for record in census]
    with open(f"{cfg.outdir}/usage_census.json", "w") as f:
        json.dump(summary, f, indent=1)
    with open(f"{cfg.outdir}/usage_rollout_calls.json", "w") as f:
        json.dump(calls, f, indent=1)
    print(json.dumps(summary, indent=1)[:4000], flush=True)
    print(json.dumps(calls, indent=1)[:4000], flush=True)

    def hook_saw(pattern):
        return [r for r in census if re.search(pattern, json.dumps(r))]

    def rollout_saw(pattern):
        return [c for c in calls if re.search(pattern, json.dumps(c))]

    common.check(
        "codex-skill-read-reaches-a-hook",
        bool(hook_saw(re.escape(SKILL_FILE))),
        "a hook was handed the shell read naming the skill's SKILL.md",
    )
    common.check(
        "codex-skill-read-is-in-the-rollout",
        bool(rollout_saw(re.escape(SKILL_FILE))),
        "the rollout holds the call that read SKILL.md",
    )
    common.check(
        "codex-explicit-skill-is-in-the-rollout",
        f"<name>{SKILL}</name>" in raw or "<skill>" in raw,
        "the `$skill` mention is recorded as an injected `<skill>` block",
    )
    common.check(
        "codex-agent-reaches-a-hook",
        bool(hook_saw(AGENT)),
        "a hook was handed the dispatch naming the agent",
    )
    common.check(
        "codex-agent-is-in-the-rollout",
        bool(rollout_saw(rf"spawn_agent.*{AGENT}|{AGENT}.*spawn_agent")),
        "the rollout holds spawn_agent with agent_type",
    )
    common.check(
        "codex-mcp-reaches-a-hook",
        any(r.get("HOOK_TOOL_NATIVE") == MCP_NATIVE for r in census),
        f"a hook was handed the call as `{MCP_NATIVE}`",
    )
    common.check(
        "codex-mcp-is-in-the-rollout",
        MCP_NATIVE in raw,
        f"the rollout names `{MCP_NATIVE}` anywhere, not only inside exec's script",
    )
    common.check(
        "codex-turn-settled",
        turn is not None,
        "the turn ran",
    )
