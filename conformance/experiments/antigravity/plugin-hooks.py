"""Whether Antigravity loads a plugin's root `hooks.json` and `agents.json`
(docs since 1.3.4; `agents.json` since 1.3.3), with no UZE in the plugin.

A hand-made plugin carries one named PreToolUse hook and one subagent; it
is installed with `agy plugin install <dir>`, then a headless turn runs
with `--log-file`. Evidence: the vendor's own log lines about hooks and
agents, `agy plugin list --json`, and `/hooks` as JSON.

Run: python3 conformance/lab.py --harness antigravity --experiment antigravity/plugin-hooks
"""

import subprocess

from harnesses.antigravity.scenarios import agy_setup
from shared import common

PLUGIN = "/work/probe-plugin"

HOOKS = (
    '{"probe-hook": {"PreToolUse": [{"matcher": "run_command", "hooks": '
    '[{"type": "command", "command": "touch /work/probe-hook-ran"}]}]}}'
)
AGENT = "---\\nname: probe-agent\\ndescription: A probe subagent.\\n---\\nAnswer briefly.\\n"

FINAL = f"""
mkdir -p {PLUGIN}/agents
printf '%s' '{{"name":"probe-plugin","version":"0.1.0","description":"probe"}}' > {PLUGIN}/plugin.json
printf '%s' '{HOOKS}' > {PLUGIN}/hooks.json
printf '%b' '{AGENT}' > {PLUGIN}/agents/probe-agent.md
printf '%s' '{{"entries":[{{"path":"agents"}}]}}' > {PLUGIN}/agents.json
agy plugin install {PLUGIN} 2>&1 | tail -12
echo '===== plugin list'
agy plugin list --json 2>&1 | head -40
echo '===== turn'
# decision: experiment-isolation
agy --print "hello" --dangerously-skip-permissions --print-timeout 60s --log-file /work/agy.log >/dev/null 2>&1
grep -n -iE "hooks?[.]json|named hook|loaded .* hook|agents[.]json|subagent|probe" /work/agy.log | head -40
echo '===== /hooks'
agy --print "/hooks" --output-format json --print-timeout 30s 2>&1 | head -c 2500
echo
"""


def run(cfg, prov_ip):
    common.start_provider(cfg, "static")
    setup = agy_setup(cfg, prov_ip, include_mcp=False, final_cmd=FINAL, auth="apikey")
    r = subprocess.run(
        common.docker_base(cfg, prov_ip, setup, tty=False),
        capture_output=True,
        text=True,
        errors="replace",
        timeout=400,
    )
    out = r.stdout + r.stderr
    with open(f"{cfg.outdir}/plugin-hooks.out", "w") as f:
        f.write(out)
    print(out[-7000:], flush=True)
