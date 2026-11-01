"""Exploration: what does Codex tell a hook, and keep in its rollout, when a
person invokes a skill in the prompt — and what does each hook event cost?

Finding (2026-10-31, codex-cli 0.166.0, code mode):

- A skill the person names (`$probe-skill`, typed or taken from the `$`
  popup, which inserts `$<name> `; a plugin's as `$flow:analyze`) reaches
  `UserPromptSubmit` only as the raw text, in `prompt`: no hook payload
  carries the `<skill><name>…</name><path>…</path>` item Codex injects, and
  the rollout keeps that item as a user `message`. `codex exec` and the TUI
  behave alike.
- Skills are offered as `- <name>: <desc> (file: rN/<dir>/SKILL.md)` with a
  per-request alias map (`r0` = `~/.agents/skills`, `r1` =
  `~/.codex/skills/.system`, `rN` = `~/.codex/plugins/cache/<market>/
  <plugin>/<ver>/skills`). Codex does not rewrite an alias: `cat
  r0/probe-skill/SKILL.md` reaches `PreToolUse`/`PostToolUse` as typed
  (`tool_name` `Bash`, `tool_input.command`) and fails in the shell (`No
  such file`, in `tool_response`). A model that reads the skill resolves the
  alias itself, so a detector matches `…/<dir>/SKILL.md` and judges success
  by `tool_response`; a plugin skill's own name (`flow:commit`) is not in
  its path, only its plugin directory is.
- `PostToolUse` carries `tool_name`, `tool_input`, `tool_response`,
  `tool_use_id`, `turn_id`: `Bash` + `command` for the shell,
  `collaborationspawn_agent` + `agent_type` for a dispatch, and
  `mcp__<server>__<tool>` (input `{}`, the result in `tool_response`) for an
  MCP tool called inside `exec`. `SubagentStart`/`SubagentStop` name the
  agent in `agent_type` too.
- Cost: a matcher that does not match spawns nothing (0 calls; 20685 ms
  against 20612 ms with no hook). A sync handler fires once per nested
  shell command and the turn waits for it (10 × 1 s sleep: +10 s); a
  trivial `sh` handler costs about 5 ms a call (+41 and +57 ms over 10).
  `"async": true` does not block (20673 ms) but is lossy: 8 of 10 calls
  logged, the rest cancelled at session end or past the 8-concurrent cap.

Phases (select with `USER_INVOCATION_PHASES=offer,use,tui,perf`; default
all), each its own container and provider. A plain logger is declared in the
person's own `~/.codex/hooks.json` on every event Codex 0.166 knows, trusted
in `config.toml` the way a review records it (`[hooks.state."<key>"]`), so no
UZE hook is in the loop: what is measured is the harness.

- `offer` (headless): the prompt mentions `$probe-skill` and
  `$flow:analyze`; the turn is answered with text. What the hooks are handed,
  what the request offers (`--discovery`), what the rollout keeps.
- `use` (headless): the model reads the skills' `SKILL.md` exactly as
  offered (the root alias left as it is), then resolved; dispatches a global
  agent; calls the MCP fixture's tool inside `exec`.
- `tui` (interactive): the person types `$probe` and takes the popup's
  completion with Enter, the way a person picks a skill, then sends.
- `perf` (headless): one `exec` cell running ten shell commands, under
  `PostToolUse` handlers that sleep a second: none, a matcher that never
  matches, a matched sync one, a matched async one.

Run: python3 conformance/lab.py --harness codex --experiment codex/user-invocation --discovery
"""

import hashlib
import json
import os
import re
import shlex
import subprocess
import time

from contract import hooks as hook_contract
from contract.bindings import hook_prelude
from contract.tui import Tui
from harnesses.codex.bindings import CodexBindings
from harnesses.codex.scenarios import codex_container
from shared import common

