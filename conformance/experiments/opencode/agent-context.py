"""Can a launch hand OpenCode an agent-context document, invisibly?

See `experiments.agent_context_probe` for the four questions. The channel
is a plugin injected for this launch alone (`OPENCODE_CONFIG_CONTENT`
naming a package directory, as `experiments/opencode/launch-hook` proved)
whose `session.hook("context", ...)` pushes the document onto the event's
`system` list. The document is read on every call, from
`$UZE_AGENT_CONTEXT_DIR/<sessionID>.md` when it exists, else from
`$UZE_AGENT_CONTEXT_DIR/default.md` — the launcher of a first process does
not know the session's id yet.

The plugin records every event it is handed (keys, `sessionID`, the shape
of `system[0]`), so a wrong guess about the event is evidence, not silence.

Two headless processes, `opencode run --standalone`: the first with marker
A in `default.md`; the second `--session <id>` after marker B was written
to `<id>.md` (A stays in `default.md`, so B arriving proves the keyed read).

A second world (`service`) asks what the background service does with a
plugin written after it started — see `service_script`.

Run: python3 conformance/lab.py --harness opencode --experiment opencode/agent-context
Env: AGENT_CONTEXT_PHASES=standalone,service
"""

import json
import os
import subprocess
import time

from experiments import agent_context_probe as probe
from harnesses.opencode.scenarios import opencode_setup
from shared import common

PLUGIN_DIR = "/work/uze-inject/agent-context"
CONTEXT_DIR = "/work/uze-inject/context"
DEBUG = "/work/plugin-debug.jsonl"
CONTENT = json.dumps({"plugins": [PLUGIN_DIR]})

PLUGIN = r"""import { appendFileSync, existsSync, readFileSync } from "node:fs";
const DEBUG = "__DEBUG__";
const log = (o) => { try { appendFileSync(DEBUG, JSON.stringify({ id: "__ID__", pid: process.pid, ...o }) + "\n"); } catch (e) {} };
const read = (sessionID) => {
  const dir = process.env.UZE_AGENT_CONTEXT_DIR;
  if (!dir) return null;
  for (const name of [`${sessionID}.md`, "default.md"]) {
    const file = `${dir}/${name}`;
    if (existsSync(file)) return { file, text: readFileSync(file, "utf8") };
  }
  return null;
};
export default {
  id: "__ID__",
  async setup(ctx) {
    log({ kind: "setup", session_keys: Object.keys(ctx.session ?? {}) });
    await ctx.session.hook("context", async (event) => {
      const system = event.system;
      const first = Array.isArray(system) ? system[0] : undefined;
      const found = read(event.sessionID);
      log({
        kind: "context",
        keys: Object.keys(event),
        sessionID: event.sessionID ?? null,
        systemType: Array.isArray(system) ? "array" : typeof system,
        systemLength: Array.isArray(system) ? system.length : null,
        firstShape: first === undefined ? null : (typeof first === "string" ? "string" : Object.keys(first)),
        file: found?.file ?? null,
      });
      if (!found || !Array.isArray(system)) return;
      const text = found.text;
      system.push(typeof first === "object" && first !== null ? { ...first, text } : text);
    });
  },
};
"""


def plugin(plugin_id):
    return PLUGIN.replace("__DEBUG__", DEBUG).replace("__ID__", plugin_id)


#: The shared background service a default launch attaches to (how UZE
#: launches OpenCode). Started with no plugin; then a plugin *file* is
#: written into the auto-discovered `~/.config/opencode/plugins/` and a
#: package directory is named by a later launch's `OPENCODE_CONFIG_CONTENT`
#: — is either loaded without a restart? Each launch's turn token says
#: which request is whose.
SERVICE_TOKENS = (
    "AGENTCTX_SVC_WARM",
    "AGENTCTX_SVC_FILE",
    "AGENTCTX_SVC_CONTENT",
    "AGENTCTX_SVC_RESTARTED",
)
USER_PLUGIN = "/work/home/.config/opencode/plugins/uze-agent-context-file.ts"


