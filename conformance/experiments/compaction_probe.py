"""Does a `SessionStart` hook fire after a compaction, and does what it adds
reach the model?

`project-rules` re-delivers always-on rules after a compaction through a
`SessionStart` hook with no matcher. Before that is built, this measures
the two harnesses whose route depends on it (`experiments/<h>/session-
start-compact`), with no UZE in the loop: a hand-written POSIX `sh` hook
in the person's own user configuration keeps every payload it is handed
and answers `hookSpecificOutput.additionalContext` with a marker naming
the `source` it was given (`UZE_SS_CTX_<source>`).

One TUI session, driven as a person drives it:

1. a first turn (`TURN_ONE`);
2. `/compact`, then a turn (`TURN_TWO`);
3. `/clear`, then a turn (`TURN_THREE`).

The judge reads the hook's own payloads (which `source` values fired, in
which order) and the provider's requests: whether the request of each turn
carries the marker of the start that preceded it. A synthetic provider
answers, so this proves what reaches the request, never what a model does
with it; the compaction summary is the provider's own text.
"""

import json
import os
import shlex

from experiments import agent_context_probe as probe
from shared import capture, common

HOOK_LOG = "/work/sslog"
HOOK = "/work/ss-hook"
PROJECT = "/work/cmp-project"
TURN_ONE = "UZE_CMP_TURN_ONE"
TURN_TWO = "UZE_CMP_TURN_TWO"
TURN_THREE = "UZE_CMP_TURN_THREE"
SOURCES = ("startup", "resume", "clear", "compact")
#: What the Codex provider answers a compaction with; Claude's summary is
#: the provider's ordinary text, so it never carries it.
COMPACTION_MARK = "UZE_LAB_COMPACTION_SUMMARY"

HOOK_SH = rf"""#!/bin/sh
payload=$(cat)
mkdir -p {HOOK_LOG}
printf '%s\n' "$payload" > {HOOK_LOG}/ss-$(date +%s%N).json
src=$(printf '%s' "$payload" | sed -n 's/.*"source" *: *"\([a-z_]*\)".*/\1/p' | head -n 1)
printf '{{"hookSpecificOutput":{{"hookEventName":"SessionStart","additionalContext":"Lab session context UZE_SS_CTX_%s."}}}}' "${{src:-none}}"
"""


PROVIDER_ENV = {"DISCOVERY": "1"}
#: What the static provider answers every turn with (the Lab fixes it per
#: harness): the driver waits for it, so a turn ends on what it shows
#: rather than on its tries running out.
ANSWER = "UZE_CONFORMANCE_OK"


def marker(source):
    return f"UZE_SS_CTX_{source}"


def hooks_document():
    """The hooks document both harnesses read: `SessionStart` with no
    matcher, one command handler."""
    return {
        "hooks": {
            "SessionStart": [
                {"hooks": [{"type": "command", "command": HOOK, "timeout": 10}]}
            ]
        }
    }


def prelude(config_path):
    """The world: a Git project, the hook, and `config_path` (the person's
    own user file) carrying it."""
    return f"""
mkdir -p {PROJECT} {HOOK_LOG} $(dirname {config_path})
cd {PROJECT} && git init -q -b main . && printf 'lab project\\n' > README.md
cat > {HOOK} <<'UZE_EOF'
{HOOK_SH}UZE_EOF
chmod +x {HOOK}
printf '%s' {shlex.quote(json.dumps(hooks_document()))} > {config_path}
"""


def _wait_for(world, token, tui, reads=16):
    for _ in range(reads):
        if token in probe.provider_log(world):
            return True
        tui.collect(reads=1)
    return token in probe.provider_log(world)


def _sources(world):
    found = []
    for name, text in sorted(common.harness_files(world, HOOK_LOG).items()):
        try:
            found.append(json.loads(text).get("source"))
        except ValueError:
            found.append(f"<unreadable {name}>")
    return found


def _command(tui, bindings, command):
    """A slash command typed as a person types it; a second Enter lands on
    the completion menu when one opened, and is inert otherwise."""
    bindings.await_input(tui)
    tui.type(command)
    tui.submit()
    tui.collect(reads=2)
    tui.submit()