PROJECT = "/work/project"
SKILL = "probe-skill"
SKILL_FILE = f"/work/home/.agents/skills/{SKILL}/SKILL.md"
AGENT = "probe_agent"
AGENT_FILE = f"/work/home/.codex/agents/{AGENT}.toml"
PLUGIN_SKILL = "flow:analyze"
#: A skill Codex reads out of the plugin it installed (`flow:analyze` is
#: model-only, so UZE attaches it to `~/.agents/skills` instead).
PLUGIN_READ = "flow:commit"
TRIGGER = hook_contract.PROMPT
PROMPT = f"${SKILL} ${PLUGIN_SKILL} {TRIGGER}"
USER_HOOKS = "/work/home/.codex/hooks.json"
LOGGER = "/work/hooklog.sh"
HOOK_LOG = "/work/hooklog"
MCP_NATIVE = "mcp__uze_conformance__uze_conformance"

#: Every event codex-cli 0.166.0 names (its binary and its hooks page),
#: PascalCase as `hooks.json` keys them, snake_case as its trust keys do.
EVENTS = {
    "SessionStart": "session_start",
    "SessionEnd": "session_end",
    "SubagentStart": "subagent_start",
    "SubagentStop": "subagent_stop",
    "UserPromptSubmit": "user_prompt_submit",
    "PreToolUse": "pre_tool_use",
    "PermissionRequest": "permission_request",
    "PostToolUse": "post_tool_use",
    "PreCompact": "pre_compact",
    "PostCompact": "post_compact",
    "Stop": "stop",
    "Interrupt": "interrupt",
}
#: `SessionEnd` and `Interrupt` cap their timeout at 3 seconds.
TIMEOUT = 3

LOGGER_SCRIPT = f"""#!/bin/sh
# One line per call: the event, the time it started, the payload.
printf '@@HOOK %s %s ' "$1" "$(date +%s%N)" >> {HOOK_LOG}
tr -d '\\n' >> {HOOK_LOG}
echo >> {HOOK_LOG}
[ -n "$2" ] && sleep "$2"
exit 0
"""


def identity(event, command, timeout, matcher=None, asynchronous=False):
    """Codex's trust hash of one command handler (`version_for_toml` over
    the normalized group), as `experiments/codex/launch-hook` measured."""
    group = {
        "event_name": event,
        "hooks": [
            {
                "async": asynchronous,
                "command": command,
                "timeout": timeout,
                "type": "command",
            }
        ],
    }
    if matcher is not None:
        group["matcher"] = matcher
    canonical = json.dumps(
        group, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    )
    return "sha256:" + hashlib.sha256(canonical.encode()).hexdigest()


def handlers(groups):
    """`groups`: `{Event: (command, matcher, async)}`. Returns the
    `hooks.json` text and the `config.toml` trust tables for them."""
    declared, trust = {}, []
    for event, (command, matcher, asynchronous) in groups.items():
        handler = {"type": "command", "command": command, "timeout": TIMEOUT}
        if asynchronous:
            handler["async"] = True
        group = {"hooks": [handler]}
        if matcher is not None:
            group["matcher"] = matcher
        declared[event] = [group]
        snake = EVENTS[event]
        key = f"{USER_HOOKS}:{snake}:0:0"
        digest = identity(snake, command, TIMEOUT, matcher, asynchronous)
        trust.append(
            f"[hooks.state.{json.dumps(key)}]\ntrusted_hash = {json.dumps(digest)}\n"
        )
    return json.dumps({"hooks": declared}), "\n".join(trust)


def every_event():
    return {event: (f"{LOGGER} {event}", None, False) for event in EVENTS}


def world(groups):
    """The person's skill, agent and hooks, written after UZE installed the
    plugins (`codex_setup`), so the trust tables follow UZE's."""
    declared, trust = handlers(groups)
    return f"""
{hook_prelude(PROJECT)}
mkdir -p $(dirname {SKILL_FILE}) $(dirname {AGENT_FILE})
cat > {SKILL_FILE} <<'EOF'
---
name: {SKILL}
description: Use this skill when the user asks for the probe report. UZE_UI_DESC_4b7e
---

Write the probe report. UZE_UI_BODY_91c3
EOF
cat > {AGENT_FILE} <<'EOF'
name = "{AGENT}"
description = "Answers the probe."
developer_instructions = "Answer the probe in one word."
EOF
cat > {LOGGER} <<'UZE_LOGGER_EOF'
{LOGGER_SCRIPT}UZE_LOGGER_EOF
chmod +x {LOGGER}
printf '%s' {shlex.quote(declared)} > {USER_HOOKS}
printf '\\n%s' {shlex.quote(trust)} >> /work/home/.codex/config.toml
"""


