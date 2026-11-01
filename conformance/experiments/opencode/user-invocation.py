"""Exploration: what does OpenCode record when the *person* invokes a skill,
an agent or a command from the prompt, and what does a tracker riding a
plugin cost a turn?

`opencode/usage-trace` measured the model's own uses (tools `skill`,
`subagent`, `execute`). This measures the rest of what a usage count
needs, no UZE in the loop beyond provisioning:

1. a skill named by the person: `@<skill>` and `/<skill>` typed into
   `opencode run`, and the prompt's structured `skills` list (what the TUI
   sends once a `@` completion is taken), through `opencode api`;
2. an agent named by the person: `@<agent>` typed, the structured `agents`
   list, and a primary agent chosen with `--agent`;
3. a command of the person's own (`commands/<name>.md`) whose template
   names the skill, typed as `/<name>` and sent through `session.command`;
4. an MCP server configured by hand in `opencode.json` (not by a UZE
   plugin): how its tool is named to a plugin's tool hook;
5. `opencode stats` with sessions in two projects: machine-wide or not;
6. the cost of a plugin's `execute.after` handler: whether the harness
   awaits it before the next step, with a handler that sleeps and one that
   only appends a line.

A logger plugin in the global plugin directory records every
`ctx.tool.hook` call, the session `context` hook and the whole
`ctx.event.subscribe()` stream (deltas left out). Each session is read
back with `opencode session export`. OpenCode's database is never opened.
The TUI turns are typed and their completions taken the way a person does.

Finding (2026-10-31, OpenCode v2.2.5):

1. A skill is named by the person with `@<skill>`, taken from the `@`
   completion (skills listed as kind `skill`, beside agents and files);
   `/<skill>` matches no command. The prompt then carries a structured
   list: `session.inbox.enqueued` (event stream) has
   `item.payload.skills[{id, name, text, mention}]`, `text` being the
   skill's body inlined, and the export's `user` message the same
   `skills[]`. No tool runs, so the tool hook never sees it; a model that
   also calls `skill` on the same turn would be a second use of it.
   `session.prompt` admits the same `skills`/`agents` lists over the API.
   `opencode run` does not parse mentions: `@name` stays plain text.
2. An agent named with `@<agent>` (subagents only in the completion) lands
   as `payload.agents[{name, mention}]`, in the stream and the export; it
   asks the model to dispatch, the dispatch itself being the `subagent`
   tool. A primary agent chosen at launch (`--agent`) is the export's
   `info.agent`, an `agent-switched` message and each `assistant.agent`,
   and every tool hook event's `agent`.
3. A command of the person's own (`/<cmd>`) arrives expanded: the
   template's text is the user message, and the command's name reaches no
   event and no export. A skill its template names is text until the
   model calls `skill`.
4. A hand-written MCP server (`mcp.servers.<key>` in `opencode.json`) is
   offered as `tools["<key>"]` in Code Mode and reaches the tool hook as
   `<key>_<tool>` (`direct-probe_uze_conformance`); a UZE-delivered one's
   `@` becomes `_`. The server is recovered by matching the configured
   keys, as both key and tool may hold `_`. In a fresh process the first
   `execute` can run before the server connects ("Unknown tool"), with
   `execute` still `completed`: a use is the nested `<key>_<tool>` call,
   never the `execute`. (`opencode mcp list` printed "No MCP servers
   configured" for the 2.x `mcp.servers` shape.)
5. `opencode stats` with no `--project` is machine-wide from any
   directory (9 sessions across three projects; `--project .` 6 + 1 + 2).
6. The tool hook is awaited: a 300 ms `execute.after` handler added
   6.8 s to 22 calls (2.7 s against 9.5 s), so it is on the turn's
   critical path. A handler appending one line with `appendFileSync`
   costs 0.05 ms mean, 0.12 ms max; a tracker records synchronously and
   defers anything slower without awaiting it.

Run: python3 conformance/lab.py --harness opencode --experiment opencode/user-invocation --discovery
"""

import json
import re
import subprocess
import time

import pexpect

from harnesses.opencode.scenarios import opencode_setup
from shared import common
from shared.common import make_screen, make_waiter

SKILL = "probe-skill"
AGENT = "probe-agent"
PRIMARY = "probe-primary"
COMMAND = "probe-cmd"
SKILL_MARK = "UZE_UI_SKILL_DESC_4b2e"
SKILL_BODY = "UZE_UI_SKILL_BODY_91fd"
CONFIG = "/work/home/.config/opencode"
LOG = "/work/probe.jsonl"
DIRECT = "direct-probe"
UZE_NAMESPACE = "uze-mcp-conformance_uze-lab-uze-conformance"
TRIGGER = "UZE_UI_TOOLS"
PERF_CALLS = 20

