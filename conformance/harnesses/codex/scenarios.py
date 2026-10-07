#!/usr/bin/env python3
import json
import subprocess

"""Codex scenario (latest channel) — Real Harness + Synthetic World.

Phase A (TUI): auth.json seed skips login; trust prompt dismissed; prompt;
/skills lists the default Skill delivered through the generated plugin;
/plugins lists the plugin; /mcp lists the UZE-delivered server; deterministic
turn; the model request carries `flow:commit` and never `flow:review`.

Phase B (CLI/state): `codex plugin list` reports the UZE plugins installed +
enabled (secondary; the /plugins TUI surface is the primary assertion).

Phase C (invocation policy, ADR-030): `codex debug prompt-input` renders the
model-visible catalog with zero model calls; a user-only Skill must be absent
from it and a default one present, with a sidecar-removal control proving the
exclusion is caused by Codex reading UZE's policy sidecar.

Every absence assertion here is guarded by a presence precondition in the
same capture: an empty catalog hides nothing, it proves nothing.
"""
import os
import re
import sys
import time

import pexpect

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", ".."))
import shared.common as common
from contract.bindings import hook_prelude
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


def codex_setup(cfg, prov_ip, final_cmd, plugins="flow mcp-plugin"):
    return f"""
set -e
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin
export HOME=/work/home CODEX_HOME=/work/home/.codex UZE_HOME=/work/home/.uze
export OPENAI_API_KEY=uze-conformance-invalid-by-design
export CODEX_CA_CERTIFICATES=/app/ca.crt
export SSL_CERT_FILE=/app/ca.crt
mkdir -p /work/home/.codex /work/home/.agents
# decision: synthetic-credentials
cp /app/fixtures/auth.json /work/home/.codex/auth.json
# decision: the container is the sandbox. Codex's own needs the host
# kernel's Landlock, which GitHub's runners do not provide consistently:
# where it is missing, a command fails inside it and the turn ends with the
# tool never run, so the same run passed and failed by runner. Written
# before `uze install`, which keeps it, since a top-level key must precede
# the tables UZE adds.
printf 'sandbox_mode = "danger-full-access"\n' > /work/home/.codex/config.toml
{materialize_marketplace(cfg)}
uze market add /work/market >/dev/null 2>&1
for p in {plugins}; do uze install $p@uze-lab -m >/dev/null 2>&1; done
{final_cmd}
"""


def codex_container(cfg, prov_ip, final_cmd, plugins="flow mcp-plugin", tty=True):
    cmd = docker_base(
        cfg, prov_ip, codex_setup(cfg, prov_ip, final_cmd, plugins=plugins), tty=tty
    )
    ca_crt, _, _ = generate_certs(cfg)
    i = cmd.index(common.HARNESS_IMAGE)
    cmd = (
        cmd[:i]
        + ["-v", f"{ca_crt}:/app/ca.crt:ro", "-e", "CODEX_HOME=/work/home/.codex"]
        + cmd[i:]
    )
    return cmd


def drive_onboarding(child):
    """auth.json seed skips the login screen; the directory-trust prompt is
    dismissed with Enter (its default first option) until the directory is
    trusted. Returns (raw, screen).

    The prompt is recognised by either wording it has had: "Do you trust"
    up to codex-cli 0.155, "Trust this folder?" from 0.156 on, which also
    renamed the option to "Trust and continue". Missing it left every phase
    that opens the TUI parked on the dialog.

    The composer line is chrome, not readiness. codex-cli 0.153.2 paints
    "Ask Codex to do anything" while the header still reads
    `directory: loading`, and only *then* opens the trust dialog over it — so
    a wait that accepts that line returns before the dialog exists, and the
    prompt typed next is swallowed by the dialog instead of starting a turn.
    That race is why a run failed a different subset of the hooks checks each
    time, every one of them reporting a turn that never settled: until the
    directory is trusted the harness loads no project-local hooks at all.

    The header is the signal that cannot race: `directory: loading` until the
    directory is trusted, the working directory afterwards. The path itself is
    never asserted — the isolation phase runs in a worktree, not /work — only
    that `loading` is gone. Reading it off `render_screen` rather than a
    snapshot is what makes it reliable: a read can land mid-redraw, and the
    grid is where the current header exists.

    codex-cli 0.157 starts its shared app-server daemon on an interactive
    launch (openai/codex#47179): it paints a first frame whose header
    already names the directory, steps out of the alternate screen to start
    (and, on a fresh home, install) the daemon, and only then opens the
    trust dialog in a second frame. The directory is no longer the signal
    that cannot race; the model is — `model: loading` until the session the
    daemon serves exists, which is after the directory is trusted. Both are
    required, so the older order still settles the same way.

    codex-cli 0.158 writes neither label: its header is the version, the
    directory and the model, with no `loading` state, and the trust dialog
    arrives in the same frame as the prompt, drawn over it where
    `render_screen` does not recover it. Waiting for the labels therefore
    spent every one of the 30 reads (~3 minutes per launch, measured
    2026-09-28) and answered no dialog. So the dialog is also recognised in
    the text a read returned, and the prompt is the signal again, but only
    after the dialog was answered, or when it keeps arriving with no dialog
    at all (a directory trusted already).
    """
    screen = make_screen(child)
    raw = ""
    shown = ""
    answered = False
    prompt_reads = 0
    quiet_reads = 0
    for _ in range(30):
        chunk, plain = screen(1.5)
        raw += chunk
        shown = common.render_screen(raw)
        squashed = shown.replace(" ", "")
        flat = plain.replace(" ", "")
        if any(
            dialog in squashed or dialog in flat
            for dialog in ("Doyoutrust", "Trustthisfolder?")
        ):
            child.send("\r")
            answered = True
            continue
        if (
            "directory:" in shown
            and "directory:loading" not in squashed
            and "model:" in shown
            and "model:loading" not in squashed
        ):
            break
        prompt = "AskCodextodoanything" in flat
        prompt_reads += prompt
        quiet_reads = quiet_reads + 1 if not chunk else 0
        if answered and (prompt or quiet_reads >= 2):
            break
        if not answered and prompt_reads >= 3:
            break
    return raw, shown


