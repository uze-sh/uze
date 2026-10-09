#!/usr/bin/env python3
import json
import subprocess

"""Antigravity scenario (latest channel) — Real Harness + Synthetic World.

The session runs **signed in**, against a synthetic identity: a `consumer`
token file the CLI reads as a Google account, and the CloudCode plane
answered by the run's own provider (identity, tier, flags, model catalogue,
model path). That is the mode users are in. API-key mode is measured too:
until 1.1.24 it loaded `hooks.json` hooks and never ran them (vendor bug
google-antigravity/antigravity-cli#893, kept on the report as a
declaration); since 1.1.25 the same probe sees the hook fire there, so
that check is asserted like the rest.

Phase A (TUI): prompt + synthetic credential, /skills (flow:commit,
flow:review, uze:init), /mcp (server listed + tools enumerated),
deterministic turn, model-only Skill hidden from the slash surface but
present for the model, user-only Skill CAPABILITY_ADAPTED, MCP tool invocation
inside the interactive TUI (proof round-trip).

Phase B (CLI/state): plugin registration via `agy plugin list` + staged
mcp_config.json (AGY has no plugin TUI surface — verified).
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
    make_screen,
    make_waiter,
    materialize_marketplace,
    provider_struct,
    start_provider,
)
from shared.markers import AGENTS

#: Signed-in ("consumer") mode is how the vertical runs: it is the mode
#: users are in, and until 1.1.24 the only one in which this harness
#: executed `hooks.json` hooks at all (vendor bug
#: google-antigravity/antigravity-cli#893). API-key mode stays reachable —
#: `phase_hooks_api_key_mode` asserts the hook fires there since 1.1.25 —
#: and is what `auth="apikey"` selects.
CONSUMER_TOKEN = "/work/home/.gemini/antigravity-cli/antigravity-oauth-token"


def auth_fragment(prov_ip, auth):
    if auth == "apikey":
        # `modelProvider: gemini` is what routes the turn at the API key:
        # without it the CLI expects its signed-in backend, and with it the
        # CLI refuses to start unless GEMINI_API_KEY is set — the two travel
        # together, which is why the mode carries its own settings fixture.
        return f"""export GEMINI_API_KEY=uze-conformance-invalid-by-design
export GOOGLE_GEMINI_BASE_URL=http://{prov_ip}:9999
cp /app/fixtures/settings-api-key.json /work/home/.gemini/antigravity-cli/settings.json
rm -f {CONSUMER_TOKEN}"""
    # The synthetic account: a token file the CLI reads as a signed-in
    # session, whose every value is a literal and whose expiry is far
    # enough away that no refresh is attempted. With it in place the CLI
    # ignores GOOGLE_GEMINI_BASE_URL and speaks CloudCode over TLS — which
    # the provider's signed-in listener answers.
    return f"cp /app/fixtures/antigravity-oauth-token {CONSUMER_TOKEN}"


def agy_setup(
    cfg, prov_ip, include_mcp, final_cmd, plugins=None, prelude="", auth="consumer"
):
    if plugins is None:
        plugins = "flow"
        if include_mcp:
            plugins += " mcp-plugin"
    return f"""
