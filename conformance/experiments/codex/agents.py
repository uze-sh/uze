"""How Codex names and dispatches an agent — see `experiments.agent_probe`.

PROBE_SCRIPT=... python3 conformance/lab.py --harness codex --experiment codex/agents
"""

from experiments.agent_probe import run_with
from harnesses.codex.scenarios import codex_setup
from shared import common


def container(cfg, prov_ip, script):
    cmd = common.docker_base(
        cfg, prov_ip, codex_setup(cfg, prov_ip, script, plugins="flow"), tty=False
    )
    ca_crt, _, _ = common.generate_certs(cfg)
    i = cmd.index(common.HARNESS_IMAGE)
    return cmd[:i] + ["-v", f"{ca_crt}:/app/ca.crt:ro"] + cmd[i:]


def run(cfg, prov_ip):
    run_with(cfg, prov_ip, container)
