"""Exploration: how can the machine Overview list OpenCode's sessions that
changed since a time, and read one back, without opening the harness's
database — and what does each answer cost?

Sessions in three projects, one of them long (forty tool calls) and one
dispatching a subagent, all made with `opencode run`. Then, from a directory
that is no project, the harness's own CLI is asked: its `session` help,
`session list` in JSON, and `session export` of the shortest and the longest
session, each timed. OpenCode's database is never opened.

Finding (2026-10-31, OpenCode v2.2.5):

- `opencode session list --format json` lists the top-level sessions of
  the project of the directory it is run from (from `/tmp`: `[]`; from each
  project: that project's only), newest first, at most 100 by default. No
  use for a machine-wide reading.
- `opencode api session.list`, run from `/tmp` against the background
  service, answers `{"data": [...]}` with every session of the machine:
  4 sessions across 3 projects, the subagent's included (`parentID`). Each
  carries `id`, `projectID`, `time.{created,updated}` (ms) and
  `location.directory`. `--param limit=N` is honoured (2 gave 2), so the
  answer is bounded; no default was hit with 4 sessions.
- Costs, in the container: `api session.list` 151–159 ms; `session list`
  691 ms cold, 104 ms warm; `session export` 86–97 ms per session, whether
  1.7 KB or 61 KB (a 40-call session).
- An export's tool parts carry `state.metadata.toolCalls[{tool, status}]`
  for Code Mode's nested calls, `tool` being `<server>.<tool>`.

Run: python3 conformance/lab.py --harness opencode --experiment opencode/session-listing
"""

import json
import re
import subprocess

from harnesses.opencode.scenarios import opencode_setup
from shared import common

CONFIG = "/work/home/.config/opencode"
AGENT = "probe-agent"
LONG = 40
TRIGGER = "UZE_SL_TURN"

SEQUENCE = [
    {
        "name": "subagent",
        "args": json.dumps(
            {"agent": AGENT, "description": "probe", "prompt": "probe the agent"}
        ),
    },
    *[
        {"name": "read", "args": json.dumps({"path": "/work/pc/readme.txt"})}
        for _ in range(LONG)
    ],
]


def script():
    # decision: headless-permissions
    run = "timeout 240 opencode run --standalone --auto --format json"
    return f"""
set +e
mkdir -p {CONFIG}/agents /work/pa /work/pb /work/pc
cat > {CONFIG}/agents/{AGENT}.md <<'EOF'
---
description: Probes things when asked.
mode: subagent
---

You probe things.
EOF
for d in pa pb pc; do (cd /work/$d && git init -q . && echo $d > readme.txt && git add readme.txt && git -c user.email=lab@uze -c user.name=lab commit -qm "$d"); done
(cd /work/pa && {run} "hello short" </dev/null > /work/pa.out 2>&1)
(cd /work/pb && {run} "hello again" </dev/null > /work/pb.out 2>&1)
(cd /work/pc && {run} "{TRIGGER} the long one" </dev/null > /work/pc.out 2>&1)
cd /tmp
ms() {{ date +%s%3N; }}
echo "===== help"
opencode session --help 2>&1 | head -60
opencode session list --help 2>&1 | head -60
echo "===== list"
t0=$(ms); timeout 60 opencode session list --format json > /work/list.json 2>/work/list.err; t1=$(ms)
echo "list-exit=$? list_ms=$((t1 - t0))"
head -c 6000 /work/list.json; echo; head -c 600 /work/list.err
t0=$(ms); timeout 60 opencode session list --format json > /dev/null 2>&1; t1=$(ms)
echo "list_warm_ms=$((t1 - t0))"
echo "===== probes"
for d in pa pb pc; do echo "-- list from /work/$d"; (cd /work/$d && timeout 60 opencode session list --format json 2>&1 | head -c 800); echo; done
opencode api --help 2>&1 | head -80
t0=$(ms); timeout 60 opencode api session.list > /work/api-list.json 2>&1; t1=$(ms)
echo "api-list-exit=$? api_list_ms=$((t1 - t0))"; head -c 4000 /work/api-list.json; echo
timeout 60 opencode api session.list --help 2>&1 | head -40
timeout 60 opencode api project.list 2>&1 | head -c 2000; echo
timeout 60 opencode api experimental.session.list 2>&1 | head -c 2000; echo
node -e 'const d=JSON.parse(require("fs").readFileSync("/work/api-list.json","utf8")).data;console.log("api-list sessions="+d.length+" projects="+new Set(d.map(x=>x.projectID)).size+" children="+d.filter(x=>x.parentID).length+" keys="+Object.keys(d[0]).join(","))'
for p in "limit=2" "limit=100000"; do echo "-- session.list --param $p"; timeout 60 opencode api session.list --param $p 2>&1 | head -c 1500; echo; done
echo "===== exports"
for sid in $(cat /work/*.out /work/api-list.json 2>/dev/null | grep -o 'ses_[A-Za-z0-9]*' | sort -u); do
  t0=$(ms); timeout 60 opencode session export "$sid" > /work/export-$sid.json 2>&1; t1=$(ms)
  echo "export $sid bytes=$(wc -c < /work/export-$sid.json) ms=$((t1 - t0)) parent=$(node -e 'try{{const d=JSON.parse(require("fs").readFileSync("/work/export-'$sid'.json","utf8"));console.log(d.info.parentID||d.info.parent||"-")}}catch(e){{console.log("unparsed")}}')"
done
echo "===== end"
"""