def phase_tui(cfg, prov_ip):
    cmd = codex_container(cfg, prov_ip, "exec codex")
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

    t, p = drive_onboarding(child)
    snap("01_prompt", t)
    check(
        "tui-reached-prompt",
        "Ask Codex to do anything" in p,
        "codex TUI reached its prompt"
        if "Ask Codex" in p
        else p[-120:].replace("\n", " "),
    )

    # /skills
    for ch in "/skills":
        child.send(ch)
        time.sleep(0.1)
    time.sleep(1)
    child.send("\r")
    t, p, m = wait_for(["Choose an action", "skills"], tries=8, stop_on_death=True)
    snap("02_skills", t)
    check(
        "skills-surface-in-tui",
        "Choose an action" in p,
        "/skills opens the skill management surface",
    )
    child.send("2")
    time.sleep(2.5)
    t, p, m = wait_for(["Enable/Disable"], tries=8, stop_on_death=True)
    snap("02b_skills_list", t)
    check(
        "skills-list-opens",
        "Enable/Disable" in p,
        "the Enable/Disable skill list opens",
    )
    child.send("\x1b")
    time.sleep(1.0)

    # /plugins
    for ch in "/plugins":
        child.send(ch)
        time.sleep(0.08)
    time.sleep(1)
    child.send("\r")
    t, p, m = wait_for(["Installed", "Plugins"], tries=8, stop_on_death=True)
    snap("02c_plugins", t)
    check(
        "plugins-in-tui",
        "Installed" in p and "flow" in p,
        "/plugins shows the UZE-delivered `flow` plugin installed"
        if "flow" in p
        else p[-240:].replace("\n", " "),
    )
    child.send("\x1b")
    time.sleep(1.0)

    # /mcp — the inventory must name the UZE-delivered server. The heading
    # "MCP Tools" renders even over "No MCP servers configured", so the
    # heading alone is not evidence.
    for ch in "/mcp":
        child.send(ch)
        time.sleep(0.08)
    time.sleep(1)
    child.send("\r")
    t, p, m = wait_for(
        ["uze-conformance", "No MCP servers configured"], tries=8, stop_on_death=True
    )
    snap("02d_mcp", t)
    check(
        "mcp-server-in-tui-inventory",
        "uze-conformance" in p,
        "/mcp lists the UZE-delivered `uze-conformance` server"
        if "uze-conformance" in p
        else p[-240:].replace("\n", " "),
    )
    child.send("\x1b")
    time.sleep(1.0)

    # deterministic turn
    for ch in "hi":
        child.send(ch)
        time.sleep(0.08)
    time.sleep(1)
    try:
        child.read_nonblocking(size=100000, timeout=3)
    except Exception:
        pass
    child.send("\r")
    t3, p3, _ = wait_for(
        ["UZE_CONFORMANCE_OK", "error", "Error"], tries=20, gap=2.5, stop_on_death=True
    )
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
    default_listed = bool(markers.get("flow:commit"))
    check(
        "model-visible-skill-present",
        default_listed,
        "flow:commit in the request codex sent to its provider"
        if default_listed
        else ", ".join(f"{m}={markers.get(m)}" for m in sorted(markers)),
    )
    check(
        "model-only-skill-present",
        bool(markers.get("flow:analyze")),
        "flow:analyze (model-only, delivered individually) in the request",
    )
    # Only meaningful once the catalog is proven to carry this plugin's
    # skills: an empty catalog would hide `flow:review` for free.
    common.check_absence(
        "user-only-skill-hidden-from-model",
        not markers.get("flow:review"),
        has_catalog,
        proof=default_listed,
        detail="flow:review absent from the request that listed flow:commit",
    )

    child.send("\x03")
    time.sleep(0.5)
    child.send("\x03")
    time.sleep(0.5)
    child.close(force=True)


