"""Claude Code: does a `SessionStart` hook with no matcher fire for
`compact` and `clear`, and does its `additionalContext` reach the model?

See `experiments.compaction_probe`. The hook sits in the person's own
`~/.claude/settings.json`; nothing from UZE is installed.

Run: python3 conformance/lab.py --harness claude --experiment claude/session-start-compact --discovery
"""

from contract.tui import Tui
from experiments import compaction_probe as cp
from harnesses.claude.bindings import ClaudeBindings
from harnesses.claude.scenarios import claude_container
from shared import common


def job(world):
    prov_ip = common.start_provider(world, "static", cp.PROVIDER_ENV)
    final = f"{cp.prelude('/work/home/.claude/settings.json')}\ncd {cp.PROJECT} && exec claude"
    tui = Tui(
        world, claude_container(world, prov_ip, final, plugins=""), "claude-compact"
    )
    with tui:
        return cp.drive(world, ClaudeBindings(), tui)


def run(cfg, prov_ip):
    out = common.concurrently(cfg, {"compact": job})["compact"]
    cp.judge(cfg, "claude-ss", out)
