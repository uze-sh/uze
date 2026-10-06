"""How Claude Code shows a denial a hook answers, with no UZE in the hook.

The hooks contract holds that a denial UZE delivers is not shown as a
failed hook. On 2.1.290 Claude showed UZE's denial as
`PreToolUse:Bash hook error: <reason>` although the delivered wrapper
answered in the documented dialect (`permissionDecision: deny` on stdout,
exit 0). This measures the vendor's own rendering, so the check states a
promise the vendor keeps or a limit it declares, never one about UZE's
wrapper alone.

Each variation installs a plugin with no hooks, writes one hand-written
`PreToolUse` hook for `Bash` into the user's `settings.json`, and lets the
model call `Bash` once:

- `json`: prints `{"hookSpecificOutput": {"permissionDecision": "deny", ...}}`
  and exits 0, the documented decision;
- `exit2`: prints the reason on stderr and exits 2, the documented block.

Run: python3 conformance/lab.py --harness claude --experiment claude/deny-render
"""

import json

from harnesses.claude.bindings import ClaudeBindings
from shared import common

REASON = "hand-hook-denied"

HOOKS = {
    "json": (
        "printf '%s' "
        + json.dumps(
            json.dumps(
                {
                    "hookSpecificOutput": {
                        "hookEventName": "PreToolUse",
                        "permissionDecision": "deny",
                        "permissionDecisionReason": REASON,
                    }
                }
            )
        )
        + "; exit 0"
    ),
    "exit2": f"echo {REASON} >&2; exit 2",
}


def _settings(command):
    """The shell that writes the hand hook and splices it into the user's
    `settings.json` — the image has no Python or jq, so the `hooks` key goes
    in after the opening brace of whatever `uze install` left there."""
    script = "/work/hand-hook"
    hooks = json.dumps(
        {
            "PreToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [{"type": "command", "command": script}],
                }
            ]
        },
        separators=(",", ":"),
    )
    return f"""
printf '#!/bin/sh\\ncat >/dev/null\\n%s\\n' {json.dumps(command)} > {script}
chmod +x {script}
settings=/work/home/.claude/settings.json
if grep -q '"' "$settings" 2>/dev/null; then
  sed -i '0,/{{/s|{{|{{"hooks":{hooks},|' "$settings"
else
  printf '%s' '{{"hooks":{hooks}}}' > "$settings"
fi
"""


def run(cfg, prov_ip):
    bindings = ClaudeBindings()
    prompt = "LAB_HOOK_TURN run the step"
    for variation, command in HOOKS.items():
        calls = [{"tool": "Bash", "args": {"command": "echo ran > /work/hand-side"}}]
        mode, env = bindings.sequence(calls, prompt)
        prov_ip = common.start_provider(cfg, mode, env)
        with bindings.hook_session(
            cfg, prov_ip, "flow", f"deny-render-{variation}", before=_settings(command)
        ) as tui:
            plain, ready = bindings.prepare(tui)
            if not ready:
                common.check(
                    f"deny-render-{variation}-ready",
                    ready,
                    plain[-160:].replace("\n", " "),
                )
                continue
            turn = bindings.hook_turn(tui, prompt)
            tui.snapshot(f"deny-render-{variation}", turn.plain)
            shown_as_error = bindings.hook_error(turn.plain)
            reason_shown = REASON in common.squash(turn.plain)
        common.check(
            f"deny-render-{variation}-reason-shown",
            reason_shown,
            "the reason reached the screen" if reason_shown else "no reason on screen",
        )
        common.check(
            f"deny-render-{variation}-not-an-error",
            not shown_as_error,
            "Claude showed the denial as a decision"
            if not shown_as_error
            else "Claude showed the denial as a hook error",
        )
