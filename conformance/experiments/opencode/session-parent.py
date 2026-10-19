"""Does an OpenCode child session tell a plugin which session spawned it?

UZE's `SessionPlugin` channel finds the agent's context by the session id
the launch assigned, so a subagent's child session, which has an id of its
own, is given nothing (`opencode/launch-instructions`, `LI_MODE=subagent`).
Keying the context to the parent would need the child to name it. This runs
the same subagent scene with one more plugin, which answers every
`context` event by pushing what it can learn onto the request's system
parts: the event's keys, the plugin context's keys, and the session record
asked of the plugin's client, with its `parentID`. The provider's raw
request log is then the evidence: the child's request carries the line.

Run: python3 conformance/lab.py --harness opencode --experiment opencode/session-parent
"""

import json

from experiments import launch_instructions_probe as li
from experiments.agent_context_probe import requests

PROBE = "UZE_SESSION_PROBE"

PLUGIN = r"""export default {
  id: "uze-session-probe",
  async setup(ctx) {
    await ctx.session.hook("context", async (event) => {
      if (!Array.isArray(event.system)) return;
      let info = null;
      const asks = [
        () => ctx.client?.session?.get?.({ path: { id: event.sessionID } }),
        () => ctx.client?.session?.get?.({ sessionID: event.sessionID }),
        () => ctx.session?.get?.({ sessionID: event.sessionID }),
        () => ctx.session?.get?.({ id: event.sessionID }),
      ];
      const tried = [];
      for (const ask of asks) {
        try {
          const answer = await ask();
          tried.push(answer === undefined ? "undefined" : "answered");
          if (answer !== undefined) { info = answer?.data ?? answer; break; }
        } catch (e) { tried.push(String(e).slice(0, 120)); }
      }
      const plain = (o) => o && typeof o === "object" ? Object.keys(o) : typeof o;
      const text = "UZE_SESSION_PROBE " + JSON.stringify({
        id: event.sessionID,
        eventKeys: plain(event),
        ctxKeys: plain(ctx),
        sessionApi: plain(ctx.session),
        clientSession: plain(ctx.client?.session),
        tried,
        parentID: info?.parentID ?? info?.parentId ?? info?.parent ?? null,
        info: JSON.stringify(info)?.slice(0, 600) ?? null,
      });
      const first = event.system[0];
      event.system.push(typeof first === "object" && first !== null ? { ...first, text } : text);
    });
  },
};
"""

_prelude = li.prelude


def prelude(bindings, prov_ip):
    return (
        _prelude(bindings, prov_ip)
        + "\nmkdir -p /work/home/.config/opencode/plugins\n"
        + "cat > /work/home/.config/opencode/plugins/uze-session-probe.ts <<'UZE_EOF'\n"
        + PLUGIN
        + "UZE_EOF\n"
    )


def _strings(node):
    if isinstance(node, dict):
        for child in node.values():
            yield from _strings(child)
    elif isinstance(node, list):
        for child in node:
            yield from _strings(child)
    elif isinstance(node, str) and PROBE in node:
        yield node


def run(cfg, prov_ip):
    li.MODE = "subagent"
    li.prelude = prelude
    li.run(cfg, prov_ip)
    with open(f"{cfg.outdir}/li-raw-requests.log") as f:
        log = f.read()
    seen = []
    for i, (header, body, raw) in enumerate(requests(log)):
        for text in _strings(body):
            for line in text.splitlines():
                if line.startswith(PROBE + " "):
                    try:
                        found = json.loads(line[len(PROBE) + 1 :])
                    except ValueError:
                        found = line[:300]
                    seen.append({"index": i, "probe": found, "sub": li.SUB_TURN in raw})
    with open(f"{cfg.outdir}/session-probe.json", "w") as f:
        json.dump(seen, f, indent=1)
    for entry in seen:
        li.probe.say(f"[session] {json.dumps(entry)[:900]}")
    child = [e for e in seen if isinstance(e["probe"], dict) and e["sub"]]
    li.common.check(
        "session-child-names-its-parent",
        any(e["probe"].get("parentID") for e in child),
        f"probe lines in subagent requests: {len(child)}",
    )
