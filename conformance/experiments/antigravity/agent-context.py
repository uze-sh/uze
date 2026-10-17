"""Can a launch hand Antigravity an agent-context document, invisibly?

See `experiments.agent_context_probe` for the four questions. The channel
is a `PreInvocation` hook in `<launch dir>/.agents/hooks.json`, loaded for
this process alone through `--add-dir <launch dir>` (as
`experiments/antigravity/launch-hook` proved). The handler (`node`: the
image has no Python or jq) keeps its stdin as evidence and answers with the
document in one of the two step shapes the vendor's own `hooks.json` guide
(embedded in the binary) documents for `PreInvocation`:

  AGENT_CONTEXT_STEP=ephemeral  {"injectSteps":[{"ephemeralMessage": doc}]}
                                ("transient message dropped after the
                                current turn")
  AGENT_CONTEXT_STEP=system     {"injectSteps":[{"systemMessage":
                                {"systemMessage": doc}}]} ("persists in the
                                conversation trajectory across turns")

Default runs both, each its own world. Each world is signed in (the
synthetic `consumer` identity: hooks never ran on an API key,
google-antigravity/antigravity-cli#893) and runs two headless processes:
`agy --print` with marker A, then `agy --conversation <id> --print` with
the same `--add-dir` after the document was rewritten to marker B.

Run: python3 conformance/lab.py --harness antigravity --experiment antigravity/agent-context
"""

import json
import os
import subprocess
import time

import pexpect

from experiments import agent_context_probe as probe
from harnesses.antigravity.scenarios import agy_setup, answer_first_run
from shared import common

STEPS = os.environ.get("AGENT_CONTEXT_STEP", "ephemeral,system").split(",")
#: The first turn makes one tool call, so it is two model invocations: a
#: hook that persists what it injects on every invocation would leave two
#: copies behind. `AGENT_CONTEXT_TOOLCALL=0` makes it one invocation.
TOOLCALL = os.environ.get("AGENT_CONTEXT_TOOLCALL", "1") == "1"
SEQUENCE = [
    {
        "name": "run_command",
        "args": {
            "CommandLine": "echo lab",
            "Cwd": "/work/project",
            "WaitMsBeforeAsync": 2000,
            "toolSummary": "Command execution",
            "toolAction": "Running command",
        },
    }
]
LAUNCH = "/work/uze-launch"
DOC = f"{LAUNCH}/agent-context.md"

HOOK_JS = r"""const fs = require("fs");
let input = "";
try { input = fs.readFileSync(0, "utf8"); } catch (e) {}
fs.appendFileSync("/work/hook-stdin.log", input.replace(/\n/g, " ") + "\n");
const doc = fs.readFileSync(process.env.UZE_AGENT_CONTEXT || "__DOC__", "utf8");
const step = "__STEP__" === "system" ? { systemMessage: { systemMessage: doc } } : { ephemeralMessage: doc };
process.stdout.write(JSON.stringify({ injectSteps: [step] }));
"""

HOOKS_JSON = (
    '{"uze-agent-context":{"PreInvocation":[{"type":"command",'
    f'"command":"node {LAUNCH}/ctx.js","timeout":10}}]}}}}'
)


#: An interactive session per step shape, to read the pane for the marker.
#: `AGENT_CONTEXT_TUI=0` skips it.
TUI = os.environ.get("AGENT_CONTEXT_TUI", "1") == "1"


def tui_launch(step):
    """The same world as `final`, ending in the interactive TUI."""
    head = final(step).split("echo '=== one'", 1)[0]
    return head + f"exec agy --add-dir {LAUNCH} --log-file /work/agy-tui.log\n"


def tui_job(step):
    def run_world(world):
        prov_ip = common.start_provider(world, "static", {"DISCOVERY": "1"})
        setup = agy_setup(world, prov_ip, include_mcp=False, final_cmd=tui_launch(step))
        cmd = common.docker_base(world, prov_ip, setup)
        child = pexpect.spawn(
            cmd[0], cmd[1:], encoding="utf-8", codec_errors="replace", timeout=300
        )
        child.setwinsize(50, 160)
        child.logfile_read = common.CastRecorder(
            world.outdir, f"agent-context-tui-{step}"
        )
        screen = common.make_screen(child)
        first = answer_first_run(child, screen) or ""
        for ch in f"{probe.TURN_ONE} hello":
            child.send(ch)
            time.sleep(0.08)
        child.send("\r")
        seen_raw = ""
        for _ in range(20):
            t, _ = screen(2.0)
            seen_raw += t
            if (
                "UZE_CONFORMANCE_OK" in common.render_screen(seen_raw)
                or "UZE_CONFORMANCE_OK" in t
            ):
                break
        time.sleep(2)
        t, _ = screen(2.0)
        seen_raw += t
        log = probe.provider_log(world)
        child.close(force=True)
        subprocess.run(
            ["docker", "rm", "-f", world.harness_container], capture_output=True
        )
        with open(
            os.path.join(world.outdir, f"agent-context-tui-{step}.typescript"),
            errors="replace",
        ) as f:
            record = common.ansi_strip(f.read())
        return {
            "first": first,
            "record": record,
            "shown": common.render_screen(seen_raw),
            "log": log,
        }

    return run_world