SEQUENCE = [
    {
        "name": "execute",
        "args": json.dumps(
            {"code": f'return await tools["{UZE_NAMESPACE}"].uze_conformance({{}});'}
        ),
    },
    {
        "name": "execute",
        "args": json.dumps(
            {"code": f'return await tools["{DIRECT}"].uze_conformance({{}});'}
        ),
    },
    *[
        {"name": "read", "args": json.dumps({"path": "/work/pc/readme.txt"})}
        for _ in range(PERF_CALLS)
    ],
]

PLUGIN = (
    r"""// No import of `@opencode-ai/plugin`: 2.x cannot resolve it from the
// global plugin directory (measured by `opencode/session-start`).
import { appendFileSync } from "node:fs";
const LOG = "%s";
const SLEEP = Number(process.env.PROBE_SLEEP || 0);
const cut = (v) => {
  try {
    const s = JSON.stringify(v);
    return s && s.length > 1500 ? s.slice(0, 1500) + "..." : s;
  } catch (e) {
    return String(v);
  }
};
const rec = (kind, data) => {
  try {
    appendFileSync(LOG, JSON.stringify({ t: Date.now(), kind, data: cut(data) }) + "\n");
  } catch (e) {}
};
export default {
  id: "invocation-probe",
  async setup(ctx) {
    rec("setup", Object.keys(ctx));
    await ctx.tool.hook("execute.before", async (e) => {
      rec("tool.before", { tool: e.tool, input: e.input, agent: e.agent, sessionID: e.sessionID });
    });
    await ctx.tool.hook("execute.after", async (e) => {
      const t0 = performance.now();
      rec("tool.after", { tool: e.tool, input: e.input, agent: e.agent, sessionID: e.sessionID, status: e.status });
      if (SLEEP) await new Promise((r) => setTimeout(r, SLEEP));
      rec("tool.after.cost", performance.now() - t0);
    });
    try {
      await ctx.session.hook("context", async (e) => {
        rec("session.context", Object.keys(e ?? {}));
      });
    } catch (err) {
      rec("context-error", String(err));
    }
    (async () => {
      let stream = ctx.event.subscribe();
      if (stream?.then) stream = await stream;
      for await (const ev of stream) {
        if (String(ev?.type).endsWith(".delta")) continue;
        rec("event", { type: ev?.type, data: ev?.data });
      }
    })().catch((err) => rec("stream-error", String(err)));
  },
};
"""
    % LOG
)

SID = """node -e 'const l=require("fs").readFileSync(process.argv[1],"utf8").split("\\n").filter(Boolean).map(s=>{try{return JSON.parse(s)}catch(e){return {}}}).map(e=>e.sessionID||e.properties?.sessionID||e.part?.sessionID||e.data?.sessionID).filter(Boolean);console.log(l.length?l[0]:"")'"""


def prelude():
    """The person's world: a skill, two agents, a command, a hand-written
    MCP entry and the logger plugin, in four committed projects."""
    return f"""
set +e
mkdir -p {CONFIG}/skills/{SKILL} {CONFIG}/agents {CONFIG}/plugins {CONFIG}/commands /work/pa /work/pb /work/pc
cat > {CONFIG}/skills/{SKILL}/SKILL.md <<'EOF'
---
name: {SKILL}
description: Use this skill when the user asks for the probe things. {SKILL_MARK}
---

Do the probe things. {SKILL_BODY}
EOF
cat > {CONFIG}/agents/{AGENT}.md <<'EOF'
---
description: Probes things when asked.
mode: subagent
---

You probe things.
EOF
cat > {CONFIG}/agents/{PRIMARY}.md <<'EOF'
---
description: A primary agent for the probe.
mode: primary
---

You are the primary probe.
EOF
cat > {CONFIG}/commands/{COMMAND}.md <<'EOF'
---
description: Runs the probe through the skill.
---

Use the {SKILL} skill on: $ARGUMENTS
EOF
cat > {CONFIG}/plugins/invocation-probe.ts <<'EOF'
{PLUGIN}EOF
echo probe > /work/pc/readme.txt
# A hand-written MCP entry: the same server UZE delivered, under a key of
# the person's own.
node -e '
const fs=require("fs");
const p="{CONFIG}/opencode.json";
const d=JSON.parse(fs.readFileSync(p,"utf8"));
const servers=d.mcp.servers;
const found=Object.keys(servers).find((k) => k.includes("uze-conformance"));
servers["{DIRECT}"]=JSON.parse(JSON.stringify(servers[found]));
fs.writeFileSync(p, JSON.stringify(d,null,1));
'
for d in pa pb pc tui; do mkdir -p /work/$d; (cd /work/$d && git init -q . && echo $d > id.txt && git add id.txt && git -c user.email=lab@uze -c user.name=lab commit -qm "$d"); done
"""


