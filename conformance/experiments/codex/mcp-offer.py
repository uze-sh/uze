"""Whether Codex offers a delivered MCP server's tool to the model.

The MCP contract proves the server is registered (`/mcp` lists it). This
asks the next question in a person's own session: once `/mcp` shows the
server, a user turn is sent, and the request it produces is read for the
server's tool — as a namespace, a nested tool inside code mode's `exec`,
or behind `tool_search`. The requests are read from `raw-requests.log`
once the run has pulled it.

Measured on 0.160.1: the default model runs in code mode with the search
tool on, so every MCP tool is a deferred nested tool, named nowhere in the
request but reachable as `tools.mcp__<server>__<tool>` inside `exec` (the
MCP contract's execution scene calls it); `MCP_OFFER_ARGS=" -m o3"` opens
a direct-mode model instead, whose request carries the `mcp__<server>`
namespace. A server written into `config.toml` by hand rides along as the
control, so plugin delivery is never the only thing measured.

Run: python3 conformance/lab.py --harness codex --experiment codex/mcp-offer --discovery
"""

import os
import subprocess

from contract.mcp import SERVER
from contract.tui import Tui
from harnesses.codex.bindings import CodexBindings
from harnesses.codex.scenarios import codex_container
from shared import common

PROMPT = "run the mcp probe"


def run(cfg, prov_ip):
    bindings = CodexBindings()
    prov_ip = common.start_provider(cfg, "static")
    # The session a person opens, with Codex's MCP and tool-plan tracing on.
    # Beside the server UZE delivered through its plugin, one a person
    # writes into `config.toml` by hand: the control that tells "Codex
    # offers no MCP tool" from "Codex offers no plugin's MCP tool".
    hand = (
        'printf \'\\n[mcp_servers.handprobe]\\ncommand = "%s"\\nargs = ["--proof", "HAND"]\\n\' '
        f"{cfg.mcp_fixture_bin} >> /work/home/.codex/config.toml; "
    )
    launch = (
        hand + "RUST_LOG=codex_mcp=trace,codex_core=debug exec codex "
        "-c log_dir='\"/work/codexlog\"'" + os.environ.get("MCP_OFFER_ARGS", "")
    )
    with Tui(cfg, codex_container(cfg, prov_ip, launch), "codex-mcp-offer") as tui:
        plain, ready = bindings.prepare(tui)
        common.check("mcp-offer-ready", bool(ready), plain[-160:].replace("\n", " "))
        if not ready:
            return
        inventory = bindings.mcp_inventory(tui)
        listed = bindings.names_server(inventory, SERVER)
        common.check(
            "mcp-offer-server-listed", listed, inventory[-200:].replace("\n", " ")
        )
        tui.child.send("\x1b")
        turn = bindings.hook_turn(tui, PROMPT)
        tui.snapshot("mcp-offer-turn", turn.plain)
        # A second turn in the same session: whether a server that finished
        # starting after the session opened reaches the model later.
        second = bindings.hook_turn(tui, "run the mcp probe again")
        tui.snapshot("mcp-offer-second", second.plain)
        # Codex's own account of the turn: which MCP servers and tools the
        # model binding captured, and why one was left out.
        log = subprocess.run(
            [
                "docker",
                "exec",
                cfg.harness_container,
                "sh",
                "-c",
                "ls -la /work/codexlog 2>&1; cat /work/codexlog/* 2>/dev/null",
            ],
            capture_output=True,
            text=True,
            errors="replace",
        ).stdout
        with open(f"{cfg.outdir}/codex-log.txt", "w") as f:
            f.write(log)
    common.check(
        "mcp-offer-turn-settled",
        turn.settled,
        turn.detail,
    )
