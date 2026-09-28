"""Where Antigravity discovers skills/agents and which frontmatter it
tolerates — no UZE in the loop; see `experiments.claude.study_discovery_lib`.

STUDY_CASE=skills-loc AGY_OUTDIR=... python3 conformance/lab.py --harness antigravity \\
    --experiment antigravity/study_discovery
"""

import os

from experiments.agent_probe import run_with
from experiments.claude import study_discovery_lib as lib
from harnesses.antigravity.scenarios import auth_fragment
from shared import common


def setup(prov_ip):
    return f"""
set -e
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin
export HOME=/work/home
export AGY_CLI_DISABLE_AUTO_UPDATE=1
export SSL_CERT_FILE=/app/ca.crt
export SSL_CERT_DIR=/app
mkdir -p /work/home/.gemini/antigravity-cli
cp /app/fixtures/settings.json /work/home/.gemini/antigravity-cli/settings.json
cp /app/fixtures/jetski_state.pbtxt /work/home/.gemini/antigravity-cli/jetski_state.pbtxt
cp /app/fixtures/installation_id /work/home/.gemini/antigravity-cli/installation_id
{auth_fragment(prov_ip, "consumer")}
"""


def turn(tag):
    return f"""echo '=== turn {tag}'
pkill -x agy 2>/dev/null
timeout 200 agy {os.environ.get("STUDY_AGY_ARGS", "")} --print "{lib.turn_marker(tag)} hello" --dangerously-skip-permissions --print-timeout 90s --log-file /work/agy-{tag}.log 2>&1 | tail -20
echo '=== agents {tag}'
timeout 60 agy agents 2>&1 | head -60
echo '=== log {tag}'
grep -inE 'skill|agent|warn|error|invalid|fail' /work/agy-{tag}.log | grep -v -i 'migrat' | cut -c1-300 | head -60
echo"""


MCP_LIST = """
echo '=== mcp-list'
cd /work/proj
timeout 60 agy mcp list 2>&1 | head -30
echo '=== hooks-list'
timeout 120 agy --print "/hooks" --output-format json --dangerously-skip-permissions --print-timeout 60s 2>&1 | head -c 4000
"""


def container(cfg, prov_ip, script):
    return common.docker_base(cfg, prov_ip, setup(prov_ip) + script, tty=False)


def run(cfg, prov_ip):
    case = os.environ.get("STUDY_CASE", "skills-loc")
    extra = os.environ.get("STUDY_EXTRA", "")
    script = lib.case_script(
        case,
        turn,
        os.environ.get("STUDY_SKILL_DIR", "/work/home/.gemini/config/skills"),
        "/work/home/.gemini/config/agents",
        extra,
    )
    if case == "mcp-hooks":
        script += MCP_LIST
    path = os.path.join(cfg.outdir, "probe.sh")
    with open(path, "w") as f:
        f.write(script)
    os.environ["PROBE_SCRIPT"] = path
    run_with(cfg, prov_ip, container)
    lib.analyze(cfg.outdir)