DUMP = f"""
echo '@@SECTION hooks'; cat {HOOK_LOG} 2>/dev/null
echo '@@SECTION rollout'; cat /work/home/.codex/sessions/*/*/*/rollout-*.jsonl 2>/dev/null
echo '@@SECTION plugin-skills'; find /work/home/.codex -name SKILL.md 2>/dev/null
echo '@@SECTION end'
"""


def sections(output):
    found, name = {}, None
    for line in output.splitlines():
        if line.startswith("@@SECTION "):
            name = line.split(" ", 1)[1].strip()
            found[name] = []
        elif name:
            found[name].append(line)
    return {k: "\n".join(v).strip() for k, v in found.items()}


def hook_calls(text):
    """`[(event, started_ns, payload)]` from the logger's lines."""
    calls = []
    for line in text.splitlines():
        match = re.match(r"@@HOOK (\S+) (\d+) (.*)$", line)
        if not match:
            continue
        try:
            payload = json.loads(match.group(3))
        except ValueError:
            payload = {"unparsed": match.group(3)[:400]}
        calls.append((match.group(1), int(match.group(2)), payload))
    return calls


def rollout_entries(text):
    entries = []
    for line in text.splitlines():
        try:
            entries.append(json.loads(line))
        except ValueError:
            continue
    return entries


def headless(cfg, prov_ip, phase, groups, prompt=PROMPT):
    launch = f"""
cd {PROJECT}
set +e
start=$(date +%s%N)
timeout 240 codex exec {shlex.quote(prompt)} > /work/exec.out 2>&1
echo "@@EXIT $? $(( ($(date +%s%N) - start) / 1000000 ))"
tail -c 1500 /work/exec.out
"""
    final = world(groups) + launch + DUMP
    out = subprocess.run(
        codex_container(cfg, prov_ip, final, plugins="flow mcp-plugin", tty=False),
        capture_output=True,
        text=True,
        errors="replace",
        timeout=420,
    ).stdout
    with open(os.path.join(cfg.outdir, f"user-invocation-{phase}.txt"), "w") as f:
        f.write(out)
    match = re.search(r"@@EXIT (\d+) (\d+)", out)
    elapsed = int(match.group(2)) if match else None
    return out, sections(out), elapsed


def provider_requests(cfg):
    """The current provider's raw requests (`--discovery`), read in place."""
    r = subprocess.run(
        ["docker", "exec", cfg.prov_name, "cat", "/app/raw-requests.log"],
        capture_output=True,
        text=True,
        errors="replace",
    )
    return r.stdout


def summarize(calls):
    """Per event, the payload's keys and the fields that name a use."""
    rows = []
    for event, _, payload in calls:
        tool_input = payload.get("tool_input")
        rows.append(
            {
                "event": event,
                "keys": sorted(payload),
                "tool_name": payload.get("tool_name"),
                "tool_input": json.dumps(tool_input)[:300] if tool_input else None,
                "prompt": (payload.get("prompt") or "")[:300] or None,
                "agent_type": payload.get("agent_type"),
            }
        )
    return rows


