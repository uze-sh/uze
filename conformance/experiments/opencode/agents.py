"""How OpenCode names and dispatches an agent — see `experiments.agent_probe`.

PROBE_SCRIPT=... python3 conformance/lab.py --harness opencode --experiment opencode/agents
"""

from experiments.agent_probe import run_with
from harnesses.opencode.scenarios import opencode_setup
from shared import common


def container(cfg, prov_ip, script):
    return common.docker_base(
        cfg, prov_ip, opencode_setup(cfg, prov_ip, script, plugins="flow"), tty=False
    )


def run(cfg, prov_ip):
    run_with(cfg, prov_ip, container)