def phase_plugin_cli(cfg, prov_ip):
    """Secondary state check: `codex plugin list` reports the UZE plugins
    installed + enabled (the /plugins TUI surface is the primary assertion)."""
    final = """
echo '===== codex plugin list ====='
codex plugin list 2>&1
"""
    setup = codex_setup(cfg, prov_ip, final)
    cmd = docker_base(cfg, prov_ip, setup, tty=False)
    ca_crt, _, _ = generate_certs(cfg)
    i = cmd.index(common.HARNESS_IMAGE)
    cmd = (
        cmd[:i]
        + ["-v", f"{ca_crt}:/app/ca.crt:ro", "-e", "CODEX_HOME=/work/home/.codex"]
        + cmd[i:]
    )
    proc = subprocess.run(cmd, capture_output=True, text=True)
    out = proc.stdout + proc.stderr
    with open(f"{cfg.outdir}/05_plugin_list.txt", "w") as f:
        f.write(out)
    check(
        "plugin-delivery",
        "flow@uze-store" in out and "installed, enabled" in out,
        "codex plugin list reports the UZE plugins installed + enabled",
    )


def listed_skills(prompt_input):
    """The plugin skills `codex debug prompt-input` offers to the model, as
    the `- <name>: <description>` catalog lines name them. Plugin skills are
    listed as `<plugin>:<skill>`; individually attached ones carry the same
    label from their wrapper's frontmatter."""
    return set(re.findall(r"- (flow:[a-z]+): ", prompt_input))


def phase_skill_invocation_policy(cfg, prov_ip):
    """Invocation policy (ADR-030) as the REAL Codex binary renders it.

    This is the real-harness half of what `tests/integrations/harness/codex.rs`
    proves deterministically. It used to live there too, spawning `codex` from
    the developer's own PATH — which is UZE's runtime shim on any dogfooding
    machine, so it measured the host rather than Codex. Here the binary, the
    HOME, and the network are the container's.

    `codex debug prompt-input` renders exactly what the model would receive,
    with zero model calls. The `flow` fixture carries every shape: `commit`
    (default: model-visible), `review` (`invoke: {model: false, user:
    true}`: user-only) and `analyze` (model-only). Expected: `flow:commit`
    and `flow:analyze` are offered, `flow:review` is not.

    Delivery shape, so the evidence is read where it lives: `commit` and
    `review` arrive through the GENERATED native plugin (`uze inspect`
    reports them "provided by package"), which Codex stages into its own
    cache under `$CODEX_HOME/plugins/cache/uze-store/flow/<version>/` — the
    sidecar Codex actually reads is that cache copy. `analyze` is Degraded
    on Codex and attached individually under `~/.agents/skills`.

    The control matters as much as the assertion. Deleting the
    `agents/openai.yaml` policy sidecar from the cache copy must bring
    `flow:review` back — proving the exclusion is caused by Codex genuinely
    reading the sidecar UZE wrote, not by the Skill being absent, misnamed,
    or undelivered for some unrelated reason.
    """
    final = """
echo '===== sidecar in the generated envelope ====='
find /work/home/.uze/runtime/attachments/codex/generated -path '*/skills/review/agents/openai.yaml' 2>/dev/null
echo '===== sidecar in the codex plugin cache ====='
find /work/home/.codex/plugins/cache -path '*/skills/review/agents/openai.yaml' 2>/dev/null
echo '===== prompt-input (policy present) ====='
codex debug prompt-input 2>&1
echo '===== prompt-input (policy removed: control) ====='
find /work/home/.codex/plugins/cache -path '*/skills/review/agents/openai.yaml' -delete 2>/dev/null
codex debug prompt-input 2>&1
"""
    setup = codex_setup(cfg, prov_ip, final, plugins="flow")
    cmd = docker_base(cfg, prov_ip, setup, tty=False)
    ca_crt, _, _ = generate_certs(cfg)
    i = cmd.index(common.HARNESS_IMAGE)
    cmd = (
        cmd[:i]
        + ["-v", f"{ca_crt}:/app/ca.crt:ro", "-e", "CODEX_HOME=/work/home/.codex"]
        + cmd[i:]
    )
    proc = subprocess.run(cmd, capture_output=True, text=True)
    out = proc.stdout + proc.stderr
    with open(f"{cfg.outdir}/06_skill_invocation_policy.txt", "w") as f:
        f.write(out)

    envelope_section, _, rest = out.partition(
        "===== sidecar in the codex plugin cache ====="
    )
    cache_section, _, rest = rest.partition("===== prompt-input (policy present) =====")
    with_policy, _, without_policy = rest.partition(
        "===== prompt-input (policy removed: control) ====="
    )
    offered = listed_skills(with_policy)
    offered_without_policy = listed_skills(without_policy)

    check(
        "policy-sidecar-ingested",
        "/plugins/cache/uze-store/flow/" in cache_section
        and "/skills/review/agents/openai.yaml" in cache_section,
        "Codex staged the envelope, sidecar included, into its plugin cache",
    )
    check(
        "default-skill-offered",
        "flow:commit" in offered,
        "flow:commit (generated plugin) is offered to the model"
        if "flow:commit" in offered
        else f"offered: {sorted(offered)}",
    )
    check(
        "model-only-skill-offered",
        "flow:analyze" in offered,
        "flow:analyze (individually attached) is offered to the model"
        if "flow:analyze" in offered
        else f"offered: {sorted(offered)}",
    )
    hidden = "flow:commit" in offered and "flow:review" not in offered
    check(
        "user-only-skill-hidden",
        hidden,
        "flow:review is absent while flow:commit from the same plugin is present"
        if hidden
        else f"not proven — offered: {sorted(offered)}",
    )
    control = hidden and "flow:review" in offered_without_policy
    check(
        "control-sidecar-drives-the-exclusion",
        control,
        "removing the cached sidecar restores flow:review, proving Codex reads it"
        if control
        else f"offered without the sidecar: {sorted(offered_without_policy)}",
    )


