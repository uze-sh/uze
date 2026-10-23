"""Observation experiment: can a global plugin give OpenCode each project's
own `.agents/agents`, through the shared background service?

`experiments/opencode/project-agents` found no launch can carry project
agents from outside the checkout: the environment belongs to whichever
process started the service. This asks the other channel. OpenCode V2's
agent registry is a per-location service (`makeLocationNode`), and a
plugin is handed that location (`ctx.location`) and the registry
(`ctx.agent.transform`, `ctx.agent.reload`). A plugin in the global plugins
directory, which UZE already owns for hooks and agent context, could then
read `<project>/.agents/agents/*.md` and register them for that project
alone.

The plugin records every setup and transform it runs (pid, location), so a
wrong guess about when it runs is evidence, not silence.

Two projects, `a` and `b`, each with one agent whose description is a
marker; turns run with `opencode run` against the shared service, never
`--standalone`:

1. a turn in `a`, then one in `b`: each project offers its own agent and
   not the other's;
2. a turn from `a/sub`: the plugin walks up to the repository root;
3. an agent written into `b` while the service is up, then a turn in `b`:
   the plugin's watch reloads the registry without a restart;
4. a turn in `b` whose model calls the `subagent` tool with that late
   agent: the subagent runs, and its own request carries the body the
   plugin registered as its system prompt.

Run:
  python3 conformance/lab.py --harness opencode --experiment opencode/project-agents-plugin
"""

import json
import re
import subprocess

from harnesses.opencode.scenarios import opencode_setup
from shared import common

DEBUG = "/work/plugin-debug.jsonl"

PLUGIN = r"""import { appendFileSync, existsSync, readdirSync, readFileSync, watch } from "node:fs";
import { dirname, join } from "node:path";

const DEBUG = "__DEBUG__";
const log = (o) => { try { appendFileSync(DEBUG, JSON.stringify({ pid: process.pid, ...o }) + "\n"); } catch {} };

// The nearest `.agents/agents` at or above the location, stopping at the
// repository root so one project never reads its parent's.
function agentsRoot(start) {
  for (let dir = start; ; dir = dirname(dir)) {
    const candidate = join(dir, ".agents", "agents");
    if (existsSync(candidate)) return candidate;
    if (existsSync(join(dir, ".git")) || dirname(dir) === dir) return null;
  }
}

function parse(text) {
  const match = /^---\n([\s\S]*?)\n---\n?([\s\S]*)$/.exec(text);
  if (!match) return { fields: {}, body: text };
  const fields = {};
  for (const line of match[1].split("\n")) {
    const kv = /^([A-Za-z_-]+):\s*(.*)$/.exec(line);
    if (kv) fields[kv[1]] = kv[2];
  }
  return { fields, body: match[2] };
}

function load(root) {
  if (!root || !existsSync(root)) return [];
  return readdirSync(root)
    .filter((name) => name.endsWith(".md"))
    .map((name) => ({ id: name.slice(0, -3), ...parse(readFileSync(join(root, name), "utf8")) }));
}

export default {
  id: "uze-project-agents",
  async setup(ctx) {
    const location = ctx.location;
    const root = agentsRoot(location.directory);
    log({ kind: "setup", location, root });
    await ctx.agent.transform((editor) => {
      const agents = load(root);
      log({ kind: "transform", directory: location.directory, agents: agents.map((a) => a.id) });
      for (const agent of agents) {
        editor.update(agent.id, (item) => {
          item.name = agent.fields.name ?? agent.id;
          if (agent.fields.description) item.description = agent.fields.description;
          item.mode = "subagent";
          item.system = agent.body.trim();
        });
      }
    });
    if (!root) return;
    const watcher = watch(root, () => {
      log({ kind: "change", directory: location.directory });
      ctx.agent.reload().catch((e) => log({ kind: "reload-failed", error: String(e) }));
    });
    return () => watcher.close();
  },
};
""".replace("__DEBUG__", DEBUG)


def agent_file(name, mark):
    return f"printf -- '---\\ndescription: {mark}\\n---\\nYou are {name}. {mark}_SYSTEM\\n' > {name}.md"


