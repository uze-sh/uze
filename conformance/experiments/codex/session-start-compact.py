"""Codex: does `SessionStart` fire after a compaction, with which `source`,
and does its `additionalContext` reach the model?

See `experiments.compaction_probe`. The hook sits in the person's own
`~/.codex/hooks.json`; nothing from UZE is installed. Codex reviews a hook
before running it, and the review is answered on screen.

Run: python3 conformance/lab.py --harness codex --experiment codex/session-start-compact --discovery
"""

from contract.tui import Tui
from experiments import compaction_probe as cp
from harnesses.codex.bindings import CodexBindings
from harnesses.codex.scenarios import codex_container
from shared import common


def job(world):
    prov_ip = common.start_provider(world, "static", cp.PROVIDER_ENV)
    final = (
        f"{cp.prelude('/work/home/.codex/hooks.json')}\ncd {cp.PROJECT} && exec codex"
    )
    tui = Tui(
        world, codex_container(world, prov_ip, final, plugins=""), "codex-compact"
    )
    tui.expects_hook_review = True
    with tui:
        return cp.drive(world, CodexBindings(), tui)


def run(cfg, prov_ip):
    out = common.concurrently(cfg, {"compact": job})["compact"]
    cp.judge(cfg, "codex-ss", out)