def service_script():
    warm, file_turn, content_turn, restarted = SERVICE_TOKENS
    return f"""
set +e
mkdir -p /work/project {PLUGIN_DIR} {CONTEXT_DIR} /work/home/.config/opencode/plugins
(cd /work/project && git init -q . && printf 'lab project\\n' > README.md)
printf '{{"name":"uze-agent-context","version":"1.0.0","main":"index.ts"}}' > {PLUGIN_DIR}/package.json
cat > {PLUGIN_DIR}/index.ts <<'PLUGIN_EOF'
{plugin("svc-content")}PLUGIN_EOF
{probe.write_document(f"{CONTEXT_DIR}/default.md", probe.MARKER_A, "one")}
export UZE_AGENT_CONTEXT_DIR={CONTEXT_DIR}
cd /work/project
opencode service stop >/dev/null 2>&1
echo '=== warm'
# decision: headless-permissions
timeout 150 opencode run --auto "{warm}" </dev/null 2>&1 | tail -c 300
cat > {USER_PLUGIN} <<'PLUGIN_EOF'
{plugin("svc-file")}PLUGIN_EOF
echo '=== file'
# decision: headless-permissions
timeout 150 opencode run --auto "{file_turn}" </dev/null 2>&1 | tail -c 300
echo '=== content'
# decision: headless-permissions
OPENCODE_CONFIG_CONTENT='{CONTENT}' timeout 150 opencode run --auto "{content_turn}" </dev/null 2>&1 | tail -c 300
opencode service stop >/dev/null 2>&1
echo '=== restarted'
# decision: headless-permissions
timeout 150 opencode run --auto "{restarted}" </dev/null 2>&1 | tail -c 300
opencode service stop >/dev/null 2>&1
echo '=== debug'
cat {DEBUG}
"""


def setup_script():
    return f"""
set +e
mkdir -p /work/project {PLUGIN_DIR} {CONTEXT_DIR}
(cd /work/project && git init -q . && printf 'lab project\\n' > README.md)
printf '{{"name":"uze-agent-context","version":"1.0.0","main":"index.ts"}}' > {PLUGIN_DIR}/package.json
cat > {PLUGIN_DIR}/index.ts <<'PLUGIN_EOF'
{plugin("uze-agent-context")}PLUGIN_EOF
{probe.write_document(f"{CONTEXT_DIR}/default.md", probe.MARKER_A, "one")}
export UZE_AGENT_CONTEXT_DIR={CONTEXT_DIR}
export OPENCODE_CONFIG_CONTENT='{CONTENT}'
cd /work/project
echo '=== one'
# decision: headless-permissions
timeout 150 opencode run --standalone --auto --format json "{probe.TURN_ONE}" </dev/null > /work/one.out 2>&1
echo "exit $?"; tail -c 3000 /work/one.out
sid=$(node -e 'const l=require("fs").readFileSync("{DEBUG}","utf8").trim().split("\\n").map(JSON.parse).filter(e=>e.sessionID);console.log(l.length?l[l.length-1].sessionID:"")')
echo '=== sid'
echo "$sid"
{probe.write_document(f'{CONTEXT_DIR}/"$sid".md', probe.MARKER_B, "two")}
echo '=== two'
# decision: headless-permissions
timeout 150 opencode run --standalone --auto --format json --session "$sid" "{probe.TURN_TWO}" </dev/null 2>&1 | tail -c 3000
echo "exit $?"
echo '=== debug'
cat {DEBUG}
echo '=== context-dir'
ls {CONTEXT_DIR}
echo '=== export'
opencode session export "$sid" > /work/export.json 2>&1
echo "bytes $(wc -c < /work/export.json); alpha $(grep -c {probe.MARKER_A} /work/export.json); bravo $(grep -c {probe.MARKER_B} /work/export.json)"
find /work/home/.local/share/opencode -type f 2>/dev/null | xargs grep -l {probe.MARKER_A} 2>/dev/null | head
"""