def script():
    # decision: headless-permissions
    run = "timeout 150 opencode run --standalone --auto --format json"
    return (
        prelude()
        + f"""
echo "===== config"
node -e 'const d=JSON.parse(require("fs").readFileSync("{CONFIG}/opencode.json","utf8"));delete d.providers;console.log(JSON.stringify(d))'
stamp() {{ echo "== $1 $(date +%s%3N)" >> {LOG}; }}
turn() {{
  name=$1; shift
  stamp "begin $name"
  start=$(date +%s%3N)
  "$@" > /work/$name.out 2>&1
  code=$?
  end=$(date +%s%3N)
  stamp "end $name"
  sid=$({SID} /work/$name.out)
  echo "turn=$name exit=$code ms=$((end-start)) sid=$sid" >> /work/turns.txt
  echo "$name $sid" >> /work/sessions.txt
}}
# decision: headless-permissions
cd /work/pa
turn skill_at {run} "@{SKILL} UZE_UI_SKILL_AT"
turn skill_slash {run} "/{SKILL} UZE_UI_SKILL_SLASH"
turn agent_at {run} "@{AGENT} UZE_UI_AGENT_AT"
turn primary {run} --agent {PRIMARY} "UZE_UI_PRIMARY"
turn command_slash {run} "/{COMMAND} UZE_UI_COMMAND_SLASH"
cd /work/pb
turn mcp {run} "{TRIGGER} UZE_UI_MCP"
cd /work/pc
export PROBE_SLEEP=0; turn perf_fast {run} "{TRIGGER} UZE_UI_PERF_FAST"
export PROBE_SLEEP=300; turn perf_slow {run} "{TRIGGER} UZE_UI_PERF_SLOW"

echo "===== serve help"
opencode serve --help 2>&1 | sed -n '1,30p'
cd /work/pa
export OPENCODE_PASSWORD=uze-lab-password
(opencode serve --port 4199 > /work/serve.out 2>&1 &)
for i in $(seq 1 60); do node -e 'fetch("http://127.0.0.1:4199/").then(()=>process.exit(0),()=>process.exit(1))' && break; sleep 0.5; done
export PROBE_SLEEP=0
API="opencode api --server http://127.0.0.1:4199"
echo "===== api"
$API command.list 2>&1 | head -c 3000; echo
$API skill.list 2>&1 | head -c 1500; echo
created=$($API session.create -d '{{}}' 2>&1); echo "create: $created" | head -c 1500; echo
asid=$(echo "$created" | node -e 'let s="";process.stdin.on("data",c=>s+=c).on("end",()=>{{try{{const d=JSON.parse(s);console.log(d.id||d.data?.id||"")}}catch(e){{console.log("")}}}})')
echo "api-sid=$asid"
echo "api_prompt $asid" >> /work/sessions.txt
stamp "begin api_prompt"
$API session.prompt --param sessionID=$asid -d '{{"text":"UZE_UI_API_PROMPT","skills":[{{"id":"{SKILL}"}}],"agents":[{{"name":"{AGENT}"}}]}}' 2>&1 | head -c 2000; echo
sleep 5
stamp "end api_prompt"
stamp "begin api_command"
$API session.command --param sessionID=$asid -d '{{"name":"{COMMAND}","text":"UZE_UI_API_COMMAND"}}' 2>&1 | head -c 2000; echo
sleep 5
stamp "end api_command"

echo "===== mcp list"
(cd /work/pb && timeout 60 opencode mcp list 2>&1 | head -c 2000)
echo "===== turns"
cat /work/turns.txt
echo "===== runout"
for f in skill_slash command_slash; do echo "-- $f"; head -c 1500 /work/$f.out; echo; done
echo "===== probe"
cat {LOG}
echo "===== stats"
for d in /work /work/pa /work/pb /work/pc; do
  echo "-- from $d"; (cd $d && timeout 60 opencode stats --days 0 --json 2>&1 | head -c 300); echo
done
for d in pa pb pc; do echo "-- from /work/$d --project ."; (cd /work/$d && timeout 60 opencode stats --days 0 --project . --json 2>&1 | head -c 400); echo; done
echo "===== exports"
while read name sid; do
  [ -n "$sid" ] || continue
  echo "-- $name $sid"
  (cd /work/pa && timeout 60 opencode session export "$sid" 2>&1 | head -c 12000)
  echo
done < /work/sessions.txt
echo "===== end"
"""
    )


