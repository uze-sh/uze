"""Where Codex discovers skills/agents and which frontmatter it tolerates —
no UZE in the loop; see `experiments.claude.study_discovery_lib`.

STUDY_CASE=skills-loc [STUDY_CODEX_TRUST=1] AGY_OUTDIR=... python3 conformance/lab.py \\
    --harness codex --experiment codex/study_discovery
"""

import os

from experiments.agent_probe import run_with
from experiments.claude import study_discovery_lib as lib
from shared import common

SETUP = """
set -e
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin
export HOME=/work/home CODEX_HOME=/work/home/.codex
export OPENAI_API_KEY=uze-conformance-invalid-by-design
export CODEX_CA_CERTIFICATES=/app/ca.crt
export SSL_CERT_FILE=/app/ca.crt
mkdir -p /work/home/.codex
cp /app/fixtures/auth.json /work/home/.codex/auth.json
cat > /work/home/.codex/config.toml <<'TOML'
[features]
hooks = true
TOML
"""

TRUST = """
cat >> /work/home/.codex/config.toml <<'TOML'
[projects."/work/proj"]
trust_level = "trusted"
TOML
"""


def turn(tag):
    return f"""echo '=== turn {tag}'
timeout 200 codex exec --dangerously-bypass-hook-trust --skip-git-repo-check "{lib.turn_marker(tag)} hello" 2>&1 | tail -40
echo '=== prompt-input {tag}'
timeout 60 codex debug prompt-input "{lib.turn_marker(tag)}_PI" 2>&1 | grep -oE 'STUDY[A-Z]*_[A-Za-z0-9_]+|"[^"]*SKILL.md' | sort | uniq -c
echo"""


def container(cfg, prov_ip, script):
    cmd = common.docker_base(cfg, prov_ip, SETUP + script, tty=False)
    ca_crt, _, _ = common.generate_certs(cfg)
    i = cmd.index(common.HARNESS_IMAGE)
    return cmd[:i] + ["-v", f"{ca_crt}:/app/ca.crt:ro"] + cmd[i:]


def run(cfg, prov_ip):
    case = os.environ.get("STUDY_CASE", "skills-loc")
    extra = (TRUST if os.environ.get("STUDY_CODEX_TRUST") else "") + os.environ.get(
        "STUDY_EXTRA", ""
    )
    script = lib.case_script(
        case, turn, "/work/home/.agents/skills", "/work/home/.codex/agents", extra
    )
    if case == "mcp-hooks":
        script += "\necho '=== mcp-list'; cd /work/proj && timeout 60 codex mcp list 2>&1 | head -30\n"
    path = os.path.join(cfg.outdir, "probe.sh")
    with open(path, "w") as f:
        f.write(script)
    os.environ["PROBE_SCRIPT"] = path
    run_with(cfg, prov_ip, container)
    lib.analyze(cfg.outdir)