SCRIPT = f"""
set +e
mkdir -p /work/home/.config/opencode/plugins
cat > /work/home/.config/opencode/plugins/uze-project-agents.ts <<'UZE_PLUGIN'
{PLUGIN}
UZE_PLUGIN
for p in a b; do mkdir -p /work/$p/.agents/agents && (cd /work/$p && git init -q .); done
mkdir -p /work/a/sub
(cd /work/a/.agents/agents && {agent_file("a-agent", "UZE_EXP_A_AGENT")})
(cd /work/b/.agents/agents && {agent_file("b-agent", "UZE_EXP_B_AGENT")})
opencode --version
opencode service stop >/dev/null 2>&1
cd /work/a && timeout 120 opencode run "UZE_EXP_TURN_A hello"
cd /work/b && timeout 120 opencode run "UZE_EXP_TURN_B hello"
cd /work/a/sub && timeout 120 opencode run "UZE_EXP_TURN_SUB hello"
(cd /work/b/.agents/agents && {agent_file("late-agent", "UZE_EXP_LATE_AGENT")})
sleep 2
cd /work/b && timeout 120 opencode run "UZE_EXP_TURN_LATE hello"
cd /work/b && timeout 180 opencode run "UZE_EXP_TURN_DISPATCH delegate"
opencode service stop >/dev/null 2>&1
echo '### plugin-debug'
cat {DEBUG}
"""


def _turn(requests, turn):
    """The requests carrying `turn`'s prompt."""
    return [r for r in requests if turn in r]


def _offers(requests, turn, mark):
    return any(mark in r for r in _turn(requests, turn))


def run(cfg, prov_ip):
    dispatch = {
        "agent": "late-agent",
        "description": "lab dispatch",
        "prompt": "UZE_EXP_SUB_TASK run your checks",
    }
    prov_ip = common.start_provider(
        cfg,
        "toolcall",
        {
            "DISCOVERY": "1",
            "TOOL_NAME": "subagent",
            "TOOL_ARGS": json.dumps(dispatch),
            "TOOL_TRIGGER": "UZE_EXP_TURN_DISPATCH",
        },
    )
    cmd = common.docker_base(
        cfg, prov_ip, opencode_setup(cfg, prov_ip, SCRIPT, plugins="flow"), tty=False
    )
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=900
    )
    out = proc.stdout + proc.stderr
    with open(f"{cfg.outdir}/probe.out", "w") as f:
        f.write(out)
    raw = subprocess.run(
        ["docker", "exec", cfg.prov_name, "cat", "/app/raw-requests.log"],
        capture_output=True,
        text=True,
        errors="replace",
    ).stdout
    with open(f"{cfg.outdir}/requests.log", "w") as f:
        f.write(raw)
    requests = re.split(r"\n(?=### (?:POST|GET) )", raw)
    debug = [
        json.loads(line)
        for line in out.split("### plugin-debug", 1)[-1].splitlines()
        if line.startswith("{")
    ]
    setups = [d for d in debug if d.get("kind") == "setup"]

    for turn in ("A", "B", "SUB", "LATE", "DISPATCH"):
        common.check(
            f"turn-{turn.lower()}-reached-the-provider",
            bool(_turn(requests, f"UZE_EXP_TURN_{turn}")),
            f"turn {turn} produced at least one request",
        )
    common.check(
        "plugin-set-up-once-per-location",
        len({d["location"]["directory"] for d in setups}) >= 2,
        f"setups by directory: {[d['location']['directory'] for d in setups]}",
    )
    common.check(
        "project-a-offers-its-agent",
        _offers(requests, "UZE_EXP_TURN_A", "UZE_EXP_A_AGENT"),
        "a's agent reaches the model in a",
    )
    common.check(
        "project-a-does-not-offer-b",
        not _offers(requests, "UZE_EXP_TURN_A", "UZE_EXP_B_AGENT"),
        "b's agent stays out of a",
    )
    common.check(
        "project-b-offers-its-agent",
        _offers(requests, "UZE_EXP_TURN_B", "UZE_EXP_B_AGENT"),
        "b's agent reaches the model in b, on the same service",
    )
    common.check(
        "project-b-does-not-offer-a",
        not _offers(requests, "UZE_EXP_TURN_B", "UZE_EXP_A_AGENT"),
        "a's agent does not leak into b",
    )
    common.check(
        "subdirectory-offers-the-project-agent",
        _offers(requests, "UZE_EXP_TURN_SUB", "UZE_EXP_A_AGENT"),
        "a turn from a/sub finds a's agents at the repository root",
    )
    common.check(
        "agent-added-while-running-is-offered",
        _offers(requests, "UZE_EXP_TURN_LATE", "UZE_EXP_LATE_AGENT"),
        "the watch reloaded b's registry without a restart",
    )
    subagent = [
        r
        for r in requests
        if "UZE_EXP_SUB_TASK" in r and "UZE_EXP_TURN_DISPATCH" not in r
    ]
    common.check(
        "dispatched-subagent-ran",
        bool(subagent),
        f"{len(subagent)} request(s) of the subagent's own session",
    )
    common.check(
        "dispatched-subagent-has-its-system-prompt",
        any("UZE_EXP_LATE_AGENT_SYSTEM" in r for r in subagent),
        "the body the plugin registered reaches the subagent as its prompt",
    )
