"""Observation experiment: where a project agent registered by a global
plugin stands against OpenCode's own agents, and what a running service
picks up without a restart (`opencode-single-plugin` tasks 1.3, 1.3b, 1.4).

The plugin is the shape D5/D5b describe: it reads
`<ctx.location.project.directory>/.agents/agents` with `Bun.YAML`, skips an
id the registry already holds when its transform runs, and records, on
every `context` event, the resolved agents it registered.

1.3, precedence (read from source: external plugins run between OpenCode's
`pre` and `post` internal plugins):
- a project `build.md` is skipped: the built-in `build` exists already;
- a project `dup.md` and an `.opencode/agents/dup.md`: the configured file
  applies over the project's, field by field;
- `opencode.json`'s `permissions` reach a project agent.

1.3b, a linked checkout (`git worktree add`) whose branch changed an agent
offers its own version, not the main checkout's.

1.4, a running service:
- a plugin file written after the service started is set up at the next
  location it serves;
- a document written into a watched directory reaches an existing setup.

Run:
  python3 conformance/lab.py --harness opencode --experiment opencode/project-agents-precedence
"""

import json
import re
import subprocess

from harnesses.opencode.scenarios import opencode_setup
from shared import common

DEBUG = "/work/plugin-debug.jsonl"
DOCS = "/work/docs"

PLUGIN = r"""import { appendFileSync, existsSync, readdirSync, readFileSync, watch } from "node:fs";
import { join } from "node:path";

const DEBUG = "__DEBUG__";
const DOCS = "__DOCS__";
const log = (o) => { try { appendFileSync(DEBUG, JSON.stringify({ pid: process.pid, ...o }) + "\n"); } catch {} };

function parse(text) {
  const match = /^---\n([\s\S]*?)\n---\n?([\s\S]*)$/.exec(text);
  if (!match) return { fields: {}, body: text };
  return { fields: Bun.YAML.parse(match[1]) ?? {}, body: match[2] };
}

function load(root) {
  if (!existsSync(root)) return [];
  return readdirSync(root)
    .filter((name) => name.endsWith(".md"))
    .map((name) => ({ id: name.slice(0, -3), ...parse(readFileSync(join(root, name), "utf8")) }));
}

export default {
  id: "uze-precedence-probe",
  async setup(ctx) {
    const root = join(ctx.location.project.directory, ".agents", "agents");
    const mine = new Set();
    log({ kind: "setup", directory: ctx.location.directory, project: ctx.location.project, root });
    await ctx.agent.transform((editor) => {
      const agents = load(root);
      const skipped = [];
      for (const agent of agents) {
        if (editor.get(agent.id) && !mine.has(agent.id)) { skipped.push(agent.id); continue; }
        mine.add(agent.id);
        editor.update(agent.id, (item) => {
          item.name = agent.fields.name ?? agent.id;
          if (agent.fields.description) item.description = agent.fields.description;
          item.mode = "subagent";
          item.system = agent.body.trim();
        });
      }
      log({ kind: "transform", directory: ctx.location.directory, registered: [...mine], skipped });
    });
    await ctx.session.hook("context", async () => {
      const resolved = {};
      for (const id of ["build", "dup", "a-agent"]) {
        const found = await ctx.agent.get({ agentID: id }).catch((e) => ({ error: String(e) }));
        const info = found?.data ?? found;
        resolved[id] = info && { description: info.description, system: info.system, mode: info.mode, permissions: info.permissions };
      }
      log({ kind: "context", directory: ctx.location.directory, resolved });
    });
    if (existsSync(DOCS)) {
      watch(DOCS, (event, name) => {
        let body = null;
        try { body = JSON.parse(readFileSync(join(DOCS, name), "utf8")); } catch (e) { body = { error: String(e) }; }
        log({ kind: "doc", directory: ctx.location.directory, event, name, body });
      });
    }
  },
};
""".replace("__DEBUG__", DEBUG).replace("__DOCS__", DOCS)

LATE = r"""import { appendFileSync } from "node:fs";
export default {
  id: "uze-late-probe",
  async setup(ctx) {
    appendFileSync("__DEBUG__", JSON.stringify({ pid: process.pid, kind: "late-setup", directory: ctx.location.directory }) + "\n");
  },
};
""".replace("__DEBUG__", DEBUG)

GIT = "git -c user.name=lab -c user.email=lab@uze.invalid -c commit.gpgsign=false"


def agent(path, description, body):
    return f"printf -- '---\\ndescription: {description}\\n---\\n{body}\\n' > {path}"


