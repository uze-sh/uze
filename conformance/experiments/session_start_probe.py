"""Experiment: does a portable `SessionStart` hook run once when a session opens?

The fixture `hook-session-plugin` declares an observational `SessionStart`
group (no matcher, so every portable source) beside an observational
`PreToolUse` one; both run `scripts/opened`, which appends one line per run
to `/work/session-hooks.log`: `<harness>|<event>|<source>|<tool>`. The line
count is the evidence — a hook that fires on every turn, or never, is told
apart from one that fires once only by counting.

One headless session per harness, through the harness's own non-interactive
entry point, on a provider that answers with plain text (no tool is called,
so no `pre_tool_use` line is expected). What it proves, per harness:

- the delivered configuration carries a `SessionStart` entry (read off the
  harness's own file, not UZE's report);
- the handler ran exactly once with `HOOK_EVENT=session_start` and
  `HOOK_SOURCE=startup`, and with no tool;
- the session still answered.

Run:
  python3 conformance/lab.py --harness claude --experiment claude/session-start
  python3 conformance/lab.py --harness codex --experiment codex/session-start
"""

import importlib
import time

import pexpect

from shared import common

LOG = "/work/session-hooks.log"
BEGIN, END = "=== SESSION-START-EVIDENCE ===", "=== SESSION-START-END ==="

#: How each harness opens one headless session, where it keeps the hook
#: configuration UZE merged into, and how a session reads when it answered.
HARNESSES = {
    "claude": {
        "container": "harnesses.claude.scenarios:claude_container",
        "session": "claude -p 'say hello' --output-format text",
        "config": "/work/home/.claude/settings.json",
    },
    "codex": {
        "container": "harnesses.codex.scenarios:codex_container",
        # Codex runs no hook from ~/.codex/hooks.json until it is reviewed;
        # the Lab vets exactly its own fixture, as the hooks phase does.
        "session": "codex exec --dangerously-bypass-hook-trust "
        "--skip-git-repo-check 'say hello'",
        "config": "/work/home/.codex/hooks.json",
    },
}


def container_for(harness):
    module, name = HARNESSES[harness]["container"].split(":")
    return getattr(importlib.import_module(module), name)


def evidence_command(harness):
    spec = HARNESSES[harness]
    return f"""
cd /work
echo '{BEGIN}'
echo '--- config'
cat {spec["config"]} 2>&1 || true
echo '--- session'
{spec["session"]} </dev/null 2>&1 | tail -20 || true
echo '--- log'
cat {LOG} 2>/dev/null || true
echo '{END}'
"""


def section(text, name):
    """The lines between `--- <name>` and the next marker."""
    lines, inside = [], False
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("--- ") or stripped == END:
            inside = stripped == f"--- {name}"
            continue
        if inside:
            lines.append(stripped)
    return lines


def run(cfg, prov_ip):
    harness = cfg.harness
    common.start_provider(cfg, "static")
    time.sleep(1)
    cmd = container_for(harness)(
        cfg, prov_ip, evidence_command(harness), plugins="hook-session-plugin"
    )
    child = pexpect.spawn(
        cmd[0], cmd[1:], encoding="utf-8", codec_errors="replace", timeout=240
    )
    child.setwinsize(50, 200)
    child.expect([END, pexpect.EOF, pexpect.TIMEOUT])
    text = common.ansi_strip(child.before or "")
    child.close(force=True)
    text = text[text.find(BEGIN) :] if BEGIN in text else text
    with open(f"{cfg.outdir}/session_start.raw", "w") as f:
        f.write(text)

    config = "\n".join(section(text, "config"))
    session = section(text, "session")
    log = [line for line in section(text, "log") if line]
    started = [line for line in log if line.split("|")[1:2] == ["session_start"]]

    common.check(
        "session-start-entry-delivered",
        '"SessionStart"' in config and "session_start" in config,
        "the harness's own hook configuration carries a SessionStart entry "
        "running the wrapper",
    )
    common.check(
        "session-start-session-answered",
        any(line for line in session),
        f"the headless session answered: {' / '.join(session[-3:])!r}",
    )
    common.check(
        "session-start-ran-once",
        len(started) == 1,
        f"{len(started)} session_start run(s) for one new session; log: {log!r}",
    )
    common.check(
        "session-start-source-startup",
        started == [f"{harness}|session_start|startup|"],
        f"handler saw harness|event|source|tool = {started!r}",
    )
