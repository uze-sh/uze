"""Exploration: what Antigravity records when the PERSON invokes a skill or
an agent from the prompt, how a skill read looks when the path is not the
listed one or the reader is a subagent, and what a usage hook costs.

Each case is its own world (one container, one headless `agy --print`
turn) with a global `~/.gemini/config/hooks.json` whose `census` hook logs
every event the shipped `hooks.md` lists (`SessionStart`, `PreInvocation`,
`PreToolUse`, `PostToolUse`, `PostInvocation`, `Stop`), prefixed by the
event's name. Read back: the hook log, both transcripts of every
conversation (`transcript.jsonl` and the `transcript_full.jsonl` a hook is
pointed at), and, with `--discovery`, whether a skill's body reached the
model with no tool call. No UZE hook in the loop: what is measured is the
harness. `UI_CASES=a,b` runs only those cases.

Finding (2026-10-31, 1.4.12, headless `--print`):

- A skill the person invokes is a LEADING `/<name>` (`/<plugin>:<skill>` for
  a plugin's, user-only ones included). Its body is injected into the
  user's message with no tool call, so no tool hook sees it. Every hook
  event carries `lastUserInput` raw (`/probe-skill write it …`),
  `SessionStart` and `PreInvocation` first. The transcript's `USER_INPUT`
  `content` holds the command as typed inside `<USER_REQUEST>` and, in
  `<ADDITIONAL_METADATA>`, `/<name> is a [Slash Command]:` followed by
  `<SKILL>The user has explicitly invoked the (<name>) skill. …`. A
  `/<name>` later in the prompt is plain text: not expanded.
- An agent: `@<agent>` is plain text (no dispatch, no injection). `--agent
  <name>` makes it the main agent's system prompt, and no hook payload or
  transcript names it. A model's dispatch is `invoke_subagent` (TypeName),
  as `usage-trace` measured.
- A skill read: `read_file` takes only an absolute `Path`. A `~` or `../`
  path is refused (`must be an absolute path`), and so is a read under
  `~/.gemini/antigravity-cli` outside the skills (protection boundary). A
  refused call reaches `PreToolUse` and never `PostToolUse`. `run_command
  cat <SKILL.md>` reaches both with the path inside `CommandLine`.
- A subagent is offered `view_file` (`AbsolutePath`) instead of `read_file`,
  and no `run_command`. Its calls reach the global hooks under its own
  `conversationId`, with `lastUserInput` empty; a `SessionStart` opens it.
- Cost: hooks are synchronous (`hooks.md`: no async). A matcher filters
  before any spawn: `invoke_subagent|call_mcp_tool` ran 0 times on six
  `read_file`s, `*` ran 6. A minimal `sh` handler adds no measurable time
  (turn 422–487 ms with or without). A 2 s handler costs 2 s per matching
  call on `PostToolUse` (+12 s on six) and per model call on
  `PreInvocation` (+14 s on seven).
- `~/.gemini/config/hooks.json` holds several named groups (`uze-usage`,
  `someone-else`), each of which ran, beside a plugin's own hooks
  (`hook-rows`' census recorded too).

Run: python3 conformance/lab.py --harness antigravity --experiment antigravity/user-invocation --discovery
"""

import json
import os
import subprocess

from harnesses.antigravity.scenarios import agy_setup
from shared import common

HOME = "/work/home/.gemini"
SKILL = "probe-skill"
SKILL_FILE = f"{HOME}/antigravity-cli/skills/{SKILL}/SKILL.md"
SKILL_DESC = "UZE_UI_DESC_4b7e"
SKILL_BODY = "UZE_UI_BODY_9c3f"
AGENT = "probe_agent"
AGENT_FILE = f"{HOME}/antigravity-cli/agents/{AGENT}.md"
AGENT_BODY = "UZE_UI_AGENT_BODY_2d61"
HOOK_LOG = "/work/hooklog"
LOGGER = "/work/log-hook"
SPAWNS = "/work/spawns"
GLOBAL_HOOKS = f"{HOME}/config/hooks.json"
EVENTS = [
    "SessionStart",
    "PreInvocation",
    "PreToolUse",
    "PostToolUse",
    "PostInvocation",
    "Stop",
]
FLAT = {"SessionStart", "PreInvocation", "PostInvocation", "Stop"}
MCP_ARGS = {
    "ServerName": "uze-mcp-conformance_uze-conformance",
    "ToolName": "uze_conformance",
    "Arguments": {},
}
#: A tool no request declares: the provider passes over it and answers
#: with text, so the turn makes no call at all.
NO_CALL = [{"name": "uze_ui_no_such_tool", "args": {}}]
SLEEP = "sleep 2; cat >/dev/null; printf '{}'"
COUNT = f"cat >/dev/null; echo x >> {SPAWNS}; printf '{{}}'"


