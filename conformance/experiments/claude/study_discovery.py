"""Where Claude Code discovers skills/agents and which frontmatter it
tolerates — no UZE in the loop; see `study_discovery_lib`.

STUDY_CASE=skills-loc AGY_OUTDIR=... python3 conformance/lab.py --harness claude \\
    --experiment claude/study_discovery
"""

import os

from experiments.agent_probe import run_with
from experiments.claude import study_discovery_lib as lib
from harnesses.claude.scenarios import generate_certs
from shared import common

SETUP = """
set -e
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin
export HOME=/work/home CLAUDE_CONFIG_DIR=/work/home/.claude
export ANTHROPIC_API_KEY=uze-conformance-invalid-by-design
export NODE_EXTRA_CA_CERTS=/app/ca.crt
mkdir -p /work/home/.claude
cp /app/fixtures/claude.json /work/home/.claude.json
"""


def turn(tag):
    return f"""echo '=== turn {tag}'
timeout 150 claude -p "{lib.turn_marker(tag)} hello" --permission-mode bypassPermissions --output-format json 2>&1 | tail -c 1200
echo"""


VALIDATE = """
echo '=== validate'
mkdir -p /work/vplug/.claude-plugin
echo '{"name":"vplug","version":"1.0.0","description":"v"}' > /work/vplug/.claude-plugin/plugin.json
cp -r /work/home/.claude/skills /work/vplug/skills
claude plugin validate /work/vplug 2>&1 | head -60
echo "=== validate-exit $?"
claude plugin validate /work/home/.claude/skills 2>&1 | head -60
"""


MCP_LIST = """
echo '=== mcp-list'
cd /work/proj && timeout 90 claude mcp list 2>&1 | head -40
"""


def container(cfg, prov_ip, script):
    cmd = common.docker_base(cfg, prov_ip, SETUP + script, tty=False)
    ca_crt, _, _ = generate_certs(cfg)
    i = cmd.index(common.HARNESS_IMAGE)
    return cmd[:i] + ["-v", f"{ca_crt}:/app/ca.crt:ro"] + cmd[i:]


def run(cfg, prov_ip):
    case = os.environ.get("STUDY_CASE", "skills-loc")
    script = (
        lib.case_script(
            case,
            turn,
            "/work/home/.claude/skills",
            "/work/home/.claude/agents",
            os.environ.get("STUDY_EXTRA", ""),
        )
        + (VALIDATE if case == "skills-ft" else "")
        + (MCP_LIST if case == "mcp-hooks" else "")
    )
    path = os.path.join(cfg.outdir, "probe.sh")
    with open(path, "w") as f:
        f.write(script)
    os.environ["PROBE_SCRIPT"] = path
    run_with(cfg, prov_ip, container)
    lib.analyze(cfg.outdir)
