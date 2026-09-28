"""Where OpenCode discovers skills/agents and which frontmatter it
tolerates — no UZE in the loop; see `experiments.claude.study_discovery_lib`.

STUDY_CASE=skills-loc AGY_OUTDIR=... python3 conformance/lab.py --harness opencode \\
    --experiment opencode/study_discovery
"""

import os

from experiments.agent_probe import run_with
from experiments.claude import study_discovery_lib as lib
from shared import common


def setup(prov_ip):
    return f"""
set -e
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin:/usr/local/.opencode/bin
export HOME=/work/home
export OPENCODE_DISABLE_MODELS_FETCH=1
export UZE_CONFORMANCE_KEY=dummy
mkdir -p /work/home/.config/opencode
cat > /work/home/.config/opencode/opencode.json <<'JSON'
{{"providers":{{"uze-conformance":{{"name":"UZE Conformance","env":["UZE_CONFORMANCE_KEY"],"package":"@opencode-ai/ai/providers/openai-compatible","settings":{{"baseURL":"http://{prov_ip}:9999/v1","apiKey":"{{env:UZE_CONFORMANCE_KEY}}"}},"models":{{"uze-model":{{"modelID":"uze-model","name":"UZE Conformance Model"}}}}}}}},
 "model":"uze-conformance/uze-model","agents":{{"build":{{"model":"uze-conformance/uze-model"}}}}}}
JSON
"""


def turn(tag):
    return f"""echo '=== turn {tag}'
pkill -x opencode 2>/dev/null; rm -f /work/home/.config/opencode/service.json
timeout 200 opencode run --standalone --auto "{lib.turn_marker(tag)} hello" </dev/null 2>&1 | tail -20
echo '=== debug-agents {tag}'
timeout 60 opencode debug agents >/dev/null 2>&1; sleep 6
timeout 60 opencode debug agents 2>&1 | head -c 60000
pkill -x opencode 2>/dev/null; rm -f /work/home/.config/opencode/service.json
echo"""


MCP_LIST = """
echo '=== mcp-list'
cd /work/proj
pkill -x opencode 2>/dev/null; rm -f /work/home/.config/opencode/service.json
timeout 60 opencode mcp list 2>&1 | head -30
sleep 5
timeout 60 opencode mcp list 2>&1 | head -30
echo '=== debug-config'
timeout 60 opencode debug config 2>&1 | head -60
"""


def container(cfg, prov_ip, script):
    return common.docker_base(cfg, prov_ip, setup(prov_ip) + script, tty=False)


def run(cfg, prov_ip):
    case = os.environ.get("STUDY_CASE", "skills-loc")
    script = lib.case_script(
        case,
        turn,
        "/work/home/.config/opencode/skills",
        "/work/home/.config/opencode/agents",
    )
    if case == "mcp-hooks":
        script += MCP_LIST
    path = os.path.join(cfg.outdir, "probe.sh")
    with open(path, "w") as f:
        f.write(script)
    os.environ["PROBE_SCRIPT"] = path
    run_with(cfg, prov_ip, container)
    lib.analyze(cfg.outdir)
