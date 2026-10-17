"""Can a launch hand Claude Code an agent-context document, invisibly?

See `experiments.agent_context_probe` for the four questions. Two channels,
each its own world (container + provider), each two headless processes —
`claude -p` with marker A in the document, then `claude -p --resume <id>`
after the document was rewritten to marker B:

- `adddir`: the document is `<dir>/CLAUDE.md` in a directory outside the
  project, handed with `--add-dir <dir>` and
  `CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD=1`;
- `hook`: a `SessionStart` hook in a `--settings` file outside `~/.claude`,
  printing the file `UZE_AGENT_CONTEXT` names as
  `hookSpecificOutput.additionalContext` (built with `node`: the image has
  no Python or jq).

Both pass the flags again on the resumed process, as a launcher would, and
`CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD=1` is set on every launch.
Each channel runs in a project with an `AGENTS.md` and in one without
(`-noagents`), since Claude reads the project's `AGENTS.md` natively and the
two instruction files might be merged into one item.

Run: python3 conformance/lab.py --harness claude --experiment claude/agent-context
Env: AGENT_CONTEXT_CHANNELS=adddir,hook
"""

import json
import os
import shlex
import subprocess
import time

from contract.tui import Tui
from experiments import agent_context_probe as probe
from harnesses.claude.bindings import ClaudeBindings
from harnesses.claude.scenarios import claude_container, generate_certs
from shared import common

CHANNELS = [
    c for c in os.environ.get("AGENT_CONTEXT_CHANNELS", "adddir,hook").split(",") if c
]
#: One interactive session with both channels at once — the added
#: directory's CLAUDE.md carrying marker A, the hook's document marker B —
#: to read the pane for either. `AGENT_CONTEXT_TUI=0` skips it.
TUI = os.environ.get("AGENT_CONTEXT_TUI", "1") == "1"
PROJECT = "/work/project"
LAUNCH = "/work/uze-launch"
CONTEXT_DIR = f"{LAUNCH}/context"
HOOK_DOC = f"{LAUNCH}/agent-context.md"

HOOK_JS = r"""const fs = require("fs");
const path = process.env.UZE_AGENT_CONTEXT;
let input = "";
try { input = fs.readFileSync(0, "utf8"); } catch (e) {}
fs.appendFileSync("/work/hook-stdin.log", input + "\n");
if (!path) process.exit(0);
const text = fs.readFileSync(path, "utf8");
process.stdout.write(JSON.stringify({ hookSpecificOutput: { hookEventName: "SessionStart", additionalContext: text } }));
"""

SETTINGS = {
    "hooks": {
        "SessionStart": [
            {"hooks": [{"type": "command", "command": f"node {LAUNCH}/ctx.js"}]}
        ]
    }
}


def channel_setup(channel):
    if channel == "adddir":
        doc = f"{CONTEXT_DIR}/CLAUDE.md"
        prep = f"mkdir -p {CONTEXT_DIR}"
        launch = f"claude --add-dir {CONTEXT_DIR}"
    else:
        doc = HOOK_DOC
        prep = (
            f"mkdir -p {LAUNCH}\ncat > {LAUNCH}/ctx.js <<'UZE_EOF'\n{HOOK_JS}UZE_EOF\n"
            f"printf '%s' {shlex.quote(json.dumps(SETTINGS))} > {LAUNCH}/settings.json"
        )
        launch = (
            f"env UZE_AGENT_CONTEXT={HOOK_DOC} claude --settings {LAUNCH}/settings.json"
        )
    return doc, prep, launch


def full_script(channel, agents_md):
    doc, prep, launch = channel_setup(channel)
    project_md = (
        f"printf '%s\\n' '# Project' 'Lab project instructions.' > {PROJECT}/AGENTS.md"
        if agents_md
        else ""
    )
    # decision: headless-permissions
    flags = "--permission-mode bypassPermissions --output-format json"
    return f"""
set -e
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin
export HOME=/work/home CLAUDE_CONFIG_DIR=/work/home/.claude
export ANTHROPIC_API_KEY=uze-conformance-invalid-by-design
export NODE_EXTRA_CA_CERTS=/app/ca.crt
# Set on every launch, whichever channel carries the document.
export CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD=1
mkdir -p /work/home/.claude {PROJECT}
cp /app/fixtures/claude.json /work/home/.claude.json
git -C {PROJECT} init -q && printf 'lab project\\n' > {PROJECT}/README.md
{project_md}
{prep}
{probe.write_document(doc, probe.MARKER_A, "one")}
set +e
cd {PROJECT}
echo '=== one'
timeout 150 {launch} -p {probe.TURN_ONE} {flags} > /work/one.json 2>&1
cat /work/one.json
sid=$(sed -n 's/.*"session_id" *: *"\\([^"]*\\)".*/\\1/p' /work/one.json | head -n 1)
echo '=== sid'
echo "$sid"
{probe.write_document(doc, probe.MARKER_B, "two")}
echo '=== two'
timeout 150 {launch} -p {probe.TURN_TWO} --resume "$sid" {flags} 2>&1
echo '=== hook-stdin'
cat /work/hook-stdin.log 2>&1
{probe.transcript_report("/work/home/.claude/projects/*/*.jsonl")}
"""


