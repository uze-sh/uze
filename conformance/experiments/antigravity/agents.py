"""How Antigravity names and dispatches an agent — see `experiments.agent_probe`.

PROBE_SCRIPT=... python3 conformance/lab.py --harness antigravity --experiment antigravity/agents
"""

from experiments.agent_probe import run_with
from harnesses.antigravity.scenarios import agy_setup
from shared import common


def container(cfg, prov_ip, script):
    setup = agy_setup(cfg, prov_ip, include_mcp=False, final_cmd=script)
    return common.docker_base(cfg, prov_ip, setup, tty=False)


def run(cfg, prov_ip):
    run_with(cfg, prov_ip, container)
