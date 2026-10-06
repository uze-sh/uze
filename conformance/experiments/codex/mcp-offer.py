"""Whether Codex offers a delivered MCP server's tool to the model.

The MCP contract proves the server is registered (`/mcp` lists it). This
asks the next question in a person's own session: once `/mcp` shows the
server, a user turn is sent, and the request it produces is read for the
server's tool — as a namespace, a nested tool inside code mode's `exec`,
or behind `tool_search`. The requests are read from `raw-requests.log`
once the run has pulled it.

Run: python3 conformance/lab.py --harness codex --experiment codex/mcp-offer --discovery
"""

from contract.mcp import SERVER
from harnesses.codex.bindings import CodexBindings
from shared import common

PROMPT = "run the mcp probe"


def run(cfg, prov_ip):
    bindings = CodexBindings()
    prov_ip = common.start_provider(cfg, "static")
    with bindings.session(cfg, prov_ip) as tui:
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
    common.check(
        "mcp-offer-turn-settled",
        turn.settled,
        turn.detail,
    )