def phase_offer(cfg):
    prov_ip = common.start_provider(cfg, "static")
    out, found, elapsed = headless(cfg, prov_ip, "offer", every_event())
    requests = provider_requests(cfg)
    calls = hook_calls(found.get("hooks", ""))
    rows = summarize(calls)
    offered = re.findall(r"- [^\n\\]*\(file: [^)]*\)", requests.replace("\\n", "\n"))
    aliases = sorted(
        set(re.findall(r"[^\n\\]{0,160}\br\d+\b[^\n\\]{0,160}", requests))
    )[:12]
    rollout = rollout_entries(found.get("rollout", ""))
    injected = [json.dumps(e)[:600] for e in rollout if "<skill>" in json.dumps(e)]
    report = {
        "elapsed_ms": elapsed,
        "hook_events": rows,
        "offered": offered,
        "alias_lines": aliases,
        "rollout_skill_items": injected,
        "codex_plugin_skills": found.get("plugin-skills", ""),
    }
    with open(os.path.join(cfg.outdir, "user-invocation-offer.json"), "w") as f:
        json.dump(report, f, indent=1)
    print(json.dumps(report, indent=1)[:9000], flush=True)
    prompts = [r["prompt"] for r in rows if r["event"] == "UserPromptSubmit"]
    common.check(
        "offer-hooks-ran",
        bool(calls),
        f"{len(calls)} hook calls: {sorted({r['event'] for r in rows})}",
    )
    common.check(
        "offer-user-prompt-submit-has-the-mention",
        any(p and f"${SKILL}" in p for p in prompts),
        f"UserPromptSubmit.prompt = {prompts[:1]}",
    )
    common.check(
        "offer-plugin-mention-in-prompt",
        any(p and f"${PLUGIN_SKILL}" in p for p in prompts),
        "the plugin skill's mention, raw",
    )
    common.check(
        "offer-no-hook-carries-the-injected-skill",
        not any("<skill>" in json.dumps(p) for _, _, p in calls),
        "no hook payload carries the `<skill>` item Codex injects",
    )
    common.check(
        "offer-rollout-records-explicit-skill",
        any(f"<name>{SKILL}</name>" in i for i in injected),
        f"{len(injected)} `<skill>` items in the rollout",
    )
    common.check(
        "offer-rollout-records-plugin-skill",
        any("analyze" in i for i in injected),
        "the plugin skill's `<skill>` item",
    )
    return offered


def offered_path(offered, name):
    for line in offered:
        if re.match(rf"- {re.escape(name)}:", line):
            return re.search(r"\(file: ([^)]*)\)", line).group(1)
    return None


def _read(path):
    script = (
        f"const result = await tools.exec_command({{cmd: {json.dumps('cat ' + path)}}});\n"
        'text(typeof result === "string" ? result : JSON.stringify(result));'
    )
    return {"tool": "functions.exec", "args": script}


def phase_use(cfg, offered):
    bindings = CodexBindings()
    alias = offered_path(offered, SKILL) or f"r0/{SKILL}/SKILL.md"
    plugin = offered_path(offered, PLUGIN_READ)
    reads = [_read(alias), _read(SKILL_FILE)]
    if plugin:
        reads.append(_read(plugin))
    calls = [
        *reads,
        {
            "tool": "collaboration.spawn_agent",
            "args": {
                "task_name": "lab_usage",
                "message": "Answer the probe.",
                "agent_type": AGENT,
                "fork_turns": "none",
            },
        },
        {"tool": "collaboration.wait_agent", "args": {"timeout_ms": 30000}},
        *bindings.mcp_calls(),
    ]
    mode, env = bindings.sequence(calls, TRIGGER)
    prov_ip = common.start_provider(cfg, mode, env)
    out, found, elapsed = headless(cfg, prov_ip, "use", every_event(), prompt=TRIGGER)
    calls_seen = hook_calls(found.get("hooks", ""))
    rows = summarize(calls_seen)
    rollout = rollout_entries(found.get("rollout", ""))
    rollout_calls = []
    for e in rollout:
        p = e.get("payload") or {}
        if p.get("type") in (
            "function_call",
            "custom_tool_call",
            "function_call_output",
            "custom_tool_call_output",
        ):
            rollout_calls.append(
                {
                    "type": p.get("type"),
                    "name": p.get("name"),
                    "namespace": p.get("namespace"),
                    "carried": str(
                        p.get("arguments") or p.get("input") or p.get("output") or ""
                    )[:300],
                }
            )
    report = {
        "elapsed_ms": elapsed,
        "alias_path": alias,
        "plugin_path": plugin,
        "hook_events": rows,
        "rollout_calls": rollout_calls,
        "exec_tail": out.split("@@EXIT", 1)[-1][:1500],
    }
    with open(os.path.join(cfg.outdir, "user-invocation-use.json"), "w") as f:
        json.dump(report, f, indent=1)
    print(json.dumps(report, indent=1)[:12000], flush=True)

    def post(pattern):
        return [
            r
            for r in rows
            if r["event"] == "PostToolUse" and re.search(pattern, json.dumps(r))
        ]

    common.check(
        "use-post-tool-use-sees-the-alias-read",
        bool(post(re.escape(alias))),
        f"PostToolUse names `{alias}`",
    )
    common.check(
        "use-alias-is-not-rewritten",
        "No such file" in json.dumps(post(re.escape(alias))),
        "Codex hands the shell the alias as typed; resolving it is the model's job",
    )
    common.check(
        "use-post-tool-use-sees-the-absolute-read",
        bool(post(re.escape(SKILL_FILE))),
        f"PostToolUse names `{SKILL_FILE}`",
    )
    common.check(
        "use-post-tool-use-sees-the-plugin-read",
        bool(plugin) and bool(post(re.escape(plugin))),
        f"PostToolUse names `{plugin}`",
    )
    common.check(
        "use-post-tool-use-sees-spawn-agent",
        bool(post(rf"spawn_agent.*{AGENT}")),
        "PostToolUse for spawn_agent with agent_type",
    )
    common.check(
        "use-subagent-start-names-the-agent",
        any(r["event"] == "SubagentStart" and r["agent_type"] == AGENT for r in rows),
        "SubagentStart.agent_type",
    )
    common.check(
        "use-post-tool-use-sees-the-mcp-tool",
        any(r["event"] == "PostToolUse" and r["tool_name"] == MCP_NATIVE for r in rows),
        f"PostToolUse.tool_name = {MCP_NATIVE}",
    )


