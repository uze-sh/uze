"""A portable SessionStart hook on this harness — see `experiments.session_start_probe`.

python3 conformance/lab.py --harness codex --experiment codex/session-start
"""

from experiments.session_start_probe import run

__all__ = ["run"]
