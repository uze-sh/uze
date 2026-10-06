#!/usr/bin/env python3
"""OpenCode scenario (latest channel) — Real Harness + Synthetic World.

Phase A (TUI): the global opencode.json (custom provider + model + MCP)
makes the TUI boot straight to the prompt (no onboarding, observed);
/skills (flow:commit / flow:review / uze:init listed); /mcps (the UZE
MCP server connected + enabled); deterministic turn; provider-request
observation (model-visible Skill present; user-only skill visible to the
model — opencode lists every registered skill in the system prompt, so the
policy is ADAPTED); MCP tool invocation inside the interactive TUI (proof
round-trip: the real opencode executed the real MCP server and returned the
proof value).

OpenCode is the one harness whose provider is configurable: unlike
claude/codex there is no hardcoded host to intercept — the custom
`baseURL` in the config is the hook, plain HTTP.
"""

import json
import os
import re
import sys
import time

import pexpect

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", ".."))
import shared.common as common
from shared.common import (
    check,
    describe,
    docker_base,
    make_screen,
    make_waiter,
    materialize_marketplace,
    provider_struct,
)


def opencode_setup(cfg, prov_ip, final_cmd, plugins="flow mcp-plugin"):
    return f"""
set -e
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin:/usr/local/.opencode/bin
export HOME=/work/home UZE_HOME=/work/home/.uze
export OPENCODE_DISABLE_MODELS_FETCH=1
export UZE_CONFORMANCE_KEY=dummy
mkdir -p /work/home/.config/opencode /work/home/.agents
{materialize_marketplace(cfg)}
uze market add /work/market >/dev/null 2>&1
for p in {plugins}; do uze install $p@uze-lab -m >/dev/null 2>&1; done
node -e '
const fs=require("fs");
const p="/work/home/.config/opencode/opencode.json";
let d={{}};
try {{ d=JSON.parse(fs.readFileSync(p,"utf8")); }} catch (e) {{ d={{}}; }}
d.providers={{"uze-conformance":{{"name":"UZE Conformance","env":["UZE_CONFORMANCE_KEY"],"package":"@opencode-ai/ai/providers/openai-compatible","settings":{{"baseURL":"http://{prov_ip}:9999/v1","apiKey":"{{env:UZE_CONFORMANCE_KEY}}"}},"models":{{"uze-model":{{"modelID":"uze-model","name":"UZE Conformance Model"}},"claude-haiku-4-5":{{"modelID":"claude-haiku-4-5","name":"Claude Haiku 4.5"}}}}}}}};
d.model="uze-conformance/uze-model";
d.agents={{"build":{{"model":"uze-conformance/uze-model"}}}};
fs.writeFileSync(p, JSON.stringify(d,null,1));
'
{final_cmd}
"""


def opencode_container(cfg, prov_ip, final_cmd, plugins="flow mcp-plugin", tty=True):
    cmd = docker_base(
        cfg, prov_ip, opencode_setup(cfg, prov_ip, final_cmd, plugins=plugins), tty=tty
    )
    return cmd


def phase_tui(cfg, prov_ip):
    cmd = opencode_container(
        cfg,
        prov_ip,
        "exec opencode --standalone",
    )
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

    t, p, m = wait_for(["Ask anything"], tries=16, stop_on_death=True)
    snap("01_prompt", t)
    check(
        "tui-reached-prompt",
        "Ask anything" in p,
        "opencode TUI reached its prompt (no onboarding needed)"
        if "Ask anything" in p
        else p[-120:].replace("\n", " "),
    )
    # The prompt renders long before the skills/MCP state finishes loading
    # (observed); typing into the palette too early loses input. The status
    # row "1 MCP" also renders early — what matters is a fixed warmup after
    # the prompt (25s matched the working manual probe), then interact.
    time.sleep(25)

    # /skills — wait for the list to load (the header renders before the
    # entries; the surface fills in async, observed)
    for ch in "/skills":
        child.send(ch)
        time.sleep(0.08)
    time.sleep(1)
    child.send("\r")
    # the surface renders by region (repaint frames split names across
    # reads); accumulate several reads and match markers that survive
    # frame splits
    accumulated = ""
    for _ in range(8):
        time.sleep(1.5)
        try:
            accumulated += child.read_nonblocking(size=400000, timeout=3)
        except Exception:
            break
    t = accumulated
    p = re.sub(r"\x1b\][^\x07]*\x07", "", t)
    p = re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", p).replace("\x1b", "")
    snap("02_skills", t)
    joined = p.replace(" ", "")
    check(
        "skills-surface-in-tui",
        "Skills" in p,
        "/skills opens the skill management surface",
    )
    child.send("\x1b")
    time.sleep(1.0)
    child.send("\x1b")
    time.sleep(1.0)

    # /mcps (trailing s — the MCP toggle surface)
    for ch in "/mcps":
        child.send(ch)
        time.sleep(0.08)
    time.sleep(1)
    child.send("\r")
    accumulated = ""
    for _ in range(8):
        time.sleep(1.5)
        try:
            accumulated += child.read_nonblocking(size=400000, timeout=3)
        except Exception:
            break
    t = accumulated
    p = re.sub(r"\x1b\][^\x07]*\x07", "", t)
    p = re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", p).replace("\x1b", "")
    snap("02b_mcp", t)
    joined = p.replace(" ", "")
    check(
        "mcp-surface-in-tui",
        "MCPs" in p or "mcps" in joined,
        "/mcps opens the MCP toggle surface",
    )
    check(
        "mcp-server-connected-in-tui",
        ("Connected" in p and "✓" in p) or "disconnectspace" in joined,
        "the /mcps surface shows uze-conformance connected + enabled"
        if ("Connected" in p or "disconnectspace" in joined)
        else p[-120:].replace("\n", " "),
    )
    child.send("\x1b")
    time.sleep(1.0)
    child.send("\x1b")
    time.sleep(1.0)

    # deterministic turn
    for ch in "hi":
        child.send(ch)
        time.sleep(0.08)
    time.sleep(1)
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

    # model-facing observation (structural)
    struct = provider_struct(cfg)
    with open(f"{cfg.outdir}/04_provider_struct.json", "w") as f:
        json.dump(struct, f, indent=1)
    markers = common.observed_markers(struct, "skill_markers")
    has_catalog = any(r.get("summary", {}).get("has_available_skills") for r in struct)
    check("provider-request-captured", bool(struct), "requests structurally recorded")
    check(
        "skills-instructions-in-request",
        has_catalog,
        "the model request carries the skills catalog section",
    )
    visible = bool(markers.get("flow:commit"))
    check(
        "model-visible-skill-present",
        visible,
        "flow:commit present in the primary request opencode sent",
    )
    common.check_absence(
        "user-only-skill-hidden-from-model",
        not markers.get("flow:review"),
        "UZE_CONFORMANCE_OK" in p3,
        proof=visible and has_catalog,
        detail="flow:review is omitted from the catalog that listed flow:commit",
    )
    check(
        "model-only-skill-present",
        bool(markers.get("flow:analyze")),
        "flow:analyze is present in model-facing skill discovery",
    )

    child.send("\x03")
    time.sleep(0.5)
    child.send("\x03")
    time.sleep(0.5)
    child.close(force=True)


