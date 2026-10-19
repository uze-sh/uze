"""Which working directory does OpenCode hand the rules plugin?

The generated rules plugin (`crates/uze-integrations/src/rules/plugin.rs`)
asks the engine about "the checkout the session works in", and has to name
it: `event.directory`, `event.cwd`, `ctx.directory`, `ctx.worktree`, or
`process.cwd()`, in that order. Which of those OpenCode fills, and with
what, decides whether a project's rules reach a session at all when the
process serving it was started somewhere else — which is what a default
launch is: it attaches to the shared background service, whose working
directory is wherever the first launch ran.

A probe plugin (nothing to do with the delivered one) records, for every
`context` and `execute.after` event, each candidate's value and the
process that answered. Three launches, one tool call each (`read` of the
project's README):

1. `opencode run --standalone` in `/work/project` (the process is the
   project's own);
2. `opencode run` in `/work/elsewhere`, which starts the shared service
   there;
3. `opencode run` in `/work/project`, served by that service.

The rules plugin setup delivers (the workspace's, from the executable
already installed) is installed beside the probe, and
`/work/project` keeps an always-on rule whose body carries a marker: the
provider's requests say whether the delivered plugin found the project's
rules in each launch, so the measurement and the product are read off the
same run.

Run: python3 conformance/lab.py --harness opencode --experiment opencode/rules-directory --discovery
"""

import json
import os
import subprocess

from experiments import agent_context_probe as probe
from harnesses.opencode.scenarios import opencode_setup
from shared import capture, common

DEBUG = "/work/rules-dir-debug.jsonl"
RULE_MARK = "UZE_RULES_DIR_ALWAYS"
TOKENS = {
    "standalone": "UZE_RULES_DIR_STANDALONE",
    "warm": "UZE_RULES_DIR_WARM",
    "served": "UZE_RULES_DIR_SERVED",
}

PROBE = r"""// No import: OpenCode refuses a plugin-directory file that imports its
// own plugin package.
import { appendFileSync } from "node:fs";
const DEBUG = "__DEBUG__";
const log = (o) => { try { appendFileSync(DEBUG, JSON.stringify({ pid: process.pid, ...o }) + "\n"); } catch (e) {} };
// Every own and prototype member of a value, two levels deep, with the
// strings it holds: what a plugin can read a directory from.
const describe = (value, depth = 0) => {
  if (value === null || value === undefined) return value ?? null;
  if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") return value;
  if (typeof value === "function") return "<function>";
  if (depth >= 2) return `<${typeof value}>`;
  const out = {};
  const names = new Set(Object.keys(value));
  const proto = Object.getPrototypeOf(value);
  if (proto && proto !== Object.prototype) for (const n of Object.getOwnPropertyNames(proto)) names.add(n);
  for (const n of names) {
    if (n === "constructor") continue;
    try { out[n] = describe(value[n], depth + 1); } catch (e) { out[n] = `<throws ${e}>`; }
  }
  return out;
};
const candidates = (ctx, event) => ({
  event_directory: event?.directory ?? null,
  event_cwd: event?.cwd ?? null,
  ctx_directory: ctx?.directory ?? null,
  ctx_worktree: describe(ctx?.worktree),
  ctx_location: describe(ctx?.location),
  ctx_app: describe(ctx?.app),
  ctx_vcs: describe(ctx?.vcs),
  process_cwd: process.cwd(),
  event_keys: Object.keys(event ?? {}),
});
export default {
  id: "rules-dir-probe",
  async setup(ctx) {
    log({ kind: "setup", ctx_keys: Object.keys(ctx ?? {}), ...candidates(ctx, {}) });
    await ctx.session.hook("context", async (event) => {
      log({ kind: "context", session: event.sessionID ?? null, ...candidates(ctx, event) });
    });
    await ctx.tool.hook("execute.after", async (event) => {
      log({ kind: "execute.after", session: event.sessionID ?? null, tool: event.tool ?? null, ...candidates(ctx, event) });
    });
  },
};
"""