def section(out, name):
    start = out.find(f"===== {name}")
    if start < 0:
        return ""
    rest = out[start + len(name) + 6 :]
    end = rest.find("\n===== ")
    return rest if end < 0 else rest[:end]


def between(probe, name):
    """The probe log's lines inside the turn `name`."""
    begin = probe.find(f"== begin {name} ")
    end = probe.find(f"== end {name} ")
    return probe[begin:end] if begin >= 0 and end > begin else ""


TUI_TURNS = [
    # (name, typed before the completion is taken, typed after it)
    ("tui_skill", f"@{SKILL[:-2]}", " UZE_UI_TUI_SKILL"),
    ("tui_command", f"/{COMMAND[:-2]}", " UZE_UI_TUI_COMMAND"),
    ("tui_agent", f"@{AGENT[:-2]}", " UZE_UI_TUI_AGENT"),
]


def _exec(cfg, command, timeout=90):
    return subprocess.run(
        ["docker", "exec", cfg.harness_container, "sh", "-c", command],
        capture_output=True,
        text=True,
        errors="replace",
        timeout=timeout,
    ).stdout


def _type(child, text):
    for character in text:
        child.send(character)
        time.sleep(0.05)


def _settled(cfg, count, tries=40):
    """Waits until the probe has seen `count` executions succeed."""
    for _ in range(tries):
        seen = _exec(
            cfg, f"grep -c 'session.execution.succeeded' {LOG} 2>/dev/null"
        ).strip()
        if seen.isdigit() and int(seen) >= count:
            return True
        time.sleep(1.5)
    return False


def tui_phase(cfg, prov_ip):
    """The person's own gestures in the TUI: a `@` completion of a skill
    and of an agent, and a `/` command, each taken from the list."""
    final = prelude() + "\ncd /work/tui && exec opencode --standalone\n"
    cmd = common.docker_base(
        cfg, prov_ip, opencode_setup(cfg, prov_ip, final, plugins="flow mcp-plugin")
    )
    child = pexpect.spawn(
        cmd[0], cmd[1:], encoding="utf-8", codec_errors="replace", timeout=300
    )
    child.setwinsize(50, 160)
    screen = make_screen(child)
    wait_for = make_waiter(screen)
    boot, _, marker = wait_for(["Ask anything"], tries=16, stop_on_death=True)
    common.check("tui-boots", marker == "Ask anything", boot[-200:].replace("\n", " "))
    # Plugins finish loading after the prompt renders, or typed input is lost.
    time.sleep(25)
    screens = {}
    done = 0
    for name, before, after in TUI_TURNS:
        _type(child, before)
        time.sleep(2)
        screens[f"{name}-completion"] = common.render_screen(screen(wait=1.5))
        child.send("\r")
        time.sleep(1.5)
        _type(child, after)
        time.sleep(1)
        screens[f"{name}-typed"] = common.render_screen(screen(wait=1.0))
        child.send("\r")
        done += 1
        common.check(f"{name}-settles", _settled(cfg, done), name)
        time.sleep(2)
    _type(child, f"/{SKILL[:-2]}")
    time.sleep(2)
    screens["slash-skill-completion"] = common.render_screen(screen(wait=1.5))
    probe = _exec(cfg, f"cat {LOG}")
    sids = sorted(set(re.findall(r"(ses_[A-Za-z0-9]+)", probe)))
    exports = "".join(
        f"-- {sid}\n"
        + _exec(
            cfg,
            f"cd /work/tui && PATH=$PATH:/usr/local/.opencode/bin HOME=/work/home timeout 60 opencode session export {sid} 2>&1 | head -c 20000",
        )
        + "\n"
        for sid in sids
    )
    child.terminate(force=True)
    with open(f"{cfg.outdir}/tui_screens.txt", "w") as f:
        for key, value in screens.items():
            f.write(f"===== {key}\n{value}\n")
    with open(f"{cfg.outdir}/tui_probe.jsonl", "w") as f:
        f.write(probe)
    with open(f"{cfg.outdir}/tui_exports.txt", "w") as f:
        f.write(exports)
    compact_probe = probe.replace("\\", "")
    common.check(
        "tui-skill-mention-reaches-the-event-stream",
        '"skills":[{' in compact_probe and SKILL in compact_probe,
        "session.inbox.enqueued carries the prompt's skills list",
    )
    common.check(
        "tui-skill-mention-is-in-the-export",
        '"skills"' in exports and SKILL in exports,
        "the user message's skills list names it",
    )
    common.check(
        "tui-agent-mention-reaches-the-event-stream",
        '"agents":[{' in compact_probe and AGENT in compact_probe,
        "the prompt's agents list names it",
    )
    common.check(
        "tui-command-arrives-expanded-without-its-name",
        f"Use the {SKILL} skill on:" in compact_probe
        and COMMAND not in compact_probe
        and COMMAND not in exports,
        "the template's text reaches the stream and the export; the command's name reaches neither",
    )


