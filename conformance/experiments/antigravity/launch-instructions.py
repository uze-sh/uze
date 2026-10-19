"""Workspace instructions through the launch context — see `experiments.launch_instructions_probe`.

python3 conformance/lab.py --harness antigravity --experiment antigravity/launch-instructions
"""

from experiments.launch_instructions_probe import run

__all__ = ["run"]
