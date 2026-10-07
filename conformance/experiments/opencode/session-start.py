"""Can the generated OpenCode plugin observe a session starting, once?

The portable `SessionStart` event is delivered natively on Claude Code and
Codex (the hooks contract's `events` scene). OpenCode has no hook file; the
generated plugin would have to subscribe to the harness's own
`session.created` event. Whether that is a route depends on facts this
experiment measures rather than assumes:

1. what a user plugin's `setup(ctx)` is handed (its keys, and whether an
   event subscription is among them);
2. whether `session.created` reaches the plugin for a new session;
3. that it does *not* arrive again for a continued one (a resume must not
   look like a start, and nothing in the event says which it was).

A probe plugin (nothing to do with the delivered artifact) records what it
sees; two headless runs follow — a new session, then `--continue` on it.

  every (1)-(3) holds -> the bridge may register SessionStart on it
  otherwise           -> SessionStart stays Unsupported on OpenCode, with
                         the recorded observation as its reason

Observed on opencode 2.0.18 (2026-09-28): (1) holds — `ctx.event.subscribe()`
is an async iterable; (2) does not — across a new session and a continued
one the stream yields `session.inbox.enqueued`, `session.execution.started`,
`session.step.started`… and never `session.created`, and the two sequences
differ only in incidental events (`session.renamed`, `session.usage.updated`).
SessionStart is therefore Unsupported on OpenCode.

The same run also records, via `--print-logs`, whether the harness loaded
each plugin file: 2.0.18 refuses a global plugin importing
`@opencode-ai/plugin` ("Cannot find package"), which is why the probe does
not import it.

Run: python3 conformance/lab.py --harness opencode --experiment opencode/session-start
"""

import json
import time

import pexpect

from harnesses.opencode.scenarios import opencode_container
from shared import common
from shared.common import check

LOG = "/tmp/session-events.jsonl"
BEGIN, END = "=== SESSION-EVENTS ===", "=== SESSION-EVENTS-END ==="

PROBE = r"""
mkdir -p /work/home/.config/opencode/plugins
cat > /work/home/.config/opencode/plugins/session-probe.ts <<'PROBE_EOF'
// No import: `Plugin.define` is the identity, and 2.0.18 cannot resolve
// `@opencode-ai/plugin` from the global plugin directory (measured here).
const LOG = "/tmp/session-events.jsonl";
async function record(kind, detail) {
  try {
    const before = await Bun.file(LOG).text().catch(() => "");
    await Bun.write(LOG, before + JSON.stringify({ kind, detail }) + "\n");
  } catch (error) {
    console.error("probe failed", error);
  }
}
function shape(value) {
  if (!value || typeof value !== "object") return typeof value;
  const out = {};
  for (const key of Object.keys(value)) {
    const inner = value[key];
    out[key] = inner && typeof inner === "object" ? Object.keys(inner) : typeof inner;
  }
  return out;
}
export default {
  id: "session-probe",
  async setup(ctx) {
    await record("setup", shape(ctx));
    const seen = (via) => async (event) =>
      record("event", { via, type: event?.type, data: event?.data ?? event?.properties ?? null });
    try {
      if (ctx.event?.on) {
        ctx.event.on("session.created", seen("event.on"));
        await record("subscribed", "event.on");
      } else if (ctx.events?.on) {
        ctx.events.on("session.created", seen("events.on"));
        await record("subscribed", "events.on");
      } else if (ctx.event?.subscribe) {
        let stream = ctx.event.subscribe();
        const describe = (value) => ({
          type: typeof value,
          constructor: value?.constructor?.name,
          then: typeof value?.then,
          asyncIterator: typeof value?.[Symbol.asyncIterator],
          prototype: value ? Object.getOwnPropertyNames(Object.getPrototypeOf(value) ?? {}) : [],
        });
        const returned = describe(stream);
        if (typeof stream?.then === "function") stream = await stream;
        await record("subscribed", { via: "event.subscribe", returns: returned, resolved: describe(stream) });
        if (stream && typeof stream[Symbol.asyncIterator] === "function") {
          (async () => {
            for await (const event of stream) {
              if (event?.type === "session.created") await seen("event.subscribe")(event);
              else await record("other", event?.type ?? Object.keys(event ?? {}));
            }
            await record("stream-ended", null);
          })().catch((error) => record("stream-error", String(error)));
        }
      } else {
        await record("subscribed", "none");
      }
    } catch (error) {
      await record("subscribe-error", String(error));
    }
  },
};
PROBE_EOF
"""


def evidence_command():
    run = "opencode run --standalone --auto"
    return f"""
cd /work
echo '{BEGIN}'
echo '--- new'
{run} --print-logs 'say hello' </dev/null 2>&1 | grep -E 'load plugin|UZE_CONF' | cut -c1-400 || true
echo '--- continued'
{run} --continue 'say hello again' </dev/null 2>&1 | tail -5 || true
echo '--- events'
cat {LOG} 2>/dev/null || true
echo '{END}'
"""


def run(cfg, prov_ip):
    common.start_provider(cfg, "static")
    time.sleep(1)
    cmd = opencode_container(
        cfg, prov_ip, PROBE + evidence_command(), plugins="hook-session-plugin"
    )
    child = pexpect.spawn(
        cmd[0], cmd[1:], encoding="utf-8", codec_errors="replace", timeout=300
    )
    child.setwinsize(50, 200)
    child.expect([END, pexpect.EOF, pexpect.TIMEOUT])
    text = common.ansi_strip(child.before or "")
    child.close(force=True)
    text = text[text.find(BEGIN) :] if BEGIN in text else text
    with open(f"{cfg.outdir}/session_events.raw", "w") as f:
        f.write(text)

    events, section = [], None
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("--- "):
            section = stripped[4:]
            continue
        if section == "events" and stripped.startswith("{"):
            try:
                events.append(json.loads(stripped))
            except json.JSONDecodeError:
                continue
    with open(f"{cfg.outdir}/session_events.json", "w") as f:
        json.dump(events, f, indent=1)

    setups = [e for e in events if e["kind"] == "setup"]
    subscribed = [e["detail"] for e in events if e["kind"] == "subscribed"]
    created = [e for e in events if e["kind"] == "event"]
    check(
        "session-probe-loaded",
        bool(setups),
        f"{len(setups)} plugin setup(s); ctx shape: {json.dumps(setups[:1])[:400]}",
    )
    check(
        "session-probe-can-subscribe",
        any(detail != "none" for detail in subscribed),
        f"subscription route(s): {json.dumps(subscribed)[:300]}",
    )
    check(
        "session-created-once-for-a-new-session-only",
        len(created) == 1,
        f"{len(created)} session.created event(s) across a new and a continued "
        f"session: {json.dumps(created)[:400]}",
    )
