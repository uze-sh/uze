"""Exploration: what does Claude Code tell a hook, and write to its
transcript, when the person invokes a skill, an agent or an MCP prompt from
the prompt line — and what does a usage hook cost a turn?

`claude/usage-trace` measured the uses the model makes (the `Skill`,
`Agent` and MCP tools). Most real uses start with the person instead:
`/<skill>`, `/<plugin>:<skill>`, `@agent-<name>`, `/mcp__<server>__<prompt>`.
One TUI session, five turns, a logger hook with no matcher on every event
that could see one (`UserPromptSubmit`, `UserPromptExpansion`, `PreToolUse`,
`PostToolUse`, `SubagentStart`, `SubagentStop`, `InstructionsLoaded`,
`Stop`), each payload stamped with the time it ran. No UZE in the hook.

1. `@agent-usage-probe-agent …`: the provider answers as a model would, an
   `Agent` call for that agent, then the `flow` plugin's `scout`, three
   `Read`s, a `Bash`, a `Read` and the fixture's MCP tool. Beside
   the logger, `PostToolUse` carries a counting hook matched on
   `^(Skill|Agent|mcp__.*)$`, a synchronous `sleep 2` matched on `ToolSearch`
   and an `async: true` `sleep 2` matched on `Bash`.
2. `/usage-probe <args>`: a global skill typed by the person.
3. `/flow:review <args>`: a plugin skill only the person may invoke.
4. `/flow:commit /usage-probe <args>`: two skills stacked on one line.
5. `/mcp__probe__probe_prompt <args>`: a prompt of a user-scoped MCP server
   (`claude mcp add -s user`, a few lines of Node).

In turns 2–5 the provider calls no tool, so a `Skill` the harness ran on
its own would be the harness double counting. Turn 1 also opens with the
model's own `Skill` call of the same skill.

Finding (2026-10-31, 2.1.316), 12/12:

- A typed skill, global or a plugin's, and an MCP prompt fire
  `UserPromptExpansion` once each, with `command_name` (`usage-probe`,
  `flow:review`, `mcp__probe__probe_prompt`), `command_args`,
  `command_source` (`userSettings`, `plugin`, `mcp`) and `expansion_type`
  (`slash_command`, `mcp_prompt`). Two stacked skills fire it twice, one per
  skill. No `Skill` tool runs for a typed skill, and the model's own `Skill`
  call fires no expansion: the two channels never overlap.
  `UserPromptSubmit` gets the line as typed, and also fires on turns Claude
  starts itself (`<task-notification>`), so it is not a count.
- Transcript: a typed skill or prompt is a `user` entry
  `<command-name>/<name></command-name><command-args>…`, followed by an
  `isMeta` entry holding the expanded body.
- `@agent-<name>` is the raw text in `UserPromptSubmit` and an
  `agent_mention` attachment (`agentType`) in the transcript; the agent
  runs only when the model calls `Agent`, which fires `SubagentStart` once
  with `agent_type` (`usage-probe-agent`, a plugin's `flow:scout`).
  `SubagentStop` fires several times per agent (each time it goes idle), so
  it is not a count either.
- Cost: a `PostToolUse` matcher `^(Skill|Agent|mcp__.*)$` ran its hook for
  exactly the matching calls, never for `Read`, `Bash` or `ToolSearch`. A
  synchronous hook holds the turn for its whole run (`sleep 2` → 2.06 s
  before the next call, against 0.05–0.2 s otherwise); an `async: true`
  one does not (0.05 s). A minimal `sh` hook costs about 2 ms a run.

Run: python3 conformance/lab.py --harness claude --experiment claude/user-invocation --discovery
"""

import json
import subprocess
import time

from harnesses.claude.bindings import ClaudeBindings
from shared import common

SKILL = "usage-probe"
AGENT = "usage-probe-agent"
PLUGIN_AGENT = "flow:scout"
CLAUDE = "/work/home/.claude"
LOG = "/work/hooklog"
COUNT = "/work/hookcount"
LOGGER = "/work/hook-logger"
COUNTER = "/work/hook-counter"
SLEEPER = "/work/hook-sleeper"
MCP_SERVER = "/work/probe-mcp.js"
FINAL = "UZE_CONFORMANCE_PASS"