def phase_mcp_toolcall(cfg, prov_ip):
    """MCP tool invocation inside the interactive TUI: restart the provider
    in toolcall mode and drive a fresh TUI turn; the real opencode executes
    the real MCP server and the proof value returns through the follow-up
    provider request, rendered as UZE_CONFORMANCE_PASS.

    OpenCode 2.x offers an MCP server to the model through Code Mode, not
    as a tool of its own: the request lists the server as a catalog
    namespace in the instructions, and the model calls it with an
    `execute` tool whose JavaScript invokes it (observed on 2.0.18). The
    provider scripts exactly that call, so the proof that comes back is the
    delivered server having run. The earlier reading of this channel — a
    direct call answered `Unknown tool` — scripted a stale tool name, not a
    limit of the harness.
    """
    common.start_provider(cfg, "toolcall")
    time.sleep(1)
    cmd = opencode_container(
        cfg,
        prov_ip,
        "exec opencode --standalone",
    )
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
    t, p, m = wait_for(["Ask anything"], tries=16, stop_on_death=True)
    time.sleep(2)

    for ch in "use the uze_conformance mcp tool":
        child.send(ch)
        time.sleep(0.04)
    time.sleep(1)
    child.send("\r")
    t3, p3, _ = wait_for(
        ["UZE_CONFORMANCE_PASS"], tries=24, gap=2.5, stop_on_death=True
    )
    with open(f"{cfg.outdir}/05_mcp_toolcall.raw", "w") as f:
        f.write(t3)
    check(
        "toolcall-turn-settled",
        "UZE_CONFORMANCE_PASS" in p3,
        "the toolcall turn settled (final text rendered in the TUI)"
        if "UZE_CONFORMANCE_PASS" in p3
        else p3[-160:].replace("\n", " "),
    )

    struct = provider_struct(cfg)
    with open(f"{cfg.outdir}/06_provider_struct_toolcall.json", "w") as f:
        json.dump(struct, f, indent=1)
    has_result = any(r.get("summary", {}).get("has_tool_result") for r in struct)
    model_exposed = any(r.get("summary", {}).get("mcp_tool_present") for r in struct)
    proof_returned = any(r.get("summary", {}).get("mcp_proof_present") for r in struct)

    check(
        "mcp-tool-model-exposed",
        model_exposed,
        "the UZE MCP server is offered to the model (Code Mode namespace)",
    )
    # The proof is the delivered server's own output: only its having run
    # puts it in the follow-up request. A tool message alone is not that —
    # a refused or unknown call answers with one too.
    check(
        "mcp-tool-executed-in-tui",
        proof_returned,
        "the delivered MCP server ran and its proof reached the model"
        if proof_returned
        else "a tool message without the server's proof"
        if has_result
        else "no tool result in the provider requests",
    )

    child.send("\x03")
    time.sleep(0.5)
    child.send("\x03")
    time.sleep(0.5)
    child.close(force=True)


def run(cfg, prov_ip):
    with describe("tui"):
        phase_tui(cfg, prov_ip)
    with describe("mcp.toolcall"):
        phase_mcp_toolcall(cfg, prov_ip)
    # Hooks are the hooks contract's (`contract/hooks.py`).
    # Promoted from `experiments/opencode/skill_files`; imported here
    # because the experiment imports this module for its container helper.
    from experiments.opencode import skill_files

    with describe("skill-files"):
        skill_files.run(cfg, prov_ip)
