"""Workspace instructions through the launch context — see `experiments.launch_instructions_probe`.

python3 conformance/lab.py --harness codex --experiment codex/launch-instructions
"""

from experiments.launch_instructions_probe import run

__all__ = ["run"]
