"""What an MCP server must do, on every harness.

UZE delivers one MCP server declaration and each integration writes it into
whatever configuration its vendor reads. The configuration shape is the
integration's business and diverges by design; what must not diverge is the
outcome — the server is there, and the harness knows what it offers.

The old suites asserted this four different ways, from
`mcp-server-in-tui-inventory` (one harness) to `mcp-tool-invoked-via-tui`
(another), and one of them was satisfied by the heading "MCP Tools"
rendered above the words "No MCP servers configured". A surface check that
matches its own chrome proves the screen exists, not the server.
"""

from shared.common import check, describe, provider_struct, start_provider

#: The server UZE delivers from the `mcp-plugin` fixture.
SERVER = "uze-conformance"

#: The turn that asks for the server's tool.
PROMPT = "run the mcp probe"


def assert_contract(cfg, prov_ip, bindings):
    with describe("mcp"):
        _assert_inventory(cfg, prov_ip, bindings)
        _assert_execution(cfg, bindings)
    start_provider(cfg, "static")


def _assert_execution(cfg, bindings):
    """The model calls the server's tool and the server's answer reaches the
    next request: the fixture server returns the run's proof, which only a
    call that reached it can carry back. The tool is called the way the
    harness offers it, including loading it first where the harness defers
    MCP tools."""
    calls = bindings.mcp_calls()
    if calls is None:
        # The vertical proves execution in a phase of its own.
        return
    mode, env = bindings.sequence(calls, PROMPT)
    prov_ip = start_provider(cfg, mode, env)
    with bindings.session(cfg, prov_ip) as tui:
        plain, ready = bindings.prepare(tui)
        check(
            "mcp-exec-ready",
            bool(ready),
            f"{bindings.harness} reached its prompt"
            if ready
            else plain[-160:].replace("\n", " "),
        )
        if not ready:
            return
        turn = bindings.hook_turn(tui, PROMPT)
        tui.snapshot("mcp-exec", turn.plain)
    returned = any(
        request.get("summary", {}).get("mcp_proof_present")
        for request in provider_struct(cfg)
    )
    check(
        "mcp-tool-executed",
        returned,
        "the server's answer reached the model"
        if returned
        else f"no request carried the server's proof; {turn.detail}",
    )


def _assert_inventory(cfg, prov_ip, bindings):
    with bindings.session(cfg, prov_ip) as tui:
        plain, ready = bindings.prepare(tui)
        check(
            "mcp-tui-ready",
            bool(ready),
            f"{bindings.harness} reached its prompt"
            if ready
            else plain[-160:].replace("\n", " "),
        )
        if not ready:
            return

        inventory = bindings.mcp_inventory(tui)
        tui.snapshot("mcp", inventory)

        # Named, not merely "a surface exists". The distinction is the whole
        # reason this check is worth running: a heading is not a server.
        present = bindings.names_server(inventory, SERVER)
        check(
            "mcp-server-in-inventory",
            present,
            f"the harness shows `{SERVER}` in its MCP inventory"
            if present
            else inventory[-200:].replace("\n", " "),
        )