SCRIPT = f"""
set +e
P=/work/home/.config/opencode/plugins
mkdir -p $P {DOCS}
cat > $P/uze-precedence-probe.ts <<'UZE_PLUGIN'
{PLUGIN}
UZE_PLUGIN
node -e '
const fs=require("fs"); const p="/work/home/.config/opencode/opencode.json";
const d=JSON.parse(fs.readFileSync(p,"utf8"));
d.permissions=[{{"action":"edit","resource":"/UZE_EXP_PERM/*","effect":"deny"}}];
fs.writeFileSync(p, JSON.stringify(d,null,1));
'
mkdir -p /work/a/.agents/agents /work/a/.opencode/agents /work/b
cd /work/a && git init -q . && (cd /work/b && git init -q .)
{agent("/work/a/.agents/agents/a-agent.md", "UZE_EXP_A_MAIN", "UZE_EXP_A_MAIN_SYSTEM")}
{agent("/work/a/.agents/agents/build.md", "UZE_EXP_PROJECT_BUILD", "UZE_EXP_PROJECT_BUILD_SYSTEM")}
{agent("/work/a/.agents/agents/dup.md", "UZE_EXP_PROJECT_DUP", "UZE_EXP_PROJECT_DUP_SYSTEM")}
{agent("/work/a/.opencode/agents/dup.md", "UZE_EXP_CONFIG_DUP", "UZE_EXP_CONFIG_DUP_SYSTEM")}
cd /work/a && {GIT} add -A && {GIT} commit -q -m init && {GIT} worktree add -q /work/a-wt -b branch
{agent("/work/a-wt/.agents/agents/a-agent.md", "UZE_EXP_A_BRANCH", "UZE_EXP_A_BRANCH_SYSTEM")}
opencode service stop >/dev/null 2>&1
cd /work/a && timeout 120 opencode run "UZE_EXP_TURN_MAIN hello"
cd /work/a-wt && timeout 120 opencode run "UZE_EXP_TURN_WT hello"
cat > $P/uze-late-probe.ts <<'UZE_PLUGIN'
{LATE}
UZE_PLUGIN
printf '{{"v":1,"mark":"UZE_EXP_DOC"}}' > {DOCS}/pkg.json
sleep 3
cd /work/b && timeout 120 opencode run "UZE_EXP_TURN_B hello"
cd /work/a && timeout 120 opencode run "UZE_EXP_TURN_MAIN_AGAIN hello"
opencode service stop >/dev/null 2>&1
echo '### plugin-debug'
cat {DEBUG}
"""


def _turn(requests, turn):
    return [r for r in requests if turn in r]


def run(cfg, prov_ip):
    prov_ip = common.start_provider(cfg, "static", {"DISCOVERY": "1"})
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
    with open(f"{cfg.outdir}/plugin-debug.json", "w") as f:
        json.dump(debug, f, indent=1)

    def resolved(directory, agent_id):
        found = [
            d["resolved"].get(agent_id)
            for d in debug
            if d.get("kind") == "context" and d.get("directory") == directory
        ]
        return next((r for r in found if r), None)

    main_build = resolved("/work/a", "build") or {}
    main_dup = resolved("/work/a", "dup") or {}
    main_agent = resolved("/work/a", "a-agent") or {}
    skipped = {
        s
        for d in debug
        if d.get("kind") == "transform" and d.get("directory") == "/work/a"
        for s in d["skipped"]
    }

    for turn in ("MAIN", "WT", "B", "MAIN_AGAIN"):
        common.check(
            f"turn-{turn.lower()}-reached-the-provider",
            bool(_turn(requests, f"UZE_EXP_TURN_{turn}")),
            f"turn {turn} produced at least one request",
        )
    common.check(
        "builtin-build-is-skipped",
        "build" in skipped and "UZE_EXP_PROJECT_BUILD" not in json.dumps(main_build),
        f"skipped={sorted(skipped)} build={main_build.get('description')!r}",
    )
    common.check(
        "configured-dup-applies-over-the-project",
        main_dup.get("description") == "UZE_EXP_CONFIG_DUP",
        f"dup={json.dumps(main_dup)[:300]}",
    )
    common.check(
        "global-permissions-reach-a-project-agent",
        "UZE_EXP_PERM" in json.dumps(main_agent.get("permissions")),
        f"a-agent permissions={json.dumps(main_agent.get('permissions'))[:300]}",
    )
    common.check(
        "linked-checkout-offers-its-branch",
        any("UZE_EXP_A_BRANCH" in r for r in _turn(requests, "UZE_EXP_TURN_WT"))
        and not any("UZE_EXP_A_MAIN" in r for r in _turn(requests, "UZE_EXP_TURN_WT")),
        "the worktree's own a-agent, not main's",
    )
    late = [d for d in debug if d.get("kind") == "late-setup"]
    common.check(
        "plugin-written-while-running-is-set-up",
        bool(late),
        f"late setups: {[d['directory'] for d in late]}",
    )
    docs = [d for d in debug if d.get("kind") == "doc"]
    common.check(
        "watched-document-reaches-an-existing-setup",
        any(d.get("body", {}).get("mark") == "UZE_EXP_DOC" for d in docs),
        f"doc events: {[(d['directory'], d['event']) for d in docs]}",
    )