def script():
    probe_file = PROBE.replace("__DEBUG__", DEBUG)
    rule = f"--- 'trigger: always_on' --- '' 'Lab convention {RULE_MARK}: files here are lab fixtures.'"
    return f"""
set +e
# The workspace sets up the harness it finds installed (`uze setup` would
# update it first, which an offline world cannot).
cd /work
script -qfc 'uze workspace' /work/rules-setup.typescript </dev/null >/dev/null 2>&1 &
setup_pid=$!
i=0
while [ $i -lt 120 ] && ! grep -aq ' ready' /work/rules-setup.typescript 2>/dev/null; do sleep 0.5; i=$((i+1)); done
kill $setup_pid 2>/dev/null
mkdir -p /work/home/.config/opencode/plugins
cat > /work/home/.config/opencode/plugins/rules-dir-probe.ts <<'PROBE_EOF'
{probe_file}PROBE_EOF
for d in /work/project /work/elsewhere; do
  mkdir -p $d && (cd $d && git init -q -b main . && printf 'lab project\\n' > README.md)
done
mkdir -p /work/project/.agents/rules
printf '%s\\n' {rule} > /work/project/.agents/rules/always.md
echo '=== plugins'
ls /work/home/.config/opencode/plugins
opencode service stop >/dev/null 2>&1
cd /work/project
echo '=== standalone'
# decision: headless-permissions
timeout 150 opencode run --standalone --auto "{TOKENS["standalone"]}" </dev/null 2>&1 | tail -c 300
cd /work/elsewhere
echo '=== warm'
# decision: headless-permissions
timeout 150 opencode run --auto "{TOKENS["warm"]}" </dev/null 2>&1 | tail -c 300
cd /work/project
echo '=== served'
# decision: headless-permissions
timeout 150 opencode run --auto "{TOKENS["served"]}" </dev/null 2>&1 | tail -c 300
opencode service stop >/dev/null 2>&1
echo '=== debug'
cat {DEBUG}
"""


def job(world):
    steps = [{"name": "read", "args": json.dumps({"path": "/work/project/README.md"})}]
    prov_ip = common.start_provider(
        world, "toolcall", {"DISCOVERY": "1", "TOOL_SEQUENCE": json.dumps(steps)}
    )
    cmd = common.docker_base(
        world, prov_ip, opencode_setup(world, prov_ip, script(), plugins=""), tty=False
    )
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=900
    )
    return {"out": proc.stdout + proc.stderr, "log": probe.provider_log(world)}


def _sections(text):
    out, name = {}, "pre"
    for line in text.splitlines():
        if line.startswith("=== "):
            name = line[4:].strip()
            out[name] = []
        else:
            out.setdefault(name, []).append(line)
    return {k: "\n".join(v) for k, v in out.items()}


def run(cfg, prov_ip):
    result = common.concurrently(cfg, {"dir": job})["dir"]
    out = result["out"]
    with open(os.path.join(cfg.outdir, "rules-directory.out"), "w") as f:
        f.write(out)
    with open(os.path.join(cfg.outdir, "rules-directory-raw-requests.log"), "w") as f:
        f.write(result["log"])
    parts = _sections(out)
    events = []
    for line in parts.get("debug", "").splitlines():
        try:
            events.append(json.loads(line))
        except ValueError:
            pass
    with open(os.path.join(cfg.outdir, "rules-directory-events.json"), "w") as f:
        json.dump(events, f, indent=1)
    probe.say(f"plugins: {parts.get('plugins', '').split()}")
    for event in events:
        probe.say(json.dumps(event)[:400])
    reqs = [
        (h, b, raw)
        for h, b, raw in probe.requests(result["log"])
        if b is not None and capture.declared_tools(b)
    ]
    reached = {}
    for launch, token in TOKENS.items():
        carrying = [raw for _, _, raw in reqs if token in raw]
        reached[launch] = any(RULE_MARK in raw for raw in carrying)
        probe.say(
            f"[{launch}] {len(carrying)} model requests; rule reached: {reached[launch]}"
        )
    with open(os.path.join(cfg.outdir, "rules-directory-result.json"), "w") as f:
        json.dump({"reached": reached, "events": events}, f, indent=1)
    common.check(
        "rules-directory-events-recorded",
        any(e.get("kind") == "execute.after" for e in events)
        and any(e.get("kind") == "context" for e in events),
        f"{len(events)} probe events",
    )
    common.check(
        "rules-directory-standalone-reaches-model",
        reached["standalone"],
        "the delivered plugin found the project's rules in a standalone launch",
    )
    common.check(
        "rules-directory-served-reaches-model",
        reached["served"],
        "the delivered plugin found the project's rules in a launch the shared "
        "service, started in another directory, served",
    )
    common.check(
        "rules-directory-elsewhere-has-no-rule",
        not reached["warm"],
        "a launch in a directory without rules was handed none",
    )