TOOLS_TURN = f"@agent-{AGENT} UZE_UI_TOOLS_TURN dispatch it and read around"
SKILL_TURN = f"/{SKILL} UZE_UI_SKILL_ARGS_1d2e"
PLUGIN_TURN = "/flow:review UZE_UI_PLUGIN_ARGS_6b0f"
STACK_TURN = f"/flow:commit /{SKILL} UZE_UI_STACK_ARGS_93ac"
MCP_TURN = "/mcp__probe__probe_prompt UZE_UI_MCP_ARGS_c7d1"

#: A user-scoped stdio MCP server with one prompt, newline-delimited
#: JSON-RPC, in the image's Node (there is no Python).
MCP_JS = r"""
const rl = require('readline').createInterface({ input: process.stdin });
const send = (o) => process.stdout.write(JSON.stringify(o) + '\n');
rl.on('line', (line) => {
  let m; try { m = JSON.parse(line); } catch { return; }
  if (m.id === undefined) return;
  const r = { jsonrpc: '2.0', id: m.id };
  if (m.method === 'initialize') r.result = {
    protocolVersion: (m.params && m.params.protocolVersion) || '2025-06-18',
    capabilities: { prompts: {}, tools: {} },
    serverInfo: { name: 'probe', version: '1.0.0' } };
  else if (m.method === 'prompts/list') r.result = { prompts: [{
    name: 'probe_prompt', description: 'Probe prompt',
    arguments: [{ name: 'topic', description: 'topic', required: false }] }] };
  else if (m.method === 'prompts/get') r.result = { description: 'Probe',
    messages: [{ role: 'user', content: { type: 'text',
      text: 'UZE_UI_MCP_PROMPT_BODY_4e7a ' + JSON.stringify(m.params.arguments || {}) } }] };
  else if (m.method === 'tools/list') r.result = { tools: [] };
  else if (m.method === 'resources/list') r.result = { resources: [] };
  else r.error = { code: -32601, message: 'not found' };
  send(r);
});
"""


def _hooks():
    def command(path, **extra):
        return {"type": "command", "command": path, **extra}

    logger = {"hooks": [command(LOGGER)]}
    events = (
        "UserPromptSubmit",
        "UserPromptExpansion",
        "PreToolUse",
        "SubagentStart",
        "SubagentStop",
        "InstructionsLoaded",
        "Stop",
    )
    hooks = {event: [logger] for event in events}
    hooks["PostToolUse"] = [
        logger,
        {"matcher": "^(Skill|Agent|mcp__.*)$", "hooks": [command(COUNTER)]},
        {"matcher": "ToolSearch", "hooks": [command(SLEEPER)]},
        {"matcher": "Bash", "hooks": [command(SLEEPER, **{"async": True})]},
    ]
    return json.dumps(hooks, separators=(",", ":"))


def _before():
    hooks = _hooks()
    return f"""
mkdir -p {CLAUDE}/skills/{SKILL} {CLAUDE}/agents
cat > {CLAUDE}/skills/{SKILL}/SKILL.md <<'EOF'
---
name: {SKILL}
description: Use when the user asks for the probe skill. UZE_UI_SKILL_DESC_3c1e
---

Follow the probe procedure. UZE_UI_SKILL_BODY_8d20 $ARGUMENTS
EOF
cat > {CLAUDE}/agents/{AGENT}.md <<'EOF'
---
name: {AGENT}
description: Use for the probe agent. UZE_UI_AGENT_DESC_51ab
---

You are the probe agent. Answer briefly.
EOF
printf '#!/bin/sh\\n{{ printf "T %%s\\\\n" "$(date +%%s%%N)"; cat; echo; }} >> {LOG}\\n' > {LOGGER}
printf '#!/bin/sh\\ncat >> {COUNT}; echo >> {COUNT}\\n' > {COUNTER}
printf '#!/bin/sh\\ncat >/dev/null; sleep 2\\n' > {SLEEPER}
chmod +x {LOGGER} {COUNTER} {SLEEPER}
cat > {MCP_SERVER} <<'EOF'
{MCP_JS}
EOF
claude mcp add -s user probe -- node {MCP_SERVER} >/dev/null 2>&1 || true
cat > /work/hooks.json <<'EOF'
{hooks}
EOF
settings={CLAUDE}/settings.json
[ -s "$settings" ] || printf '{{}}' > "$settings"
jq -s '.[0] * {{hooks: .[1]}}' "$settings" /work/hooks.json > /work/settings.next
mv /work/settings.next "$settings"
"""


