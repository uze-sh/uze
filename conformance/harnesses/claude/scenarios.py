#!/usr/bin/env python3
import json

"""Claude Code scenario (latest channel) — Real Harness + Synthetic World.

Phase A (TUI): onboarding drive -> prompt; /plugin, /mcp (server connected,
1 tool), deterministic turn, model-visible skill present and user-only skill
ABSENT from the PRIMARY model request (disable-model-invocation — genuine
policy preservation), MCP registration + connection via /mcp.

Preflight: TLS interception of the hardcoded Anthropic hosts via /etc/hosts
+ injected CA (NODE_EXTRA_CA_CERTS). `ANTHROPIC_BASE_URL` is ignored by the
interactive TUI — the TLS interception is the required hook.
"""
import os
import sys
import time

import pexpect

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", ".."))
import shared.common as common
from shared.common import (
    check,
    describe,
    docker_base,
    generate_certs,
    make_screen,
    make_waiter,
    materialize_marketplace,
    provider_struct,
)


def claude_setup(cfg, prov_ip, final_cmd, plugins="flow mcp-plugin"):
    return f"""
set -e
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin
export HOME=/work/home CLAUDE_CONFIG_DIR=/work/home/.claude UZE_HOME=/work/home/.uze
export ANTHROPIC_API_KEY=uze-conformance-invalid-by-design
export ANTHROPIC_BASE_URL=https://api.anthropic.com
export NODE_EXTRA_CA_CERTS=/app/ca.crt
mkdir -p /work/home/.claude
cp /app/fixtures/claude.json /work/home/.claude.json
{materialize_marketplace(cfg)}
uze market add /work/market >/dev/null 2>&1
for p in {plugins}; do uze install $p@uze-lab -m >/dev/null 2>&1; done
{final_cmd}
"""


def claude_container(cfg, prov_ip, final_cmd, plugins="flow mcp-plugin", tty=True):
    cmd = docker_base(
        cfg, prov_ip, claude_setup(cfg, prov_ip, final_cmd, plugins=plugins), tty=tty
    )
    ca_crt, _, _ = generate_certs(cfg)
    i = cmd.index(common.HARNESS_IMAGE)
    cmd = (
        cmd[:i]
        + [
            "-v",
            f"{ca_crt}:/app/ca.crt:ro",
            "-e",
            "CLAUDE_CONFIG_DIR=/work/home/.claude",
        ]
        + cmd[i:]
    )
    return cmd