def phase_explicit_route(cfg, prov_ip, log=""):
    """A package shipping Codex's own envelope beside an Agent Plugins root
    manifest, which Codex reads first (0.147.0 onwards): each capability
    must load once. Read from Codex's own config and its own MCP listing,
    never from UZE's report; whether code mode then offers the tool to the
    model is the MCP contract's execution scene."""
    common.start_provider(cfg, "static")
    final = f"""{hook_prelude("/work/project")}
cd /work/project
echo ===== config.toml =====
cat /work/home/.codex/config.toml
echo ===== mcp =====
codex mcp list 2>&1
echo ===== turn =====
{log} timeout 240 codex exec 'which probe does this machine keep?' 2>&1 | tail -{400 if log else 5}
"""
    setup = codex_setup(cfg, prov_ip, final, plugins="route-explicit")
    cmd = docker_base(cfg, prov_ip, setup, tty=False)
    ca_crt, _, _ = generate_certs(cfg)
    i = cmd.index(common.HARNESS_IMAGE)
    cmd = (
        cmd[:i]
        + ["-v", f"{ca_crt}:/app/ca.crt:ro", "-e", "CODEX_HOME=/work/home/.codex"]
        + cmd[i:]
    )
    out = subprocess.run(cmd, capture_output=True, text=True, errors="replace").stdout
    with open(f"{cfg.outdir}/07_explicit_route.txt", "w") as f:
        f.write(out)
    config = out.split("===== config.toml =====", 1)[-1].split("===== turn =====", 1)[0]
    registered = "[mcp_servers." in config and "route-probe" in config
    listing = out.split("===== mcp =====", 1)[-1].split("===== turn =====", 1)[0]
    rows = [
        line for line in listing.splitlines() if line.split()[:1] == ["route-probe"]
    ]
    check(
        "explicit-route-mcp-not-registered-twice",
        not registered,
        "the server the plugin carries is not also registered in config.toml"
        if not registered
        else "config.toml registers the server the plugin already carries",
    )
    check(
        "explicit-route-mcp-loaded-once",
        len(rows) == 1,
        "`codex mcp list` names the plugin's server once"
        if len(rows) == 1
        else f"`codex mcp list` names it {len(rows)} times: {listing[-300:]!r}",
    )


def run(cfg, prov_ip):
    with describe("tui"):
        phase_tui(cfg, prov_ip)
    with describe("cli.state"):
        phase_plugin_cli(cfg, prov_ip)
    with describe("skill-invocation-policy"):
        phase_skill_invocation_policy(cfg, prov_ip)
    with describe("explicit-route"):
        phase_explicit_route(cfg, prov_ip)
    # Hooks, session start included, are the hooks contract's
    # (`contract/hooks.py`).