set -e
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin
export HOME=/work/home UZE_HOME=/work/home/.uze
export AGY_CLI_DISABLE_AUTO_UPDATE=1
# The harness is Go and honours SSL_CERT_FILE on Linux: this is how it
# trusts the run's synthetic CA for its own signed-in plane (identity,
# feature flags, account endpoints, the model path) without any Internet.
export SSL_CERT_FILE=/app/ca.crt
export SSL_CERT_DIR=/app
mkdir -p /work/home/.gemini/antigravity-cli
cp /app/fixtures/settings.json /work/home/.gemini/antigravity-cli/settings.json
cp /app/fixtures/jetski_state.pbtxt /work/home/.gemini/antigravity-cli/jetski_state.pbtxt
cp /app/fixtures/installation_id /work/home/.gemini/antigravity-cli/installation_id
{auth_fragment(prov_ip, auth)}
{materialize_marketplace(cfg)}
uze market add /work/market >/dev/null 2>&1
for p in {plugins}; do uze install $p@uze-lab -m >/dev/null 2>&1; done
{prelude}
{final_cmd}
"""


def answer_first_run(child, screen):
    """What a person answers the first time agy opens in a directory, on
    screen: the colour scheme, the terms, and — in a directory it has not
    seen, which every fresh world is — the folder trust, whose "Yes, I trust
    this folder" is preselected. Text typed while any of them is up goes to
    the dialog, never to the prompt. Returns the screen after them, or
    `None` when the onboarding never appeared.

    Each answer waits for the screen it is meant for rather than for a
    fixed nap: the naps (3s, 5s and 5s, each followed by a read of up to 3s)
    were paid in full by every session of the vertical, though each screen
    is up in well under a second. They stay as the ceilings."""
    try:
        child.expect("Choose your color scheme", timeout=150)
    except Exception:
        return None
    child.send("\r")
    await_screen(child, ["[Done]"], 3)
    child.send("\t\t")
    time.sleep(0.7)
    child.send("\r")
    plain, matched = await_screen(child, ["trust the contents", PROMPT_MARKER], 8)
    if matched == "trust the contents":
        child.send("\r")
        plain, _ = await_screen(child, [PROMPT_MARKER], 8)
    return plain


#: What agy's prompt shows under its input line, and no dialog does.
PROMPT_MARKER = "? for shortcuts"


def await_screen(child, markers, seconds, settle=0.6):
    """Reads until one of `markers` is on screen and the frame that carried
    it has finished arriving, or `seconds` run out. Returns the plain text
    read and the marker found (`None` past the ceiling)."""
    raw = ""
    deadline = time.monotonic() + seconds
    found = None
    while time.monotonic() < deadline:
        try:
            raw += child.read_nonblocking(
                size=250000, timeout=max(0.05, deadline - time.monotonic())
            )
        except pexpect.EOF:
            break
        except Exception:
            continue
        plain = common.squash(common.ansi_strip(raw))
        found = next((m for m in markers if common.squash(m) in plain), None)
        if found:
            break
    if found:
        # The rest of the frame: a TUI paints one screen in several writes.
        while True:
            try:
                more = child.read_nonblocking(size=250000, timeout=settle)
            except Exception:
                break
            if not more:
                break
            raw += more
    return common.ansi_strip(raw), found


def phase_tui(cfg, prov_ip):
    setup = agy_setup(cfg, prov_ip, include_mcp=True, final_cmd="exec agy")
    cmd = docker_base(cfg, prov_ip, setup)
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

    first = answer_first_run(child, screen)
    started = first is not None
    check(
        "tui-started",
        started,
        "the onboarding appeared" if started else "onboarding never appeared",
    )
    if not started:
        child.close(force=True)
        return

    t1, p1 = screen(3)
    # The first-run answers read the screen up to the prompt; what arrives
    # after them is only what changed since.
    p1 = f"{first}\n{p1}"
    snap("01_prompt", p1)
    check(
        "tui-reached-prompt",
        "Antigravity CLI" in p1 and ">" in p1,
        "header visible" if "Antigravity CLI" in p1 else "no header",
    )

    # /skills
    child.send("/")
    time.sleep(1.2)
    for ch in "skills":
        child.send(ch)
        time.sleep(0.15)
    time.sleep(1.2)
    child.send("\r")
    t2, p2, _ = wait_for(["flow:review"], tries=4, stop_on_death=True)
    snap("02_skills", t2)
    check("official-uzek-skill-visible", "uze:init" in p2, "uze:init in /skills")
    child.send("\x1b")
    time.sleep(1.0)
    t_settle, _, _ = wait_for([">"], tries=6, stop_on_death=True)
    snap("02c_back_to_prompt", t_settle)

    # /agents is the native agent manager; fixture visibility is the
    # behavioral proof that AGY loaded UZE's portable definition.
    child.send("/")
    time.sleep(1.2)
    for ch in "agents":
        child.send(ch)
        time.sleep(0.15)
    child.send("\r")
    label = AGENTS["flat"][0]
    t_agents, p_agents, _ = wait_for(["auditor", "Agents"], tries=8, stop_on_death=True)
    snap("02a_agents", t_agents)
    # By the label UZE commits to on every harness (`contract.agent`); agy
    # names an agent by its frontmatter `name`, so a bare `auditor` here is
    # a definition delivered without it.
    listed = label in p_agents.replace(" ", "")
    check(
        "agent-visible-in-tui",
        listed,
        f"Antigravity /agents lists the UZE agent as `{label}`"
        if listed
        else p_agents[-200:].replace("\n", " "),
    )
    child.send("\x1b")
    time.sleep(1.0)

    # /mcp
    child.send("/")
    time.sleep(1.2)
    for ch in "mcp":
        child.send(ch)
        time.sleep(0.15)
    time.sleep(1.2)
    child.send("\r")
    t_mcp, p_mcp, _ = wait_for(["Tools: uze_conformance"], tries=8, stop_on_death=True)
    snap("02b_mcp", t_mcp)
    check(
        "mcp-server-visible-in-tui",
        "uze-conformance" in p_mcp,
        "the UZE-delivered MCP server is listed in /mcp",
    )
    check(
        "mcp-server-connected-in-tui",
        "Tools: uze_conformance" in p_mcp,
        "the real AGY loaded the server and enumerated its tool",
    )
    child.send("\x1b")
    time.sleep(1.0)
    t_settle, _, _ = wait_for([">"], tries=6, stop_on_death=True)
    snap("02d_back_to_prompt", t_settle)

    # deterministic turn
    for ch in "hi":
        child.send(ch)
        time.sleep(0.1)
    child.send("\r")
    # The answer is streamed and the TUI repaints around it, so a marker can
    # land across two screen reads (`UZE_CONFORMA` … `NCE_OK`): the wait
    # searches everything read since the turn started.
    t3, p3, _ = wait_for(
        ["UZE_CONFORMANCE_OK"],
        tries=30,
        gap=2.5,
        stop_on_death=True,
        accumulate=True,
    )
    snap("03_after_prompt", t3)
    check(
        "deterministic-response-rendered",
        "UZE_CONFORMANCE_OK" in p3,
        "UZE_CONFORMANCE_OK rendered in TUI"
        if "UZE_CONFORMANCE_OK" in p3
        else p3[-160:].replace("\n", " "),
    )
    check(
        "agent-loop-clean",
        "Agent execution terminated due to error" not in p3,
        "no agent-loop error after the rendered turn",
    )

    struct = provider_struct(cfg)
    with open(f"{cfg.outdir}/04_provider_struct.json", "w") as f:
        json.dump(struct, f, indent=1)
    summaries = [entry.get("summary", {}) for entry in struct]
    markers = [summary.get("skill_markers", {}) for summary in summaries]
    # The qualified label only. A marker is a substring test over the
    # whole request body, so a bare `review`/`commit`/`analyze` also
    # matches the word in any prose the harness sends — `init` shows up
    # for exactly that reason and is no skill of UZE's. `flow:<name>`
    # is the delivered identity and the only marker that discriminates.
    model_visible = any(marker.get("flow:commit") for marker in markers)
    check(
        "model-visible-skill-present",
        model_visible,
        "flow:commit in the request the harness sent to its provider",
    )
    # Gated on the presence above: "the policy worked" and "nothing was
    # delivered" are the same observation otherwise (DECISIONS.md).
    common.check_absence(
        "user-only-skill-hidden",
        not any(marker.get("flow:review") for marker in markers),
        "UZE_CONFORMANCE_OK" in p3,
        proof=model_visible,
        detail="flow:review absent from the request that listed flow:commit "
        "(disable-model-invocation preserved)",
    )
    check(
        "model-only-skill-present",
        any(marker.get("flow:analyze") for marker in markers),
        "flow:analyze present in the request while absent from /skills",
    )
    check(
        "provider-request-captured",
        any(summary.get("tools") for summary in summaries),
        "request body structurally recorded (tools/skills/markers)",
    )

    # MCP invocation inside the interactive TUI conversation
    time.sleep(2)
    try:
        child.read_nonblocking(size=200000, timeout=3)
    except Exception:
        pass
    t_settle, _, _ = wait_for([">"], tries=8, stop_on_death=True)
    snap("02e_settle", t_settle)
    start_provider(cfg, "toolcall")
    for ch in "call the uze_conformance mcp tool":
        child.send(ch)
        time.sleep(0.08)
    child.send("\r")
    # Transcript plus rendered screen, for the same reason as the
    # deterministic turn: a streamed final is continued by cursor motion.
    t4, chunk = screen(1.2)
    raw4, plain4 = t4, chunk
    p4 = f"{plain4}\n{common.render_screen(raw4)}"
    tries = 0
    while "UZE_CONFORMANCE_PASS" not in p4 and tries < 14 and child.isalive():
        # A person approves the server's tool when agy asks, with the
        # preselected "Yes".
        if any(prompt in chunk for prompt in PERMISSION_PROMPTS):
            child.send("\r")
            time.sleep(1.0)
        t4, chunk = screen(2.0)
        raw4 += t4
        plain4 += chunk
        p4 = f"{plain4}\n{common.render_screen(raw4)}"
        tries += 1
    snap("03b_mcp_invoke_tui", t4)
    check(
        "mcp-tool-invoked-via-tui",
        "UZE_CONFORMANCE_PASS" in p4
        and "Agent execution terminated due to error" not in p4,
        # The final text is served after any function response; that the
        # server ran is `mcp-tool-executed-in-tui`'s, which reads its proof.
        "MCP tool call executed and final rendered in the interactive TUI"
        if "UZE_CONFORMANCE_PASS" in p4
        else p4[-160:].replace("\n", " "),
    )
    struct2 = provider_struct(cfg)
    with open(f"{cfg.outdir}/04b_mcp_invoke_struct.json", "w") as f:
        json.dump(struct2, f, indent=1)
    # The proof rides in the request that carries the functionResponse; the
    # harness's side requests (a lighter model, no tools) come after it, so
    # the last request is not the one to read.
    executed = any(
        r.get("summary", {}).get("has_function_response")
        and r.get("summary", {}).get("mcp_proof_present")
        for r in struct2
    )
    check(
        "mcp-tool-executed-in-tui",
        executed,
        "the REAL AGY executed the MCP server inside the TUI turn (proof returned)"
        if executed
        else "a functionResponse without the proof, or none at all",
    )

    child.send("\x03")
    time.sleep(0.5)
    child.sendline("/exit")
    time.sleep(2)
    child.close(force=True)


RUN_COMMAND_ARGS = (
    '{"CommandLine":"%s","Cwd":"/work","WaitMsBeforeAsync":2000,'
    '"toolSummary":"Command execution","toolAction":"Running command"}'
)

#: A deny hook in the vendor's own file format at the vendor's own shared
#: path, with no UZE in the loop: the control that says whether this AGY
#: executes `hooks.json` hooks at all in this session.
VENDOR_CONTROL_HOOK = """cat > /work/home/.gemini/config/hooks.json <<'EOF'
{
  "uze-conformance-control": {
    "PreToolUse": [
      {
        "matcher": "run_command",
        "hooks": [
          {
            "type": "command",
            "command": "printf '{\\"decision\\":\\"deny\\",\\"reason\\":\\"blocked by protect-env\\"}'"
          }
        ]
      }
    ]
  }
}
EOF
"""

HOOK_DENIAL_MARKERS = ("blocked by protect-env", "Denied by UZE hook")

#: Every wording this harness has used to ask a person to approve the tool
#: call — an unanswered prompt stalls the turn, and the allow scenario then
#: reads as "the hook never let the command run", which is what 1.1.28 did
#: to it. Through 1.1.27 the question was "Do you want to proceed"; 1.1.28
#: made it say what is being approved ("Run this command?", "Allow access
#: to this URL?", "Allow calling this tool?" — its changelog), so the match
#: is on the header above them all rather than on any one question.
#: What agy shows when it asks before a call: every permission menu —
#: a command's, an edit's, an MCP tool's ("Allow calling this tool?") —
#: carries the same navigation line under it, with "Yes" preselected; the
#: headers above differ.
PERMISSION_PROMPTS = ("Navigate · tab Amend", "Do you want to proceed")


def hook_turn(cfg, prov_ip, tag, args, plugins, prelude="", auth="consumer"):
    """One interactive AGY turn around a scripted `run_command` (in the
    tool's declared argument shape): the provider serves the functionCall
    to the user's turn, the vendor permission prompt is answered if it
    appears, and the turn is left settled. Returns the TUI verdict and the
    provider-observed hook markers."""
    start_provider(cfg, "toolcall", {"TOOL_NAME": "run_command", "FC_ARGS": args})
    time.sleep(1)
    setup = agy_setup(
        cfg,
        prov_ip,
        include_mcp=False,
        final_cmd="exec agy",
        plugins=plugins,
        prelude=prelude,
        auth=auth,
    )
    cmd = docker_base(cfg, prov_ip, setup)
    child = pexpect.spawn(
        cmd[0], cmd[1:], encoding="utf-8", codec_errors="replace", timeout=300
    )
    child.setwinsize(50, 160)
    try:
        child.logfile_read = common.CastRecorder(cfg.outdir, f"tui-hooks-{tag}")
    except Exception:
        pass
    screen = make_screen(child)
    wait_for = make_waiter(screen)

    p1 = answer_first_run(child, screen)
    started = p1 is not None
    check(
        f"hooks-{tag}-tui-started",
        started,
        "the onboarding appeared" if started else "onboarding never appeared",
    )
    if not started:
        child.close(force=True)
        return None
    if ">" not in p1:
        wait_for([">"], tries=6, stop_on_death=True)

    for ch in "run the API check":
        child.send(ch)
        time.sleep(0.08)
    child.send("\r")
    # The transcript, plus the screen it renders to: AGY streams its answer
    # and continues the line by moving the cursor, so a marker can exist on
    # screen while no snapshot — and no concatenation of snapshots — holds it
    # contiguously (`… UZE_CONFORMA`, then `ESC[3A ESC[12C NCE_PASS`).
    seen_raw = ""
    seen = ""
    prompted = False

    def settled_now():
        rendered = f"{seen}\n{common.render_screen(seen_raw)}"
        return "UZE_CONFORMANCE_PASS" in rendered or any(
            m in rendered for m in HOOK_DENIAL_MARKERS
        )

    for _ in range(16):
        t, p = screen(2.0)
        seen_raw += t
        seen += p
        if settled_now():
            break
        if not child.isalive():
            break
        # The vendor's own permission prompt for the command: a person
        # approves it, and the hook decision — never this prompt — is what
        # the turn is judged on. A deny hook that ran never shows it.
        if any(m in seen for m in PERMISSION_PROMPTS) and not prompted:
            prompted = True
            child.send("\r")
            time.sleep(1.0)
        elif "How's the CLI experience" in p:
            child.send("0\r")
            time.sleep(1.0)
    with open(f"{cfg.outdir}/hooks_{tag}.raw", "w") as f:
        f.write(f"{seen}\n===== rendered =====\n{common.render_screen(seen_raw)}")
    turn_settled = settled_now()
    # Absence checks may only evaluate once the turn settled and the TUI
    # went quiet (ADR-035).
    settled = turn_settled and common.settle_and_quiet(screen)

    struct = provider_struct(cfg)
    with open(f"{cfg.outdir}/hooks_{tag}_struct.json", "w") as f:
        json.dump(struct, f, indent=1)
    child.send("\x03")
    time.sleep(0.5)
    child.close(force=True)
    return {
        "turn_settled": turn_settled,
        "settled": settled,
        "prompted": prompted,
        "tail": seen[-160:].replace("\n", " "),
        "markers": common.observed_markers(struct, "hook_markers"),
    }


def phase_hooks_gate(cfg, prov_ip):
    """Whether this AGY executes `hooks.json` hooks at all — measured, not
    assumed, with the vendor's own format at the vendor's own path and no
    UZE in the loop.

    Signed in, it does: the deny hook runs, the TUI renders `Tool call
    denied by pre-tool hook: blocked by protect-env`, and the reason reaches
    the conversation as the tool outcome (1.1.24, 2026-09-02, experiment
    `antigravity/signed-in`). The UZE hook checks that follow are therefore
    asserted, not declared.

    The gate stays a live precondition rather than an assumption because the
    thing it measures is a vendor gate, not ours: the executor reads
    `enable_json_hooks`, field 17 of `exa.cortex_pb.CustomizationConfig`,
    which the CLI only ever receives over the CloudCode backend it speaks
    when signed in. Under `GEMINI_API_KEY` no such config arrived on 1.1.24,
    whatever the `json-hooks-enabled` flag said — vendor bug
    google-antigravity/antigravity-cli#893 — and `phase_hooks_api_key_mode`
    re-runs this control hook there every run. If a future build closed the
    gate in signed-in mode too, this check would say so instead of the suite
    quietly proving nothing. `phase_hooks_delivery` answers the other half:
    whether the harness loads what UZE itself delivered.

    Returns True when the control hook denied the command.
    """
    outcome = hook_turn(
        cfg,
        prov_ip,
        "vendor",
        RUN_COMMAND_ARGS % "echo API secrets",
        plugins="flow",
        prelude=VENDOR_CONTROL_HOOK,
    )
    executes = bool(outcome and outcome["markers"].get("blocked by protect-env"))
    # A closed gate would leave every hook UZE delivers here inert, so it
    # fails: the hooks contract's results mean nothing without it.
    check(
        "hooks-vendor-hook-executes",
        executes,
        "a vendor-format deny hook at ~/.gemini/config/hooks.json denied run_command"
        if executes
        else (
            "no hooks.json hook executes in this AGY session: the vendor-format "
            "control hook was loaded and listed but never ran"
            + (
                " (the permission prompt surfaced instead)"
                if outcome["prompted"]
                else ""
            )
        ),
    )
    return executes


def phase_hooks_api_key_mode(cfg, prov_ip):
    """The same control hook, the same turn, on a Gemini API key — the one
    variable is the auth mode.

    On 1.1.24 the API-key session loaded the hook and ran nothing, so the
    command reached the vendor's permission prompt instead: that was
    google-antigravity/antigravity-cli#893 ("Hooks from .agents/hooks.json
    are loaded but never executed when authenticated via GEMINI_API_KEY",
    opened 2026-08-28; #78 records that Google does not support the API-key
    path at all), and the Lab carried it as a registered declaration so the
    bug stayed on the report. On 1.1.25 the identical probe measured the
    hook firing under the API key — four independent CI runs on 2026-09-03,
    while the issue was still open upstream — which escalated the
    declaration exactly as ADR-035 says it must. From here on the mode is
    asserted: the hook must deny the command on the API key as it does
    signed in, and a regression to #893 fails the vertical rather than
    hiding behind a declaration whose reason stopped being true.
    """
    outcome = hook_turn(
        cfg,
        prov_ip,
        "api-key",
        RUN_COMMAND_ARGS % "echo API secrets",
        plugins="flow",
        prelude=VENDOR_CONTROL_HOOK,
        auth="apikey",
    )
    if outcome is None:
        return
    executes = bool(outcome["markers"].get("blocked by protect-env"))
    check(
        "hooks-api-key-mode-hook-executes",
        executes,
        "the vendor-format deny hook fires under GEMINI_API_KEY as it does "
        "signed in (`blocked by protect-env` reached the conversation)"
        if executes
        else "the hook loaded but never ran under GEMINI_API_KEY"
        + (" — the permission prompt surfaced instead" if outcome["prompted"] else "")
        + " (google-antigravity/antigravity-cli#893 is back)",
    )


def phase_mcp_registration(cfg, prov_ip):
    final = """