def drive_onboarding(child):
    """Dialogs: Welcome/Security guide (2.1.250 first-run) -> API key (Yes)
    -> theme -> security notes -> trust (Yes) -> Tips/What's-new popup ->
    prompt. Returns (screen, plain, marker).

    Every match here is space-insensitive (`squash`): 2.1.260 paints the
    first-run screens with cursor-forward moves instead of spaces, so a
    marker spelled the way a person reads it ("Yes, I trust this folder")
    is absent from the transcript while being plainly on screen. Matching
    on the spelled form let the trust screen fall through to the generic
    "❯" match, which returned as if the prompt were up — and the next
    Enter landed on "No, exit" and quit the TUI, taking all 24 checks with
    it."""
    screen = make_screen(child)
    wait_for = make_waiter(screen)
    DIALOGS = [
        "Welcome to Claude Code",
        "Security guide",
        "Yes, I trust this folder",
        "Detected a custom API key",
        "theme",
        "Security notes",
        "Quick safety check",
        "Accessing workspace",
        "login method",
        "Opus5",
        "Opus",
        "API Usage Billing",
        "❯",
    ]

    def prompt_is_up(t, p):
        j = common.squash(p)
        return bool(t) and ("Opus" in j or "APIUsageBilling" in j)

    t, p, m = wait_for(DIALOGS, tries=16, stop_on_death=True, squash_spaces=True)
    for i in range(10):
        j = common.squash(p)
        if m == "Detected a custom API key":
            child.send("\x1b[A")
            time.sleep(0.3)
            child.send("\r")  # Yes
        elif m in ("theme", "Security notes"):
            child.send("\r")
        elif m in (
            "Welcome to Claude Code",
            "Security guide",
            "Quick safety check",
            "Accessing workspace",
        ):
            # 2.1.250 first-run: the folder-trust screen defaults to
            # "❯ No, exit" — plain Enter would quit the TUI. Select
            # "Yes, I trust this folder" first.
            if "No,exit" in j and "Yes,Itrust" in j:
                child.send("\x1b[B")
                time.sleep(0.3)
            child.send("\r")
        elif "Yes,Itrustthisfolder" in j and "No,exit" in j:
            # 2.1.250: keep "Yes" selected (down from "No, exit") before
            # confirming, and re-confirm if the guide stays up.
            if "❯No,exit" in j:
                child.send("\x1b[B")
                time.sleep(0.3)
            child.send("\r")
        elif m == "login method":
            child.send("\x1b[B")
            time.sleep(0.3)
            child.send("\r")
        else:
            break
        t, p, m = wait_for(DIALOGS, tries=14, stop_on_death=True, squash_spaces=True)
        if m in ("Opus5", "Opus", "API Usage Billing", "❯"):
            break
        # The 2.1.250 prompt frame carries the model/status bar content;
        # "❯" alone is invalid here (it appears on every option screen).
        if prompt_is_up(t, p):
            break
    # The Tips/What's-new overlay popup does not block the prompt; the first
    # slash-command keystroke dismisses it. Never send Esc here (Esc on an
    # empty prompt exits the TUI). 2.1.250 labels the model "Opus 5 (1M
    # context)" and paints the prompt once — the last good frame may carry
    # it while the matched marker was an earlier dialog, so return on
    # CONTENT (Opus/status bar/prompt arrow), never on a stale or empty
    # read.
    for _ in range(10):
        if prompt_is_up(t, p):
            return t, p, m
        t, p, m = wait_for(
            ["Opus5", "Opus", "API Usage Billing", "❯"],
            tries=4,
            gap=2.0,
            stop_on_death=True,
            squash_spaces=True,
        )
    return t, p, m


