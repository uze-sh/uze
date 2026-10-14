"""Whether OpenCode creates a session under an id the caller names, and
continues it when named again — the premise of moving its continuity from
read-back to assigned. Also whether a session is answered by id outside the
first page of the listing. No UZE is in the loop.

python3 conformance/lab.py --harness opencode --experiment opencode/session-assigned
"""

import os
import subprocess

from experiments.relaunch_probe import bindings_for
from shared.common import check, start_provider

CWD = "/work/session-assigned"
SESSION = "ses_uzelab0000000000000000001"
BEGIN = "UZE_PROBE_BEGIN"
END = "UZE_PROBE_END"

PRELUDE = f"""
mkdir -p {CWD} && cd {CWD} && git init -q -b main .
i=0; while [ $i -lt 2 ]; do timeout 120 opencode run "filler $i" >/dev/null 2>&1; i=$((i+1)); done
timeout 120 opencode run --session {SESSION} "first turn" >/dev/null 2>&1; echo "first-exit=$?"
i=0; while [ $i -lt 3 ]; do timeout 120 opencode run "later filler $i" >/dev/null 2>&1; i=$((i+1)); done
timeout 120 opencode run --session {SESSION} "second turn" >/dev/null 2>&1; echo "second-exit=$?"
echo {BEGIN}
echo "--- sessions in cwd"
opencode api GET "/api/session?directory={CWD}&limit=500" 2>/dev/null | grep -o '"id":"ses[^"]*"' | wc -l
opencode api GET /api/session/{SESSION} 2>&1 | head -c 2000; echo
echo "--- listing count"
opencode api GET /api/session 2>/dev/null | grep -o '"id":"ses[^"]*"' | wc -l
echo "--- listing has it"
opencode api GET /api/session 2>/dev/null | grep -c {SESSION}
echo {END}
"""


def run(cfg, prov_ip):
    bindings = bindings_for(cfg.harness)
    prov_ip = start_provider(cfg, "static")
    cmd = bindings.headless(cfg, prov_ip, PRELUDE, "second turn", CWD)
    cmd = [
        part.replace(
            "opencode run 'second turn'",
            f"opencode run --session {SESSION} 'second turn'",
        )
        for part in cmd
    ]
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=3000
    )
    out = proc.stdout + proc.stderr
    with open(os.path.join(cfg.outdir, "session-assigned.out"), "w") as f:
        f.write(out)
    probe = out[out.find(BEGIN) : out.find(END)] if BEGIN in out else ""
    print(probe, flush=True)
    check(
        "session-assigned-created",
        f'"id":"{SESSION}"' in probe,
        "the named session exists under its id",
    )
    # Two filler sessions, the named one, three more fillers: a second
    # `--session` run that made its own session would leave seven.
    in_cwd = (
        probe.split("--- sessions in cwd")[1].split()[0]
        if "--- sessions in cwd" in probe
        else ""
    )
    check(
        "session-assigned-continued",
        in_cwd == "6",
        f"{in_cwd} sessions in the directory after two runs under one id",
    )
    check(
        "session-assigned-by-id",
        '"id"' in probe.split("--- listing count")[0],
        "GET /api/session/<id> answers",
    )