def judge_tui(cfg, step, r):
    with open(os.path.join(cfg.outdir, f"tui-{step}-shown.txt"), "w") as f:
        f.write(r["shown"])
    with open(
        os.path.join(cfg.outdir, f"antigravity-tui-{step}-raw-requests.log"), "w"
    ) as f:
        f.write(r["log"])
    text = r["record"] + r["shown"]
    answered = "UZE_CONFORMANCE_OK" in text
    reached = probe.MARKER_A in r["log"]
    lines = [
        ln.strip()[:200]
        for ln in text.splitlines()
        if "Agent context" in ln or "lab task" in ln
    ][:6]
    common.check_absence(
        f"antigravity-tui-{step}-2b-not-rendered",
        probe.MARKER_A not in text and not lines,
        answered,
        proof=answered and reached,
        detail=f"turn answered={answered}; marker reached the provider={reached}; "
        f"marker on the pane: {probe.MARKER_A in text}; document lines on the pane: {lines}",
    )


def final(step):
    flags = (
        # decision: headless-permissions
        f"--add-dir {LAUNCH} --output-format json --dangerously-skip-permissions "
        "--print-timeout 90s"
    )
    return f"""
set +e
mkdir -p /work/project {LAUNCH}/.agents
git -C /work/project init -q
cat > {LAUNCH}/ctx.js <<'UZE_EOF'
{HOOK_JS.replace("__DOC__", DOC).replace("__STEP__", step)}UZE_EOF
printf '%s' '{HOOKS_JSON}' > {LAUNCH}/.agents/hooks.json
{probe.write_document(DOC, probe.MARKER_A, "one")}
export UZE_AGENT_CONTEXT={DOC}
cd /work/project
echo '=== one'
timeout 150 agy {flags} --log-file /work/agy-one.log --print {probe.TURN_ONE} > /work/one.json 2>&1
echo "exit $?"; tail -c 2500 /work/one.json
conv=$(sed -n 's/.*"conversation_\\{{0,1\\}}[iI]d" *: *"\\([^"]*\\)".*/\\1/p' /work/one.json | head -n 1)
[ -n "$conv" ] || conv=$(sed -n 's/.*"conversationId" *: *"\\([^"]*\\)".*/\\1/p' /work/hook-stdin.log | head -n 1)
echo '=== conversation'
echo "$conv"
{probe.write_document(DOC, probe.MARKER_B, "two")}
echo '=== two'
timeout 150 agy {flags} --log-file /work/agy-two.log --conversation "$conv" --print {probe.TURN_TWO} 2>&1 | tail -c 2500
echo "exit $?"
echo '=== hook-stdin'
cat /work/hook-stdin.log 2>&1 | cut -c1-700
echo '=== workspace-paths'
grep -o '"workspacePaths":[^]]*]' /work/hook-stdin.log | sort | uniq -c
{probe.transcript_report("/work/home/.gemini/antigravity-cli/brain/*/.system_generated/logs/*.jsonl")}
echo '=== hooks-log'
grep -h -i 'hook' /work/agy-one.log /work/agy-two.log 2>/dev/null | grep -v Migration | head -30
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


def job(step):
    def run_world(world):
        if TOOLCALL:
            prov_ip = common.start_provider(
                world,
                "toolcall",
                {
                    "DISCOVERY": "1",
                    "TOOL_SEQUENCE": json.dumps(SEQUENCE),
                    "FINAL_TEXT": "UZE_CONFORMANCE_OK",
                },
            )
        else:
            prov_ip = common.start_provider(world, "static", {"DISCOVERY": "1"})
        time.sleep(1)
        setup = agy_setup(world, prov_ip, include_mcp=False, final_cmd=final(step))
        proc = subprocess.run(
            common.docker_base(world, prov_ip, setup, tty=False),
            capture_output=True,
            text=True,
            errors="replace",
            timeout=600,
        )
        return {"out": proc.stdout + proc.stderr, "log": probe.provider_log(world)}

    return run_world


def run(cfg, prov_ip):
    if TUI:
        for step in STEPS:
            judge_tui(
                cfg,
                step,
                common.concurrently(cfg, {f"tui-{step}": tui_job(step)})[f"tui-{step}"],
            )
    results = {}
    # One at a time: two signed-in worlds at once have contended before.
    for step in STEPS:
        results.update(common.concurrently(cfg, {f"ctx-{step}": job(step)}))
    for tag, result in results.items():
        with open(os.path.join(cfg.outdir, f"{tag}.out"), "w") as f:
            f.write(result["out"])
        parts = sections(result["out"])
        probe.say(f"[{tag}] conversation: {parts.get('conversation', '').strip()}")
        probe.say(f"[{tag}] one: {parts.get('one', '')[-600:]!r}")
        probe.say(f"[{tag}] two: {parts.get('two', '')[-600:]!r}")
        probe.say(f"[{tag}] hook stdin: {parts.get('hook-stdin', '')[:900]!r}")
        probe.say(f"[{tag}] hooks log: {parts.get('hooks-log', '')[:400]!r}")
        probe.say(
            f"[{tag}] workspacePaths per hook call: {parts.get('workspace-paths', '')!r}"
        )
        for line in parts.get("transcript", "").splitlines():
            probe.say(f"[{tag}]   transcript {line[:300]}")
        probe.analyse(cfg, f"antigravity-preinvocation-{tag[4:]}", result["log"])