def section(out, name):
    start = out.find(f"===== {name}")
    if start < 0:
        return ""
    rest = out[start + len(name) + 6 :]
    end = rest.find("\n===== ")
    return rest if end < 0 else rest[:end]


def run(cfg, prov_ip):
    prov_ip = (
        common.start_provider(
            cfg,
            "toolcall",
            {
                "TOOL_SEQUENCE": json.dumps(SEQUENCE),
                "TOOL_TRIGGER": TRIGGER,
            },
        )
        or prov_ip
    )
    r = subprocess.run(
        common.docker_base(
            cfg, prov_ip, opencode_setup(cfg, prov_ip, script(), plugins=""), tty=False
        ),
        capture_output=True,
        text=True,
        errors="replace",
        timeout=900,
    )
    out = r.stdout
    with open(f"{cfg.outdir}/session_listing.out", "w") as f:
        f.write(out)
        f.write("\n=== stderr\n" + r.stderr)
    print(out[-9000:], flush=True)

    probes = section(out, "probes")
    exports = section(out, "exports")
    machine = re.search(r"api-list sessions=(\d+) projects=(\d+) children=(\d+)", probes)
    exported = re.findall(r"export (ses_\S+) bytes=(\d+) ms=(\d+) parent=(\S+)", exports)
    common.check(
        "session-list-is-per-project",
        all(
            len(set(re.findall(r'"projectId": "([0-9a-f]+)"', block))) == 1
            for block in probes.split("-- list from ")[1:4]
        ),
        "each project's `session list` names that project alone",
    )
    common.check(
        "api-session-list-is-machine-wide",
        bool(machine) and int(machine.group(2)) >= 3 and int(machine.group(3)) >= 1,
        machine.group(0) if machine else "no api listing",
    )
    big = probes.split("-- session.list --param limit=100000", 1)[-1]
    common.check(
        "a-large-limit-is-accepted",
        '"data":[' in big and big.count('"id":"ses_') >= 3,
        "the reader's own limit answers every session",
    )
    common.check(
        "every-listed-session-exports",
        bool(exported) and all(int(b) > 100 for _, b, _, _ in exported),
        "; ".join(f"{s[:12]} {b}B {m}ms parent={p}" for s, b, m, p in exported),
    )