def _read(path):
    return {"name": "read_file", "args": {"Path": path}}


def _shell(command):
    return {
        "name": "run_command",
        "args": {
            "CommandLine": command,
            "WaitMsBeforeAsync": 2000,
            "toolSummary": "Lab command",
        },
    }


def _group(command, matcher="*"):
    return [{"matcher": matcher, "hooks": [{"type": "command", "command": command}]}]


def census_hooks():
    return {
        "census": {
            e: (
                [{"type": "command", "command": f"{LOGGER} {e}"}]
                if e in FLAT
                else _group(f"{LOGGER} {e}")
            )
            for e in EVENTS
        }
    }


LOGGER_SCRIPT = f"""#!/bin/sh
event=$1
{{ printf '%s ' "$event"; cat; echo; }} >> {HOOK_LOG}
case "$event" in
  PreToolUse) printf '{{"decision":"allow"}}' ;;
  *) printf '{{}}' ;;
esac
"""


def _prelude(hooks):
    return f"""
mkdir -p $(dirname {SKILL_FILE}) $(dirname {AGENT_FILE}) {HOME}/config
cat > {SKILL_FILE} <<'EOF'
---
name: {SKILL}
description: Use this skill when the user asks for the probe report. {SKILL_DESC}
---

Write the probe report. {SKILL_BODY}
EOF
cat > {AGENT_FILE} <<'EOF'
---
name: {AGENT}
description: Answers probe requests.
---

Answer in one line. {AGENT_BODY}
EOF
cat > {LOGGER} <<'EOF'
{LOGGER_SCRIPT}
EOF
chmod +x {LOGGER}
cat > {GLOBAL_HOOKS} <<'EOF'
{json.dumps(hooks, indent=1)}
EOF
"""


def _final(prompt, flags=""):
    # decision: experiment-isolation
    return f"""
cd /work && mkdir -p project && cd project
echo notes > {NOTES}
start=$(date +%s%N)
agy --print {json.dumps(prompt)} {flags} --output-format stream-json \\
  --dangerously-skip-permissions --print-timeout 150s --log-file /work/agy.log > /work/stream.out 2>&1 || true
end=$(date +%s%N)
echo "===== elapsed_ms $(( (end - start) / 1000000 ))"
echo '===== hook log ====='
cat {HOOK_LOG} 2>/dev/null | head -c 60000
echo
echo '===== transcripts ====='
for t in {HOME}/antigravity-cli/brain/*/.system_generated/logs/*.jsonl; do
  echo "== $t"; head -c 20000 "$t"; echo
done
echo '===== markers ====='
ls /work/hook-markers 2>/dev/null || true
echo '===== spawns ====='
cat {SPAWNS} 2>/dev/null | wc -l || true
echo '===== stream ====='
tail -c 3000 /work/stream.out
echo '===== end ====='
"""


def _section(out, name):
    marker = f"===== {name} ====="
    if marker not in out:
        return ""
    return out.split(marker, 1)[1].split("\n===== ", 1)[0]


def _elapsed(out):
    for line in out.splitlines():
        if line.startswith("===== elapsed_ms "):
            return int(line.split()[-1])
    return -1


def _events(hooks_text):
    """(event, payload) per logged line."""
    found = []
    for line in hooks_text.splitlines():
        event, _, rest = line.partition(" ")
        if event not in EVENTS:
            continue
        try:
            found.append((event, json.loads(rest)))
        except ValueError:
            found.append((event, {"unparsed": rest[:200]}))
    return found


def _call(payload):
    return payload.get("toolCall") or {}


