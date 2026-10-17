"""Can a launch hand Codex an agent-context document, invisibly?

See `experiments.agent_context_probe` for the four questions. The channel
is a `SessionStart` hook declared for this process only, on the session
layer (`-c hooks.SessionStart=[...]`), and trusted on that layer
(`-c hooks.state={...}`) with Codex's identity hash of the handler, as
`codex/trust.rs` computes it and `experiments/codex/launch-hook` proved.
The handler is POSIX `sh`: it keeps its stdin as evidence and prints the
file `UZE_AGENT_CONTEXT` names, plain, on stdout.

Two headless processes in one world: `codex exec --json` with marker A in
the document, then `codex exec resume <thread> ...` with the same flags
after the document was rewritten to marker B.

Env: AGENT_CONTEXT_TRUST=0 drops the trust override (does exec run an
untrusted hook?).

Run: python3 conformance/lab.py --harness codex --experiment codex/agent-context
"""

import os
import shlex
import subprocess
import time

from contract.tui import Tui
from experiments import agent_context_probe as probe
from harnesses.codex.bindings import CodexBindings
from shared import common

PROJECT = "/work/project"
HOOK = "/work/uze/agent-context.sh"
DOC = "/work/uze/agent-context.md"
SESSION_KEY = "/<session-flags>/config.toml:session_start:0:0"
TRUST = os.environ.get("AGENT_CONTEXT_TRUST", "1") == "1"
#: Document sizes to run, one world each: the small one, and one past
#: Codex's 10,000-byte (2,500 tokens x 4) spill limit for hook output, the
#: marker on its last line so a cut or a spill shows.
SIZES = [
    int(n) for n in os.environ.get("AGENT_CONTEXT_BYTES", "0,12000").split(",") if n
]

HOOK_SCRIPT = r"""#!/bin/sh
mkdir -p /work/evidence
cat >> /work/evidence/hook-stdin.log
echo >> /work/evidence/hook-stdin.log
[ -n "$UZE_AGENT_CONTEXT" ] && cat "$UZE_AGENT_CONTEXT"
exit 0
"""


def identity(event, command, timeout):
    """Codex's trust hash of one command handler (`version_for_toml`)."""
    import hashlib
    import json

    group = {
        "event_name": event,
        "hooks": [
            {"async": False, "command": command, "timeout": timeout, "type": "command"}
        ],
    }
    canonical = json.dumps(group, sort_keys=True, separators=(",", ":"))
    return "sha256:" + hashlib.sha256(canonical.encode()).hexdigest()


def injection():
    import json

    args = [
        "-c",
        f'hooks.SessionStart=[{{hooks=[{{type="command",command={json.dumps(HOOK)},timeout=10}}]}}]',
    ]
    if TRUST:
        digest = identity("session_start", HOOK, 10)
        args += [
            "-c",
            f"hooks.state={{{json.dumps(SESSION_KEY)}={{trusted_hash={json.dumps(digest)}}}}}",
        ]
    return " ".join(shlex.quote(a) for a in args)