def run(cfg, prov_ip):
    prov_ip = (
        common.start_provider(
            cfg,
            "toolcall",
            {
                "TOOL_SEQUENCE": json.dumps(SEQUENCE),
                "TOOL_TRIGGER": TRIGGER,
                "DISCOVERY": "1",
            },
        )
        or prov_ip
    )
    r = subprocess.run(
        common.docker_base(
            cfg,
            prov_ip,
            opencode_setup(cfg, prov_ip, script(), plugins="flow mcp-plugin"),
            tty=False,
        ),
        capture_output=True,
        text=True,
        errors="replace",
        timeout=1500,
    )
    out = r.stdout
    with open(f"{cfg.outdir}/user_invocation.out", "w") as f:
        f.write(out)
        f.write("\n=== stderr\n" + r.stderr)
    print(out[-4000:], flush=True)

    probe = section(out, "probe")
    turns = section(out, "turns")
    common.check("probe-plugin-loaded", '"kind":"setup"' in probe, "setup recorded")
    for name in ("skill_at", "skill_slash", "agent_at", "primary", "command_slash"):
        common.check(f"turn-{name}-recorded", f"turn={name}" in turns, turns[:200])

    common.check(
        "run-does-not-parse-mentions",
        "UZE_UI_SKILL_AT" in between(probe, "skill_at")
        and '\\"skills\\"' not in between(probe, "skill_at")
        and '\\"agents\\"' not in between(probe, "agent_at"),
        "opencode run sends `@name` as plain text, with no skills or agents list",
    )
    common.check(
        "hand-written-mcp-server-reaches-the-tool-hook-by-its-key",
        f'\\"tool\\":\\"{DIRECT}_uze_conformance\\"' in probe,
        "the MCP tool reaches the hook as <config key>_<tool>",
    )
    stats = section(out, "stats")
    counts = re.findall(
        r'-- from (\S+)( --project \.)?\s*\{[\s\S]*?"sessions": (\d+)', stats
    )
    machine = {d: int(n) for d, flag, n in counts if not flag}
    scoped = {d: int(n) for d, flag, n in counts if flag}
    common.check(
        "stats-without-project-are-machine-wide",
        len(set(machine.values())) == 1
        and scoped
        and sum(scoped.values()) == next(iter(machine.values()), -1),
        f"unscoped {machine}; --project . {scoped}",
    )
    common.check(
        "api-prompt-carries-skills-and-agents",
        '"skills":[{"id":"probe-skill"' in section(out, "api")
        and '"agents":[{"name":"probe-agent"' in section(out, "api"),
        "session.prompt admits the structured lists the TUI sends",
    )

    ms = {m[0]: int(m[1]) for m in re.findall(r"turn=(\w+) exit=\d+ ms=(\d+)", turns)}
    if "perf_fast" in ms and "perf_slow" in ms:
        delta = ms["perf_slow"] - ms["perf_fast"]
        common.check(
            "tool-hook-is-awaited",
            delta > PERF_CALLS * 300 * 0.6,
            f"fast={ms['perf_fast']}ms slow={ms['perf_slow']}ms delta={delta}ms over {PERF_CALLS + 2} calls",
        )
    costs = [
        float(c)
        for c in re.findall(r'"kind":"tool.after.cost","data":"([0-9.]+)"', probe)
    ]
    fast = between(probe, "perf_fast")
    fast_costs = [
        float(c)
        for c in re.findall(r'"kind":"tool.after.cost","data":"([0-9.]+)"', fast)
    ]
    if fast_costs:
        common.check(
            "append-handler-cost",
            max(fast_costs) < 50,
            f"n={len(fast_costs)} max={max(fast_costs):.3f}ms mean={sum(fast_costs) / len(fast_costs):.3f}ms (all={len(costs)})",
        )
    tui_phase(cfg, prov_ip)