echo '===== S1 plugin list ====='
agy plugin list 2>&1
echo '===== S2 staged mcp_config.json ====='
cat /work/home/.gemini/config/plugins/uze-mcp-conformance/mcp_config.json 2>&1
"""
    setup = agy_setup(cfg, prov_ip, include_mcp=True, final_cmd=final)
    cmd = docker_base(cfg, prov_ip, setup, tty=False)
    proc = subprocess.run(cmd, capture_output=True, text=True)
    out = proc.stdout + proc.stderr
    with open(f"{cfg.outdir}/05_mcp_registration.txt", "w") as f:
        f.write(out)
    check(
        "mcp-plugin-registered",
        '"mcpServers"' in out and "uze-mcp-conformance" in out,
        "S1: plugin list shows the MCP plugin with an mcpServers component",
    )


def run(cfg, prov_ip):
    with describe("tui"):
        phase_tui(cfg, prov_ip)
    with describe("cli.state"):
        phase_mcp_registration(cfg, prov_ip)
    with describe("hooks"):
        # Whether this harness runs `hooks.json` hooks at all, in either auth
        # mode — the vendor's own format, no UZE in the loop. UZE's hooks
        # are the hooks contract's (`contract/hooks.py`).
        with describe("vendor"):
            phase_hooks_gate(cfg, prov_ip)
        with describe("api-key"):
            phase_hooks_api_key_mode(cfg, prov_ip)