def _world(cfg, prov_ip, name, case):
    env = {
        "TOOL_SEQUENCE": json.dumps(case.get("sequence", NO_CALL)),
        "FINAL_TEXT": "UZE_UI_DONE",
        "DISCOVERY": "1",
    }
    if case.get("subagent"):
        env["SUBAGENT_SEQUENCE"] = json.dumps(case["subagent"])
    common.start_provider(cfg, "toolcall", env)
    setup = agy_setup(
        cfg,
        prov_ip,
        include_mcp=True,
        final_cmd=_final(case["prompt"], case.get("flags", "")),
        plugins=case.get("plugins", "flow mcp-plugin"),
        prelude=_prelude(case.get("hooks", census_hooks())),
    )
    r = subprocess.run(
        common.docker_base(cfg, prov_ip, setup, tty=False),
        capture_output=True,
        text=True,
        errors="replace",
        timeout=500,
    )
    out = r.stdout
    with open(f"{cfg.outdir}/case_{name}.out", "w") as f:
        f.write(out)
    if r.stderr:
        with open(f"{cfg.outdir}/case_{name}.err", "w") as f:
            f.write(r.stderr)
    requests = ""
    common.pull_captures(cfg)
    log = f"{cfg.outdir}/raw-requests.log"
    if os.path.exists(log):
        with open(log, errors="replace") as f:
            requests = f.read()
        os.rename(log, f"{cfg.outdir}/case_{name}.requests.log")
    print(f"--- case {name}: {_elapsed(out)} ms", flush=True)
    for event, payload in _events(_section(out, "hook log")):
        call = _call(payload)
        print(
            json.dumps(
                {
                    "event": event,
                    "conv": (payload.get("conversationId") or "")[:8],
                    "lastUserInput": (payload.get("lastUserInput") or "")[:100],
                    "tool": call.get("name"),
                    "args": json.dumps(call.get("args"))[:140] if call else None,
                }
            ),
            flush=True,
        )
    return out, requests


#: A file in the workspace: a read of one under `~/.gemini` is refused by
#: the harness's protection boundary, and a refused call fires no PostToolUse.
NOTES = "/work/project/notes.txt"
SIX_READS = [_read(NOTES) for _ in range(6)]


def _perf(hooks):
    return {"prompt": "read six UZE_UI_TURN", "sequence": SIX_READS, "hooks": hooks}


CASES = {
    # 1. the person invokes a skill with the `/` menu's syntax
    "slash-skill": {"prompt": f"/{SKILL} write it UZE_UI_TURN"},
    # 2. a plugin's user-invocable-only skill, and an agent from the prompt
    "plugin-skill": {"prompt": "/flow:review now UZE_UI_TURN"},
    # the command later in the prompt, and two in one prompt
    "slash-mid": {"prompt": f"please run /{SKILL} and /flow:review UZE_UI_TURN"},
    "agent-mention": {"prompt": f"@{AGENT} answer the probe UZE_UI_TURN"},
    "agent-flag": {
        "prompt": "answer the probe UZE_UI_TURN",
        "flags": f"--agent {AGENT}",
    },
    # 3. the skill read through other spellings of its path
    "paths": {
        "prompt": "write the probe report UZE_UI_TURN",
        "sequence": [
            _read(f"~/.gemini/antigravity-cli/skills/{SKILL}/SKILL.md"),
            _read(f"../home/.gemini/antigravity-cli/skills/{SKILL}/SKILL.md"),
            _shell(f"cat {SKILL_FILE}"),
        ],
    },
    # 4. a subagent reads the skill and calls the MCP tool
    "subagent": {
        "prompt": "write the probe report UZE_UI_TURN",
        "sequence": [
            {
                "name": "invoke_subagent",
                "args": {
                    "TypeName": AGENT,
                    "Role": "Lab Probe",
                    "Prompt": "Read the skill and report.",
                },
            }
        ],
        # A subagent is offered `view_file` (`AbsolutePath`), not `read_file`.
        "subagent": [
            {"name": "view_file", "args": {"AbsolutePath": SKILL_FILE}},
            {"name": "call_mcp_tool", "args": MCP_ARGS},
        ],
    },
    # 5. cost of a hook on six read_file calls
    "perf-none": _perf({}),
    "perf-matcher": _perf(
        {"usage": {"PostToolUse": _group(COUNT, "invoke_subagent|call_mcp_tool")}}
    ),
    "perf-logger": _perf({"usage": {"PostToolUse": _group(COUNT)}}),
    "perf-sleep-post": _perf({"usage": {"PostToolUse": _group(SLEEP)}}),
    "perf-sleep-preinvocation": _perf(
        {"usage": {"PreInvocation": [{"type": "command", "command": SLEEP}]}}
    ),
    # coexistence: two named global groups beside a plugin's own hooks
    "coexist": {
        "prompt": "write the probe report UZE_UI_TURN",
        "sequence": [{"name": "call_mcp_tool", "args": MCP_ARGS}],
        "plugins": "flow mcp-plugin hook-rows",
        "hooks": {
            "uze-usage": {
                "PostToolUse": _group(f"{LOGGER} PostToolUse", "call_mcp_tool")
            },
            "someone-else": {"PostToolUse": _group(COUNT)},
        },
    },
}