def phase_tui(cfg, prov_ip):
    cmd = claude_container(cfg, prov_ip, "exec claude")
    child = pexpect.spawn(
        cmd[0], cmd[1:], encoding="utf-8", codec_errors="replace", timeout=300
    )
    child.setwinsize(50, 160)
    try:
        child.logfile_read = common.CastRecorder(cfg.outdir, "tui")
    except Exception:
        pass
    screen = make_screen(child)
    wait_for = make_waiter(screen)

    def snap(tag, t):
        with open(f"{cfg.outdir}/{tag}.raw", "w") as f:
            f.write(t)

    t, p, m = drive_onboarding(child)
    # 2.1.250 renders the prompt in spaced bursts; a race can hand the
    # phase an empty first frame even though the prompt is up. Re-wait for
    # a real prompt frame before snapshotting/asserting.
    if not t or ("Opus" not in p and "API Usage Billing" not in p):
        t, p, m = wait_for(
            ["Opus", "API Usage Billing", "❯"], tries=8, gap=2.0, stop_on_death=True
        )
        if not t:
            t, p, m = wait_for(["❯"], tries=4, gap=2.0, stop_on_death=True)
    snap("01_prompt", t)
    joined = p.replace(" ", "")
    check(
        "tui-reached-prompt",
        ("Opus" in joined and "❯" in p) or ("APIUsageBilling" in joined and "❯" in p),
        "claude TUI reached its prompt"
        if "Opus" in joined or "APIUsageBilling" in joined
        else p[-120:].replace("\n", " "),
    )

    # /plugin
    for ch in "/plugin":
        child.send(ch)
        time.sleep(0.06)
    child.send("\r")
    t, p, m = wait_for(["Installed", "Plugins"], tries=8, stop_on_death=True)
    snap("02_plugin", t)
    check(
        "plugin-surface-in-tui",
        "Plugins" in p and "Installed" in p,
        "/plugin opens the plugins surface",
    )
    child.send("\x1b")
    time.sleep(1.0)

    # No TUI check for agents: since 2.1.283 `/agents` (and `claude agents`)
    # manage *background* agents, and the subagent roster is no longer a
    # screen of its own. What a person relied on it for — Claude offers the
    # delivered agent under its label and runs it — is asserted where it
    # happens, on the wire, by `contract.agent` (exposure and dispatch).

    # /mcp
    for ch in "/mcp":
        child.send(ch)
        time.sleep(0.06)
    child.send("\r")
    t, p, m = wait_for(["connected", "tool"], tries=10, stop_on_death=True)
    snap("02b_mcp", t)
    joined = p.replace(" ", "")
    # Up to 2.1.2xx the row said "connected"; 2.1.281 marks the same state
    # with a check glyph before the server's name and drops the word.
    connected = "connected" in joined or "✔plugin:uze-mcp-conformance" in joined
    check(
        "mcp-server-connected-in-tui",
        connected and "1tool" in joined,
        "/mcp shows the UZE MCP server connected with 1 tool",
    )
    child.send("\x1b")
    time.sleep(1.0)

    # deterministic turn
    for ch in "hi":
        child.send(ch)
        time.sleep(0.07)
    child.send("\r")
    t3, p3, _ = wait_for(["UZE_CONFORMANCE_OK"], tries=20, gap=2.5, stop_on_death=True)
    snap("03_after_prompt", t3)
    check(
        "deterministic-response-rendered",
        "UZE_CONFORMANCE_OK" in p3,
        "UZE_CONFORMANCE_OK rendered in TUI"
        if "UZE_CONFORMANCE_OK" in p3
        else p3[-160:].replace("\n", " "),
    )

    struct = provider_struct(cfg)
    with open(f"{cfg.outdir}/04_provider_struct.json", "w") as f:
        json.dump(struct, f, indent=1)
    # The model-facing contract is the PRIMARY request (the one carrying
    # the Skill tool). Auxiliary no-tools calls (title/context) may include
    # the full skill listing — a documented secondary leak, never the
    # primary contract.
    primary = [r for r in struct if "Skill" in r.get("summary", {}).get("tools", [])]
    check(
        "provider-request-captured",
        bool(primary),
        "the primary request was structurally recorded (tools/skill markers)"
        if primary
        else f"no request offering the Skill tool among {len(struct)} recorded",
    )
    markers = common.observed_markers(primary, "skill_markers")
    # The namespaced label only: a bare `commit` is in every request.
    visible = bool(markers.get("flow:commit"))
    check(
        "model-visible-skill-present",
        visible,
        "flow:commit in the primary request claude sent to its provider",
    )
    common.check_absence(
        "user-only-skill-hidden",
        not markers.get("flow:review"),
        "UZE_CONFORMANCE_OK" in p3,
        proof=visible,
        detail="flow:review absent from the primary request that listed "
        "flow:commit (disable-model-invocation preserved)",
    )

    # MCP execution inside the conversation: PARTIAL — claude defers MCP tools
    # behind ToolSearch (deferred-tool protocol); a direct mcp__ tool_use fails
    # with "No such tool available". Registration + connection are proven via
    # the /mcp TUI surface. Documented, never a pass.
    child.send("\x03")
    time.sleep(0.6)
    child.send("\x03")
    time.sleep(0.6)
    child.close(force=True)


def run(cfg, prov_ip):
    with describe("tui"):
        phase_tui(cfg, prov_ip)
    # Promoted from `experiments/claude/parity` (ADR-035). Imported here: it
    # imports this module for its container helpers. Hooks, session start
    # included, are the hooks contract's (`contract/hooks.py`).
    from experiments.claude import parity

    with describe("parity"):
        parity.run(cfg, prov_ip)
