"""Does a Codex hook process run in its own session's directory when two
interactive sessions in different directories are live at once?

Codex serves interactive sessions from one shared app-server daemon
(0.157+). Session A opens in /work/project (it starts the daemon); while it
is live, session B opens in /work/project2 in the same container, and both
run one turn with a `shell` call (one without `workdir`, one with
`workdir=/work/elsewhere`). The wrapper's `HOOK_JQ` tap records, per hook,
the hook process's own `pwd -P`, the payload's `cwd`, `session_id` and the
first line of `transcript_path`.

Run: python3 conformance/lab.py --harness codex --experiment codex/hook-daemon
"""

import json
import os
import subprocess
import time

from contract import hooks
from contract.tui import Tui
from harnesses.codex.bindings import CodexBindings
from shared import common

PROJECT2 = "/work/project2"
TAPS = "/work/hook-payloads"

TAP = f"""#!/bin/sh
if [ "$1" = -e ] && [ "$2" = . ]; then
  mkdir -p {TAPS}
  t=$(mktemp {TAPS}/p.XXXXXX)
  cat > "$t.payload"
  pwd -P > "$t.pwd"
  tp=$(jq -r '.transcript_path // empty' < "$t.payload")
  [ -n "$tp" ] && head -n 1 "$tp" | jq -c '{{meta_cwd: .payload.cwd}}' > "$t.meta" 2>/dev/null
  rm -f "$t"
  exec jq "$@" < "$t.payload"
fi
exec jq "$@"
"""

ENV = (
    "export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin "
    "HOME=/work/home CODEX_HOME=/work/home/.codex UZE_HOME=/work/home/.uze "
    "OPENAI_API_KEY=uze-conformance-invalid-by-design "
    "CODEX_CA_CERTIFICATES=/app/ca.crt SSL_CERT_FILE=/app/ca.crt "
    "HOOK_JQ=/work/jqtap"
)


def _calls(bindings):
    side = f"{bindings.hook_project}/{hooks.SIDE}"
    cmd = f"mkdir -p {side} && pwd -P > {side}/$(pwd -P | tr / _)-$$.pwd # case-daemon"
    return [bindings.shell_in(cmd), bindings.shell_in(cmd, "/work/elsewhere")]


def _sh(cfg, script):
    return subprocess.run(
        ["docker", "exec", cfg.harness_container, "sh", "-c", script],
        capture_output=True,
        text=True,
        errors="replace",
    ).stdout


def run(cfg, prov_ip):
    bindings = CodexBindings()
    mode, env = bindings.sequence(_calls(bindings), hooks.PROMPT)
    prov_ip = common.start_provider(cfg, mode, env)
    before = "\n".join(
        [
            f"mkdir -p {PROJECT2} /work/elsewhere && (cd {PROJECT2} && git init -q -b main . && printf 'p2\\n' > README.md)",
            f"cat > /work/jqtap <<'TAP'\n{TAP}TAP",
            "chmod +x /work/jqtap",
            "export HOOK_JQ=/work/jqtap",
        ]
    )
    a = bindings.hook_session(cfg, prov_ip, "hook-rows", "daemon-a", before)
    container = None
    try:
        plain, ready_a = bindings.prepare(a)
        common.check("codex-daemon-a-ready", bool(ready_a), plain[-160:])
        container = cfg.harness_container
        b = Tui(
            cfg,
            [
                "docker",
                "exec",
                "-it",
                container,
                "sh",
                "-c",
                f"{ENV}; cd {PROJECT2} && exec codex",
            ],
            "codex-hooks-daemon-b",
        )
        b.expects_hook_review = True
        try:
            plain, ready_b = bindings.prepare(b)
            common.check("codex-daemon-b-ready", bool(ready_b), plain[-160:])
            processes = _sh(
                cfg,
                "for p in /proc/[0-9]*; do c=$(tr '\\0' ' ' < $p/cmdline 2>/dev/null); case $c in *codex*) echo \"$(basename $p) cwd=$(readlink $p/cwd) $c\";; esac; done",
            )
            turn_b = bindings.hook_turn(b, hooks.PROMPT)
            time.sleep(2)
            turn_a = bindings.hook_turn(a, hooks.PROMPT)
            time.sleep(2)
            files = common.harness_files(cfg, TAPS)
            pwds = sorted(
                common.harness_files(cfg, f"{bindings.hook_project}/{hooks.SIDE}")
            )
        finally:
            b.close()
    finally:
        a.close()

    rows = []
    for name, body in files.items():
        if not name.endswith(".payload"):
            continue
        stem = name[: -len(".payload")]
        try:
            p = json.loads(body)
        except ValueError:
            continue
        if p.get("hook_event_name") != "PreToolUse":
            continue
        meta = files.get(f"{stem}.meta", "").strip()
        rows.append(
            {
                "session_id": p.get("session_id"),
                "payload_cwd": p.get("cwd"),
                "process_pwd": files.get(f"{stem}.pwd", "").strip(),
                "session_meta": json.loads(meta) if meta else None,
            }
        )
    result = {
        "processes": processes,
        "turn_a_settled": turn_a.settled,
        "turn_b_settled": turn_b.settled,
        "command_pwds": pwds,
        "pre_tool_use": sorted(
            rows, key=lambda r: (str(r["session_id"]), str(r["payload_cwd"]))
        ),
    }
    with open(os.path.join(cfg.outdir, "hook-daemon-result.json"), "w") as f:
        json.dump(result, f, indent=1)
    print(json.dumps(result, indent=1), flush=True)
    by_session = {}
    for r in rows:
        by_session.setdefault(r["session_id"], set()).add(r["process_pwd"])
    common.check(
        "codex-daemon-two-sessions",
        len(by_session) == 2,
        f"sessions seen by hooks: {sorted(map(str, by_session))}",
    )
    common.check(
        "codex-daemon-process-pwd-follows-session",
        all(
            {r["process_pwd"]} == {(r["session_meta"] or {}).get("meta_cwd")}
            for r in rows
        ),
        json.dumps(
            [
                [
                    r["session_id"][:8] if r["session_id"] else None,
                    r["process_pwd"],
                    r["session_meta"],
                ]
                for r in rows
            ]
        ),
    )