def phase_tui(cfg):
    bindings = CodexBindings()
    prov_ip = common.start_provider(cfg, "static")
    final = world(every_event()) + f"\ncd {PROJECT} && exec codex"
    with Tui(
        cfg,
        codex_container(cfg, prov_ip, final, plugins="flow mcp-plugin"),
        "codex-user-invocation",
    ) as tui:
        plain, ready = bindings.prepare(tui)
        common.check("tui-ready", ready, plain[-160:].replace("\n", " "))
        if not ready:
            return
        time.sleep(bindings.warmup)
        tui.type("$probe")
        time.sleep(1.5)
        popup = tui.collect(reads=4)
        tui.snapshot("tui-popup", popup)
        tui.submit()
        time.sleep(1.0)
        composer = tui.collect(reads=4)
        tui.snapshot("tui-composer", composer)
        tui.type(f" {TRIGGER}")
        time.sleep(0.5)
        tui.submit()
        time.sleep(8)
        tui.collect(reads=6)
        log = subprocess.run(
            [
                "docker",
                "exec",
                cfg.harness_container,
                "sh",
                "-c",
                f"cat {HOOK_LOG}; echo @@SECTION rollout; cat /work/home/.codex/sessions/*/*/*/rollout-*.jsonl",
            ],
            capture_output=True,
            text=True,
            errors="replace",
        ).stdout
    hooks_text, _, rollout_text = log.partition("@@SECTION rollout")
    rows = summarize(hook_calls(hooks_text))
    injected = [
        json.dumps(e)[:600]
        for e in rollout_entries(rollout_text)
        if "<skill>" in json.dumps(e)
    ]
    prompts = [r["prompt"] for r in rows if r["event"] == "UserPromptSubmit"]
    report = {
        "popup_tail": popup[-1200:],
        "composer_tail": composer[-800:],
        "prompts": prompts,
        "rollout_skill_items": injected,
    }
    with open(os.path.join(cfg.outdir, "user-invocation-tui.json"), "w") as f:
        json.dump(report, f, indent=1)
    print(json.dumps(report, indent=1)[:5000], flush=True)
    common.check(
        "tui-popup-offers-the-skill",
        SKILL in popup,
        "typing `$probe` lists the skill",
    )
    common.check(
        "tui-user-prompt-submit-has-the-mention",
        any(p and f"${SKILL}" in p for p in prompts),
        f"UserPromptSubmit.prompt = {prompts[:1]}",
    )
    common.check(
        "tui-rollout-records-explicit-skill",
        any(f"<name>{SKILL}</name>" in i for i in injected),
        f"{len(injected)} `<skill>` items",
    )