def run(cfg, prov_ip):
    chosen = [c for c in os.environ.get("UI_CASES", "").split(",") if c]
    results = {}
    for name, case in CASES.items():
        if not chosen or name in chosen:
            results[name] = _world(cfg, prov_ip, name, case)

    def hooks(name):
        return _section(results[name][0], "hook log")

    def transcripts(name):
        return _section(results[name][0], "transcripts")

    if "slash-skill" in results:
        requests = results["slash-skill"][1]
        common.check(
            "slash-skill-body-reaches-the-model-without-a-tool",
            SKILL_BODY in requests,
            "the skill's body is in a request although no tool ran",
        )
        common.check(
            "slash-skill-raw-in-last-user-input",
            any(
                p.get("lastUserInput", "").startswith(f"/{SKILL}")
                for _, p in _events(hooks("slash-skill"))
            ),
            "lastUserInput keeps the slash command as typed",
        )
        common.check(
            "slash-skill-named-in-the-transcript",
            f"/{SKILL}" in transcripts("slash-skill"),
            "the transcript names the command",
        )
    if "slash-mid" in results:
        common.check(
            "a-command-later-in-the-prompt-is-plain-text",
            "[Slash Command]" not in transcripts("slash-mid"),
            "only a leading /<skill> is expanded",
        )
    if "plugin-skill" in results:
        common.check(
            "plugin-skill-body-reaches-the-model",
            "flow:review" in results["plugin-skill"][1]
            or "explicitly user-invoked" in results["plugin-skill"][1],
            "the plugin skill reached a request",
        )
    if "agent-mention" in results:
        common.check(
            "an-agent-mention-is-plain-text",
            AGENT_BODY not in results["agent-mention"][1]
            and "invoke_subagent" not in hooks("agent-mention"),
            "@<agent> neither dispatches nor injects the agent",
        )
    if "agent-flag" in results:
        common.check(
            "agent-flag-runs-the-agent-unnamed",
            AGENT_BODY in results["agent-flag"][1]
            and AGENT not in hooks("agent-flag")
            and AGENT not in transcripts("agent-flag"),
            "--agent becomes the system prompt; no hook or transcript names it",
        )
    if "paths" in results:
        events = _events(hooks("paths"))
        relative = [
            (e, p)
            for e, p in events
            if _call(p).get("name") == "read_file"
            and not json.dumps(_call(p).get("args")).startswith('{"Path": "/')
        ]
        common.check(
            "a-relative-read-reaches-only-pre-tool-use",
            {e for e, _ in relative} == {"PreToolUse"},
            "~ and ../ reads are refused (must be absolute): PreToolUse only",
        )
        common.check(
            "shell-cat-reaches-the-hook",
            any(
                _call(p).get("name") == "run_command"
                and SKILL_FILE in json.dumps(_call(p).get("args"))
                for e, p in events
                if e == "PostToolUse"
            ),
            "run_command naming the SKILL.md reached PostToolUse",
        )
    if "subagent" in results:
        events = _events(hooks("subagent"))
        parents = {
            p.get("conversationId")
            for _, p in events
            if _call(p).get("name") == "invoke_subagent"
        }
        for tool in ("view_file", "call_mcp_tool"):
            common.check(
                f"subagent-{tool}-reaches-the-global-hook",
                any(
                    e == "PostToolUse"
                    and _call(p).get("name") == tool
                    and p.get("conversationId") not in parents
                    for e, p in events
                ),
                f"PostToolUse {tool} under the subagent's own conversationId",
            )
    if "perf-matcher" in results:
        spawns = _section(results["perf-matcher"][0], "spawns").strip()
        common.check(
            "a-matcher-spawns-nothing-for-other-tools",
            spawns == "0",
            f"{spawns} spawns of the non-matching group on six read_file calls",
        )
    if "perf-logger" in results:
        spawns = _section(results["perf-logger"][0], "spawns").strip()
        common.check(
            "a-catch-all-spawns-once-per-call",
            spawns == "6",
            f"{spawns} spawns of the * group on six read_file calls",
        )
    timings = {n: _elapsed(results[n][0]) for n in results if n.startswith("perf-")}
    if timings:
        with open(f"{cfg.outdir}/timings.json", "w") as f:
            json.dump(timings, f, indent=1)
        print("timings", timings, flush=True)
    if "coexist" in results:
        out = results["coexist"][0]
        common.check(
            "named-global-group-runs-beside-another",
            "call_mcp_tool" in hooks("coexist")
            and _section(out, "spawns").strip() not in ("", "0"),
            "uze-usage logged the MCP call and someone-else ran too",
        )
        common.check(
            "plugin-hooks-still-run",
            "census" in _section(out, "markers"),
            "hook-rows' census recorded beside the global groups",
        )
