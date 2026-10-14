"""Whether Claude Code blocks a tool call whose hook cannot run, when the
hook carries `onFailure: "block"` (2.1.295), with no UZE in the hook.

Each variation writes one `PreToolUse` hook for `Bash` into the user's
`settings.json` and lets the model call `Bash` once. The command echoes a
nonce, so the verbatim requests (`--discovery`) say whether it ran:

- `missing-block`: the hook command does not exist, `onFailure: "block"`;
- `missing-plain`: the same without the key (the control: it runs);
- `timeout-block`: the hook sleeps past a 2 s timeout, `onFailure: "block"`;
- `deny-with-key` / `deny-plain`: a hook that denies, with and without the
  key — run on a Claude older than 2.1.295 (`UZE_LAB_IMAGE`) to see whether
  the key costs the hook its loading.

Run: python3 conformance/lab.py --harness claude --experiment claude/on-failure --discovery
"""

import json
import os

from harnesses.claude.bindings import ClaudeBindings
from shared import common

SIDE = "UZEONFAILSIDE7Q"

VARIATIONS = {
    "missing-block": {"command": "/work/no-such-hook", "onFailure": "block"},
    "missing-plain": {"command": "/work/no-such-hook"},
    "timeout-block": {"command": "/work/slow-hook", "timeout": 2, "onFailure": "block"},
    # On a Claude older than the key: does it still load a hook that has it?
    "deny-with-key": {"command": "/work/deny-hook", "onFailure": "block"},
    "deny-plain": {"command": "/work/deny-hook"},
}


def _settings(hook):
    hooks = json.dumps(
        {"PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", **hook}]}]},
        separators=(",", ":"),
    )
    return f"""
printf '#!/bin/sh\\nsleep 30\\n' > /work/slow-hook && chmod +x /work/slow-hook
printf '#!/bin/sh\\ncat >/dev/null\\necho denied >&2\\nexit 2\\n' > /work/deny-hook && chmod +x /work/deny-hook
settings=/work/home/.claude/settings.json
if grep -q '"' "$settings" 2>/dev/null; then
  sed -i '0,/{{/s|{{|{{"hooks":{hooks},|' "$settings"
else
  printf '%s' '{{"hooks":{hooks}}}' > "$settings"
fi
"""


def _requests(cfg):
    common.pull_captures(cfg)
    path = os.path.join(cfg.outdir, "raw-requests.log")
    if not os.path.exists(path):
        return ""
    with open(path, errors="replace") as f:
        return f.read()


def run(cfg, prov_ip):
    bindings = ClaudeBindings()
    only = os.environ.get("ONFAIL_VARIATIONS")
    for variation, hook in VARIATIONS.items():
        if only and variation not in only.split(","):
            continue
        prompt = f"LAB_HOOK_TURN run the step {variation}"
        calls = [{"tool": "Bash", "args": {"command": f"echo {SIDE}-{variation}"}}]
        mode, env = bindings.sequence(calls, prompt)
        prov_ip = common.start_provider(cfg, mode, env)
        with bindings.hook_session(
            cfg, prov_ip, "flow", f"on-failure-{variation}", before=_settings(hook)
        ) as tui:
            plain, ready = bindings.prepare(tui)
            if not ready:
                common.check(
                    f"on-failure-{variation}-ready",
                    ready,
                    plain[-160:].replace("\n", " "),
                )
                continue
            turn = bindings.hook_turn(tui, prompt)
            tui.snapshot(f"on-failure-{variation}", turn.plain)
        ran = f"{SIDE}-{variation}" in _requests(cfg).replace(
            f"echo {SIDE}-{variation}", ""
        )
        common.check(
            f"on-failure-{variation}-ran",
            ran,
            "the Bash call ran" if ran else "the Bash call was blocked",
        )