def _calls(bindings):
    readme = f"{bindings.hook_project}/README.md"
    read = {"tool": "Read", "args": {"file_path": readme}}
    return [
        {"tool": "Skill", "args": {"skill": SKILL}},
        {
            "tool": "Agent",
            "args": {
                "subagent_type": AGENT,
                "description": "lab",
                "prompt": "UZE_UI_SUBAGENT answer",
            },
        },
        {
            "tool": "Agent",
            "args": {
                "subagent_type": PLUGIN_AGENT,
                "description": "lab",
                "prompt": "UZE_UI_SUBAGENT answer",
            },
        },
        read,
        read,
        read,
        {"tool": "Bash", "args": {"command": "printf ok", "description": "lab"}},
        read,
        *bindings.mcp_calls(),
    ]


def _exec(cfg, command):
    r = subprocess.run(
        ["docker", "exec", cfg.harness_container, "sh", "-c", command],
        capture_output=True,
        text=True,
        errors="replace",
        timeout=60,
    )
    return r.stdout


def _turn(bindings, tui, prompt, tries=40):
    """`hook_turn`, submitted twice: a slash command's completion popup eats
    the first Enter (`ClaudeBindings.invoke`); an empty second one is inert."""
    bindings.await_input(tui)
    before = bindings._finals_shown(tui)
    started = time.monotonic()
    tui.type(prompt)
    time.sleep(1.2)
    tui.submit()
    time.sleep(1.0)
    tui.submit()
    settled = False
    targets = list(bindings.final_markers) + list(bindings.approval_prompts)
    for _ in range(tries):
        _, _, matched = tui.wait_for(
            targets, tries=2, stop_on_death=True, squash_spaces=True
        )
        if matched in bindings.approval_prompts:
            bindings.approve(tui, matched)
            continue
        if bindings._finals_shown(tui) > before:
            settled = True
            break
    tui.quiet()
    return settled, time.monotonic() - started


def _events(log):
    """`(ns, payload)` per hook run, in the order they ran."""
    out, stamp = [], None
    for line in log.splitlines():
        if line.startswith("T "):
            stamp = int(line[2:].strip() or 0)
        elif line.startswith("{"):
            try:
                out.append((stamp, json.loads(line)))
            except ValueError:
                continue
    return out


def _gaps(events):
    """Seconds from each tool's PostToolUse logger run to the next tool's
    PreToolUse: the turn's wait on the PostToolUse hooks plus one provider
    round trip."""
    gaps = []
    for i, (ns, p) in enumerate(events):
        if p.get("hook_event_name") != "PostToolUse" or p.get("agent_id"):
            continue
        for later_ns, q in events[i + 1 :]:
            if q.get("hook_event_name") == "PreToolUse" and not q.get("agent_id"):
                gaps.append((p.get("tool_name"), (later_ns - ns) / 1e9))
                break
    return gaps