def sections(text):
    out, name = {}, "pre"
    for line in text.splitlines():
        if line.startswith("=== "):
            name = line[4:].strip()
            out[name] = []
        else:
            out.setdefault(name, []).append(line)
    return {k: "\n".join(v) for k, v in out.items()}


PHASES = os.environ.get("AGENT_CONTEXT_PHASES", "standalone,service").split(",")


def container(cfg, body):
    prov_ip = common.start_provider(cfg, "static", {"DISCOVERY": "1"})
    time.sleep(1)
    cmd = common.docker_base(
        cfg, prov_ip, opencode_setup(cfg, prov_ip, body, plugins="flow"), tty=False
    )
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=900
    )
    return {"out": proc.stdout + proc.stderr, "log": probe.provider_log(cfg)}


def standalone(cfg):
    result = common.concurrently(cfg, {"oc": lambda w: container(w, setup_script())})[
        "oc"
    ]
    out = result["out"]
    with open(os.path.join(cfg.outdir, "opencode-context-hook.out"), "w") as f:
        f.write(out)
    parts = sections(out)
    probe.say(f"session: {parts.get('sid', '').strip()}")
    probe.say(f"turn one: {parts.get('one', '')[-300:]!r}")
    probe.say(f"turn two: {parts.get('two', '')[-300:]!r}")
    probe.say(f"plugin events: {parts.get('debug', '')[:1500]!r}")
    probe.say(f"session export / stored state: {parts.get('export', '')!r}")
    probe.analyse(cfg, "opencode-context-hook", result["log"])


def service(cfg):
    result = common.concurrently(
        cfg, {"svc": lambda w: container(w, service_script())}
    )["svc"]
    out = result["out"]
    with open(os.path.join(cfg.outdir, "opencode-service.out"), "w") as f:
        f.write(out)
    with open(os.path.join(cfg.outdir, "opencode-service-raw-requests.log"), "w") as f:
        f.write(result["log"])
    parts = sections(out)
    events = []
    for line in parts.get("debug", "").splitlines():
        try:
            events.append(json.loads(line))
        except ValueError:
            pass
    with open(os.path.join(cfg.outdir, "opencode-service-events.json"), "w") as f:
        json.dump(events, f, indent=1)
    reqs = probe.requests(result["log"])
    for token in SERVICE_TOKENS:
        carrying = [(h, b) for h, b, raw in reqs if token in raw and b is not None]
        model = next(
            ((h, b) for h, b in carrying if capture_tools(b)),
            carrying[0] if carrying else (None, None),
        )
        hits = probe.locate(model[1], probe.MARKER_A) if model[1] is not None else []
        probe.say(
            f"[service] {token}: request {model[0]}; marker at {[h['path'] for h in hits]}"
        )
    for plugin_id in ("svc-file", "svc-content"):
        kinds = [(e["kind"], e["pid"]) for e in events if e.get("id") == plugin_id]
        probe.say(f"[service] {plugin_id} events (kind, pid): {kinds}")

    def reached(token):
        for h, b, raw in reqs:
            if token in raw and probe.MARKER_A in raw:
                return True
        return False

    common.check(
        "opencode-service-file-plugin-without-restart",
        reached(SERVICE_TOKENS[1]),
        "a plugin file written into ~/.config/opencode/plugins after the service started",
    )
    # The file plugin answers every later turn too, so the marker on the
    # wire cannot say which plugin put it there: the plugin's own events can.
    content_events = [e for e in events if e.get("id") == "svc-content"]
    common.check(
        "opencode-service-config-content-plugin-without-restart",
        bool(content_events),
        "a package dir named by a later launch's OPENCODE_CONFIG_CONTENT, service "
        f"already up: {len(content_events)} event(s) from that plugin",
    )
    common.check(
        "opencode-service-plugin-after-restart",
        reached(SERVICE_TOKENS[3]),
        "the same files after `opencode service stop` and a fresh launch",
    )


def capture_tools(body):
    from shared import capture

    return capture.declared_tools(body)


def run(cfg, prov_ip):
    if "standalone" in PHASES:
        standalone(cfg)
    if "service" in PHASES:
        service(cfg)