def script(size, tui=False):
    flags = injection()
    pad = (
        f"node -e 'process.stdout.write((\"lab padding line\\n\").repeat({size // 17}))' > /work/pad.txt"
        if size
        else ": > /work/pad.txt"
    )
    return f"""
set -e
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin
export HOME=/work/home CODEX_HOME=/work/home/.codex
export OPENAI_API_KEY=uze-conformance-invalid-by-design
export CODEX_CA_CERTIFICATES=/app/ca.crt SSL_CERT_FILE=/app/ca.crt
mkdir -p /work/home/.codex /work/uze {PROJECT}
cp /app/fixtures/auth.json /work/home/.codex/auth.json
# decision: the container is the sandbox (as scenarios.codex_setup)
printf 'sandbox_mode = "danger-full-access"\\n' > /work/home/.codex/config.toml
cat > {HOOK} <<'UZE_HOOK_EOF'
{HOOK_SCRIPT}UZE_HOOK_EOF
chmod +x {HOOK}
git -C {PROJECT} init -q && printf 'lab project\\n' > {PROJECT}/README.md
{pad}
{probe.write_document(DOC, probe.MARKER_A, "one")}
cat /work/pad.txt {DOC} > {DOC}.full && mv {DOC}.full {DOC}
echo '=== size-one'
wc -c < {DOC}
touch {DOC}.size
export UZE_AGENT_CONTEXT={DOC}
cd {PROJECT}
set +e
{"exec codex " + flags if tui else ""}
echo '=== one'
timeout 200 codex exec --json {flags} {probe.TURN_ONE} > /work/one.jsonl 2>&1
echo "exit $?"; cat /work/one.jsonl
thread=$(sed -n 's/.*"thread_id" *: *"\\([^"]*\\)".*/\\1/p' /work/one.jsonl | head -n 1)
echo '=== thread'
echo "$thread"
{probe.write_document(DOC, probe.MARKER_B, "two")}
cat /work/pad.txt {DOC} > {DOC}.full && mv {DOC}.full {DOC}
echo '=== two'
timeout 200 codex exec --json {flags} resume "$thread" {probe.TURN_TWO} 2>&1
echo "exit $?"
echo '=== hook-stdin'
cat /work/evidence/hook-stdin.log 2>&1
{probe.transcript_report("/work/home/.codex/sessions/*/*/*/rollout-*.jsonl")}
echo '=== spill'
find /work/home/.codex -newer {DOC}.size -type f 2>/dev/null | grep -v -e sessions -e log -e sqlite | head
grep -rl {probe.MARKER_B} /work/home/.codex /tmp 2>/dev/null | head
echo '=== config'
cat /work/home/.codex/config.toml
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


def world(size):
    def run_world(cfg):
        prov_ip = common.start_provider(cfg, "static", {"DISCOVERY": "1"})
        time.sleep(1)
        cmd = common.docker_base(cfg, prov_ip, script(size), tty=False)
        ca_crt, _, _ = common.generate_certs(cfg)
        i = cmd.index(common.HARNESS_IMAGE)
        cmd = cmd[:i] + ["-v", f"{ca_crt}:/app/ca.crt:ro"] + cmd[i:]
        proc = subprocess.run(
            cmd, capture_output=True, text=True, errors="replace", timeout=600
        )
        return {"out": proc.stdout + proc.stderr, "log": probe.provider_log(cfg)}

    return run_world


#: An interactive session with the same injection, to read the pane for the
#: marker (and for a hook review the session trust should prevent).
#: `AGENT_CONTEXT_TUI=0` skips it.
TUI = os.environ.get("AGENT_CONTEXT_TUI", "1") == "1"


def tui_world(cfg):
    prov_ip = common.start_provider(cfg, "static", {"DISCOVERY": "1"})
    cmd = common.docker_base(cfg, prov_ip, script(0, tui=True), tty=True)
    ca_crt, _, _ = common.generate_certs(cfg)
    i = cmd.index(common.HARNESS_IMAGE)
    cmd = cmd[:i] + ["-v", f"{ca_crt}:/app/ca.crt:ro"] + cmd[i:]
    with Tui(cfg, cmd, "agent-context-tui") as tui:
        bindings = CodexBindings()
        _, ready = bindings.prepare(tui)
        turn = bindings.hook_turn(tui, f"{probe.TURN_ONE} hello")
        tui.collect(reads=4)
        probe.say(f"[tui] ready={ready}; turn: {turn.detail}")
        record, shown = tui.transcript(), tui.shown()
    return {
        "record": record,
        "shown": shown,
        "ready": ready,
        "log": probe.provider_log(cfg),
    }


def judge_tui(cfg, r):
    with open(os.path.join(cfg.outdir, "tui-record.txt"), "w") as f:
        f.write(r["record"])
    with open(os.path.join(cfg.outdir, "tui-shown.txt"), "w") as f:
        f.write(r["shown"])
    with open(os.path.join(cfg.outdir, "codex-tui-raw-requests.log"), "w") as f:
        f.write(r["log"])
    text = r["record"] + r["shown"]
    answered = "UZE_CONFORMANCE_OK" in text
    reached = probe.MARKER_A in r["log"]
    squashed = text.replace(" ", "").lower()
    hook_lines = [
        line.strip()[:160]
        for line in r["record"].splitlines()
        if "hook" in line.lower()
    ][:6]
    common.check_absence(
        "codex-tui-2b-not-rendered",
        probe.MARKER_A not in text,
        answered,
        proof=answered and reached,
        detail=f"turn answered={answered}; marker reached the provider={reached}; "
        f"marker on the pane: {probe.MARKER_A in text}; hook review shown: "
        f"{'trustallandcontinue' in squashed}; lines naming a hook: {hook_lines}",
    )


def run(cfg, prov_ip):
    if TUI:
        judge_tui(cfg, common.concurrently(cfg, {"tui": tui_world})["tui"])
    for size in SIZES:
        tag = "codex-sessionstart" + ("" if TRUST else "-untrusted")
        tag += f"-{size}b" if size else ""
        result = common.concurrently(cfg, {tag: world(size)})[tag]
        out = result["out"]
        with open(os.path.join(cfg.outdir, f"{tag}.out"), "w") as f:
            f.write(out)
        parts = sections(out)
        probe.say(f"[{tag}] document bytes: {parts.get('size-one', '').strip()}")
        probe.say(f"[{tag}] thread: {parts.get('thread', '').strip()}")
        probe.say(f"[{tag}] hook stdin: {parts.get('hook-stdin', '')[:500]!r}")
        probe.say(f"[{tag}] turn one tail: {parts.get('one', '')[-300:]!r}")
        probe.say(f"[{tag}] turn two tail: {parts.get('two', '')[-300:]!r}")
        probe.say(f"[{tag}] spill: {parts.get('spill', '')!r}")
        for line in parts.get("transcript", "").splitlines():
            probe.say(f"[{tag}]   transcript {line[:300]}")
        summary = probe.analyse(cfg, tag, result["log"])
        probe.say(f"[{tag}] {summary}")