def run(cfg, prov_ip):
    bindings = ClaudeBindings()
    mode, env = bindings.sequence(_calls(bindings), "UZE_UI_TOOLS_TURN")
    env["DISCOVERY"] = "1"
    env["RESPONSE_TEXT"] = FINAL
    prov_ip = common.start_provider(cfg, mode, env)
    turns = {}
    with bindings.hook_session(
        cfg, prov_ip, "flow mcp-plugin", "user-invocation", before=_before()
    ) as tui:
        plain, ready = bindings.prepare(tui)
        if not ready:
            common.check(
                "user-invocation-ready", ready, plain[-160:].replace("\n", " ")
            )
            return
        for name, prompt in (
            ("tools", TOOLS_TURN),
            ("skill", SKILL_TURN),
            ("plugin-skill", PLUGIN_TURN),
            ("stacked", STACK_TURN),
            ("mcp-prompt", MCP_TURN),
        ):
            mark = _exec(cfg, f"wc -l < {LOG} 2>/dev/null || echo 0").strip()
            settled, seconds = _turn(bindings, tui, prompt)
            turns[name] = {
                "settled": settled,
                "seconds": round(seconds, 1),
                "from": mark,
            }
            tui.snapshot(f"turn-{name}", tui.shown()[-4000:])
        log = _exec(cfg, f"cat {LOG} 2>/dev/null")
        count = _exec(cfg, f"cat {COUNT} 2>/dev/null")
        transcripts = _exec(
            cfg, f"find {CLAUDE}/projects -name '*.jsonl' -exec cat {{}} + 2>/dev/null"
        )
        mcp_list = _exec(cfg, "claude mcp list 2>&1 | head -20")
        spawn = _exec(
            cfg,
            f'p=\'{{"hook_event_name":"UserPromptSubmit","prompt":"x"}}\'; '
            f's=$(date +%s%N); i=0; while [ $i -lt 50 ]; do echo "$p" | {LOGGER}; '
            f"i=$((i+1)); done; e=$(date +%s%N); echo $(( (e-s)/50000 ))",
        )
    for name, text in (
        ("hook.log", log),
        ("count.log", count),
        ("transcripts.jsonl", transcripts),
        ("mcp-list.txt", mcp_list),
    ):
        with open(f"{cfg.outdir}/user_invocation_{name}", "w") as f:
            f.write(text)

    events = _events(log)
    by_event = {}
    for _, p in events:
        by_event.setdefault(p.get("hook_event_name"), []).append(p)
    summary = {
        "turns": turns,
        "events": {k: len(v) for k, v in by_event.items()},
        "expansions": [
            {
                k: p.get(k)
                for k in (
                    "expansion_type",
                    "command_name",
                    "command_args",
                    "command_source",
                    "prompt",
                )
            }
            for p in by_event.get("UserPromptExpansion", [])
        ],
        "prompts": [p.get("prompt") for p in by_event.get("UserPromptSubmit", [])],
        "subagents": [
            (p.get("hook_event_name"), p.get("agent_type"))
            for p in by_event.get("SubagentStart", [])
            + by_event.get("SubagentStop", [])
        ],
        "post_tools": [p.get("tool_name") for p in by_event.get("PostToolUse", [])],
        "counted": [
            json.loads(line).get("tool_name")
            for line in count.splitlines()
            if line.startswith("{")
        ],
        "gaps": _gaps(events),
        "logger_spawn_us": spawn.strip(),
        "mcp_list": mcp_list,
    }
    with open(f"{cfg.outdir}/user_invocation_summary.json", "w") as f:
        json.dump(summary, f, indent=1)
    print(json.dumps(summary, indent=1)[:9000], flush=True)

    names = [e["command_name"] for e in summary["expansions"]]
    common.check(
        "every-turn-settled",
        all(t["settled"] for t in turns.values()),
        json.dumps(turns),
    )
    common.check(
        "skill-typed-reaches-user-prompt-expansion", SKILL in names, str(names)
    )
    common.check(
        "plugin-skill-typed-reaches-user-prompt-expansion",
        any("review" in (n or "") for n in names),
        str(names),
    )
    common.check(
        "mcp-prompt-typed-reaches-user-prompt-expansion",
        any("probe" in (n or "") for n in names),
        str(names),
    )
    common.check(
        "no-skill-tool-run-for-a-typed-skill",
        summary["post_tools"].count("Skill") == 1,
        f"one Skill, the model's own: {summary['post_tools']}",
    )
    common.check(
        "a-model-skill-call-fires-no-expansion",
        names.count(SKILL) == 2,
        f"usage-probe expanded {names.count(SKILL)} times (typed + stacked)",
    )
    common.check(
        "agent-mention-reaches-subagent-start",
        any(t == AGENT for _, t in summary["subagents"]),
        str(summary["subagents"]),
    )
    matching = [
        t
        for t in summary["post_tools"]
        if t in ("Skill", "Agent") or (t or "").startswith("mcp__")
    ]
    common.check(
        "matcher-spawns-only-for-matching-tools",
        sorted(summary["counted"]) == sorted(matching) and len(summary["counted"]) > 0,
        f"counted {summary['counted']} of {summary['post_tools']}",
    )
    common.check(
        "typed-skill-is-in-the-transcript", "UZE_UI_SKILL_ARGS_1d2e" in transcripts, ""
    )
    starts = [t for e, t in summary["subagents"] if e == "SubagentStart"]
    common.check(
        "subagent-start-fires-once-per-dispatch",
        sorted(starts) == sorted([AGENT, PLUGIN_AGENT]),
        f"starts {starts}",
    )
    gaps = dict(summary["gaps"])
    common.check(
        "a-synchronous-hook-holds-the-turn",
        gaps.get("ToolSearch", 0) >= 2.0,
        f"ToolSearch (sync sleep 2) gap {gaps.get('ToolSearch')}",
    )
    common.check(
        "an-async-hook-does-not-hold-the-turn",
        gaps.get("Bash", 9) < 1.0,
        f"Bash (async sleep 2) gap {gaps.get('Bash')}",
    )