def drive(world, bindings, tui):
    """Runs the three steps; returns what the session left behind."""
    out = {"steps": []}
    bindings.final_markers = (ANSWER,)
    plain, ready = bindings.prepare(tui)
    out["ready"] = bool(ready)
    if not ready:
        out["log"] = probe.provider_log(world)
        return out
    probe.say("ready; turn one")
    out["turn_one"] = bindings.hook_turn(tui, f"{TURN_ONE} say hi").detail
    out["after_one"] = _sources(world)
    probe.say(f"turn one: {out['turn_one']}; /compact")
    _command(tui, bindings, "/compact")
    tui.collect(reads=10)
    out["after_compact"] = _sources(world)
    probe.say(f"after /compact: {out['after_compact']}; turn two")
    out["turn_two"] = bindings.hook_turn(tui, f"{TURN_TWO} and then?").detail
    probe.say(f"turn two: {out['turn_two']}")
    _wait_for(world, TURN_TWO, tui)
    probe.say("/clear")
    _command(tui, bindings, "/clear")
    tui.collect(reads=6)
    out["after_clear"] = _sources(world)
    probe.say(f"after /clear: {out['after_clear']}; turn three")
    out["turn_three"] = bindings.hook_turn(tui, f"{TURN_THREE} once more").detail
    _wait_for(world, TURN_THREE, tui)
    out["sources"] = _sources(world)
    out["payloads"] = common.harness_files(world, HOOK_LOG)
    out["screen"] = tui.transcript()[-4000:]
    out["log"] = probe.provider_log(world)
    return out


def _last_user(body):
    items = body.get("input") or body.get("messages") or body.get("contents") or []
    users = [i for i in items if isinstance(i, dict) and i.get("role") == "user"]
    return json.dumps(users[-1]) if users else ""


def _turn_request(reqs, token):
    """The turn's first model request: tools declared and the person's
    last message carrying `token` (a title or summary side call carries the
    turn too, but asks its own question last)."""
    for i, (h, b, raw) in enumerate(reqs):
        if b is not None and capture.declared_tools(b) and token in _last_user(b):
            return i, b
    return None, None


def judge(cfg, tag, out):
    with open(os.path.join(cfg.outdir, f"{tag}-raw-requests.log"), "w") as f:
        f.write(out.get("log", ""))
    reqs = probe.requests(out.get("log", ""))
    turns = {
        "turn_one": _turn_request(reqs, TURN_ONE),
        "turn_two": _turn_request(reqs, TURN_TWO),
        "turn_three": _turn_request(reqs, TURN_THREE),
    }
    result = {
        "ready": out.get("ready"),
        "sources_in_order": out.get("sources"),
        "after_one": out.get("after_one"),
        "after_compact": out.get("after_compact"),
        "after_clear": out.get("after_clear"),
        "turn_details": {k: out.get(k) for k in ("turn_one", "turn_two", "turn_three")},
        "requests": len(reqs),
        "turns": {},
    }
    for name, (i, body) in turns.items():
        result["turns"][name] = {
            "index": i,
            "markers": {
                s: [
                    f"{h['path']} (role={h['role']})"
                    for h in probe.locate(body, marker(s))
                ]
                for s in SOURCES
            }
            if body is not None
            else None,
            "turn_one_kept": bool(body is not None and probe.locate(body, TURN_ONE)),
            "compaction_summary_kept": bool(
                body is not None and probe.locate(body, COMPACTION_MARK)
            ),
        }
    with open(os.path.join(cfg.outdir, f"{tag}-result.json"), "w") as f:
        json.dump(result, f, indent=1)
    with open(os.path.join(cfg.outdir, f"{tag}-screen.txt"), "w") as f:
        f.write(out.get("screen", ""))
    probe.say(f"[{tag}] {json.dumps(result)[:3000]}")
    sources = out.get("sources") or []
    two = result["turns"]["turn_two"]["markers"] or {}
    three = result["turns"]["turn_three"]["markers"] or {}
    common.check(
        f"{tag}-fires-on-compact",
        "compact" in sources,
        f"SessionStart sources seen, in order: {sources}",
    )
    common.check(
        f"{tag}-compact-context-reaches-model",
        bool(two.get("compact")),
        f"turn two request #{result['turns']['turn_two']['index']}: {two}",
    )
    common.check(
        f"{tag}-fires-on-clear",
        "clear" in sources,
        f"SessionStart sources seen, in order: {sources}",
    )
    common.check(
        f"{tag}-clear-context-reaches-model",
        bool(three.get("clear")),
        f"turn three request #{result['turns']['turn_three']['index']}: {three}",
    )