PERF_COMMANDS = 10


def phase_perf(cfg):
    bindings = CodexBindings()
    script = (
        "const out = [];\n"
        + "".join(
            f'out.push(JSON.stringify(await tools.exec_command({{cmd: "true #{i}"}})));\n'
            for i in range(PERF_COMMANDS)
        )
        + 'text("ran " + out.length);'
    )
    sleeper = f"{LOGGER} Post 1"
    variants = {
        "none": {},
        "unmatched": {"PostToolUse": (sleeper, "^NoSuchTool$", False)},
        "matched-sync": {"PostToolUse": (sleeper, None, False)},
        "matched-async": {"PostToolUse": (sleeper, None, True)},
        "matched-trivial": {"PostToolUse": (f"{LOGGER} Post", None, False)},
    }
    chosen = os.environ.get("USER_INVOCATION_PERF")
    if chosen:
        variants = {k: v for k, v in variants.items() if k in chosen.split(",")}
    timings = {}
    for name, groups in variants.items():
        mode, env = bindings.sequence(
            [{"tool": "functions.exec", "args": script}], TRIGGER
        )
        prov_ip = common.start_provider(cfg, mode, env)
        out, found, elapsed = headless(
            cfg, prov_ip, f"perf-{name}", groups, prompt=TRIGGER
        )
        spawned = len(hook_calls(found.get("hooks", "")))
        timings[name] = {
            "elapsed_ms": elapsed,
            "hook_calls": spawned,
            "ran": f"ran {PERF_COMMANDS}" in out,
        }
    with open(os.path.join(cfg.outdir, "user-invocation-perf.json"), "w") as f:
        json.dump(timings, f, indent=1)
    print(json.dumps(timings, indent=1), flush=True)
    if "matched-trivial" in timings and "none" in timings:
        cost = (timings["matched-trivial"]["elapsed_ms"] or 0) - (
            timings["none"]["elapsed_ms"] or 0
        )
        common.check(
            "perf-trivial-handler-costs-milliseconds",
            timings["matched-trivial"]["hook_calls"] >= PERF_COMMANDS and cost < 1000,
            f"{cost} ms over {timings['matched-trivial']['hook_calls']} calls",
        )
    if not {"none", "unmatched", "matched-sync", "matched-async"} <= set(timings):
        return
    base = timings["none"]["elapsed_ms"] or 0
    common.check(
        "perf-every-variant-ran",
        all(t["ran"] for t in timings.values()),
        json.dumps({k: v["ran"] for k, v in timings.items()}),
    )
    common.check(
        "perf-unmatched-matcher-spawns-nothing",
        timings["unmatched"]["hook_calls"] == 0,
        f"{timings['unmatched']['hook_calls']} spawns, {timings['unmatched']['elapsed_ms']} ms vs {base} ms",
    )
    common.check(
        "perf-sync-handler-fires-per-nested-command",
        timings["matched-sync"]["hook_calls"] >= PERF_COMMANDS,
        f"{timings['matched-sync']['hook_calls']} calls, {timings['matched-sync']['elapsed_ms']} ms",
    )
    common.check(
        "perf-async-handler-does-not-block",
        (timings["matched-async"]["elapsed_ms"] or 1e9)
        < (timings["matched-sync"]["elapsed_ms"] or 0) - 5000,
        f"async {timings['matched-async']['elapsed_ms']} ms ({timings['matched-async']['hook_calls']} calls) vs sync {timings['matched-sync']['elapsed_ms']} ms",
    )


def run(cfg, prov_ip):
    phases = os.environ.get("USER_INVOCATION_PHASES", "offer,use,tui,perf").split(",")
    offered = []
    if "offer" in phases or "use" in phases:
        offered = phase_offer(cfg)
    if "use" in phases:
        phase_use(cfg, offered)
    if "tui" in phases:
        phase_tui(cfg)
    if "perf" in phases:
        phase_perf(cfg)
