"""The Codex explicit-route phase alone, for reading its raw requests.

Run: python3 conformance/lab.py --harness codex --experiment codex/explicit-route --discovery
"""

from harnesses.codex.scenarios import phase_explicit_route


def run(cfg, prov_ip):
    phase_explicit_route(
        cfg, prov_ip, log="RUST_LOG=codex_rmcp_client=debug,codex_core::mcp=debug"
    )