def sections(text):
    out, name = {}, "pre"
    for line in text.splitlines():
        if line.startswith("=== "):
            name = line[4:].strip()
            out[name] = []
        else:
            out.setdefault(name, []).append(line)
    return {k: "\n".join(v) for k, v in out.items()}


def job(channel, agents_md):
    def run_world(world):
        prov_ip = common.start_provider(world, "static", {"DISCOVERY": "1"})
        time.sleep(1)
        cmd = common.docker_base(
            world, prov_ip, full_script(channel, agents_md), tty=False
        )
        ca_crt, _, _ = generate_certs(world)
        i = cmd.index(common.HARNESS_IMAGE)
        cmd = cmd[:i] + ["-v", f"{ca_crt}:/app/ca.crt:ro"] + cmd[i:]
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=500)
        return {"out": proc.stdout + proc.stderr, "log": probe.provider_log(world)}

    return run_world


def tui_job(world):
    prov_ip = common.start_provider(world, "static", {"DISCOVERY": "1"})
    _, adddir_prep, _ = channel_setup("adddir")
    _, hook_prep, _ = channel_setup("hook")
    final = f"""
{adddir_prep}
{hook_prep}
{probe.write_document(f"{CONTEXT_DIR}/CLAUDE.md", probe.MARKER_A, "one")}
{probe.write_document(HOOK_DOC, probe.MARKER_B, "two")}
mkdir -p {PROJECT} && git -C {PROJECT} init -q
cd {PROJECT} && exec env CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD=1 UZE_AGENT_CONTEXT={HOOK_DOC} \\
  claude --add-dir {CONTEXT_DIR} --settings {LAUNCH}/settings.json
"""
    bindings = ClaudeBindings()
    tui = Tui(
        world, claude_container(world, prov_ip, final, plugins=""), "agent-context-tui"
    )
    with tui:
        onboarding, ready = bindings.prepare(tui)
        if ready:
            time.sleep(bindings.warmup)
            tui.type(f"{probe.TURN_ONE} hello")
            tui.submit()
            tui.collect(reads=8)
        record = tui.transcript()
        shown = tui.shown()
    return {
        "ready": ready,
        "onboarding": onboarding,
        "record": record,
        "shown": shown,
        "log": probe.provider_log(world),
    }


def judge_tui(cfg, r):
    with open(os.path.join(cfg.outdir, "tui-record.txt"), "w") as f:
        f.write(r["record"])
    with open(os.path.join(cfg.outdir, "tui-shown.txt"), "w") as f:
        f.write(r["shown"])
    with open(os.path.join(cfg.outdir, "claude-tui-raw-requests.log"), "w") as f:
        f.write(r["log"])
    answered = "UZE_CONFORMANCE_OK" in r["record"] or "UZE_CONFORMANCE_OK" in r["shown"]
    reached = probe.MARKER_A in r["log"] and probe.MARKER_B in r["log"]
    on_pane = [
        m for m in (probe.MARKER_A, probe.MARKER_B) if m in r["record"] + r["shown"]
    ]
    hook_lines = [
        line.strip()
        for line in (r["record"]).splitlines()
        if "SessionStart" in line or "hook" in line.lower()
    ][:6]
    common.check_absence(
        "claude-tui-2b-not-rendered",
        not on_pane,
        answered,
        proof=answered and reached,
        detail=f"ready={r['ready']}; turn answered={answered}; both markers reached the "
        f"provider={reached}; markers anywhere in the terminal record: {on_pane}; "
        f"lines naming a hook: {hook_lines}",
    )


def run(cfg, prov_ip):
    generate_certs(cfg)
    if TUI:
        judge_tui(cfg, common.concurrently(cfg, {"ctx-tui": tui_job})["ctx-tui"])
    worlds = {}
    for c in CHANNELS:
        worlds[f"ctx-{c}"] = job(c, True)
        worlds[f"ctx-{c}-noagents"] = job(c, False)
    results = common.concurrently(cfg, worlds)
    for tag, result in results.items():
        with open(os.path.join(cfg.outdir, f"{tag}.out"), "w") as f:
            f.write(result["out"])
        parts = sections(result["out"])
        probe.say(f"[{tag}] session: {parts.get('sid', '').strip()}")
        probe.say(f"[{tag}] transcript entries carrying a marker:")
        for line in parts.get("transcript", "").splitlines():
            probe.say(f"[{tag}]   {line[:300]}")
        probe.say(f"[{tag}] hook stdin: {parts.get('hook-stdin', '')[:400]!r}")
        probe.analyse(cfg, f"claude-{tag[4:]}", result["log"])
