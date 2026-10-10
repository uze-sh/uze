"""Exploration: can UZE inject a file-trail hook into ONE Antigravity
process at launch, without writing into the user's own configuration, and
does the hook learn which files the agent wrote anywhere on the machine?

The candidate mechanism is `--add-dir <dir>`: a directory added to the
session's workspace is a customization root, so `<dir>/.agents/hooks.json`
is loaded for that process alone, merged with the user's own
`~/.gemini/config/hooks.json`. The hook is hand-written POSIX `sh` (the
image has no Python or jq), with no UZE in its path: it appends the
`TargetFile` of every PostToolUse payload to the file named by
`UZE_AGENT_TRAIL`, which reaches it through the launch environment.

Two TUI sessions, the same scripted turn in each (in the tools' declared
argument shapes): `write_to_file` into `/work/other-repo` — a second Git
repository outside the workspace `/work/project` — then `run_command`
`git -C /work/other-repo status`. Both launch with `UZE_AGENT_TRAIL` and
`GIT_TRACE2_EVENT=/work/trace` in the environment; only the first adds the
launch directory.

  inject   `agy --add-dir /work/uze-launch`
  control  plain `agy` — the user's hook alone must run, the trail must
           stay empty

Every permission prompt is answered on screen (approve), and counted per
session, so the prompts the injection adds are the difference.

Switches (environment):
  LAUNCH_HOOK_AUTH=apikey  run on the Gemini API key instead of signed in.

The `json-hooks-enabled` gate (`CustomizationConfig.enable_json_hooks`) is a
server-side Unleash flag / `listExperiments` flag with no local setting in
the binary; the Lab's provider replays it as recorded live, so this
experiment measures the hook path on the gate a real account receives.

Finding (2026-10-10, 1.3.1, signed in, 13/13): `--add-dir` loads the
launch dir's `.agents/hooks.json` for that process only (`/hooks` lists
`uze-trail` beside the user's `user-own`; log `loaded 2 named hooks from 2
hooks.json file(s)`), and it fires in the TUI. PostToolUse stdin carries
`toolCall.name` and `toolCall.args.TargetFile` (absolute, as the model
wrote it), plus `workspacePaths` — which now include the launch dir. The
handler inherits the launch env (`UZE_AGENT_TRAIL`) and appended
`/work/other-repo/touched-by-agent.txt`. The user's hook ran in both
sessions and its file stayed byte-identical; the injection added no prompt
(both sessions: "Allow creation of this file? Reason: outside workspace",
then "Run this command?"). The control loaded 1 hook and wrote no trail.
`GIT_TRACE2_EVENT` reached the agent's git: one trace file with
`def_repo.worktree=/work/other-repo`, and two more from the harness's own
`git merge-base` in `/work/project`. The launch dir is recorded in the
conversation's own state (conversations/*.db, summaries), not in config,
and `agy --continue -p /hooks` without the flag lists only the user's hook.

Run: python3 conformance/lab.py --harness antigravity --experiment antigravity/launch-hook
"""

import json
import os
import re
import subprocess
import time

import pexpect

from harnesses.antigravity.scenarios import (
    PERMISSION_PROMPTS,
    PROMPT_MARKER,
    agy_setup,
    answer_first_run,
    await_screen,
)
from shared import common

LAUNCH_DIR = "/work/uze-launch"
TRAIL = "/work/trail.log"
TRACE_DIR = "/work/trace"
EVIDENCE = "/work/evidence"
TOUCHED = "/work/other-repo/touched-by-agent.txt"
FINAL = "UZE_CONFORMANCE_PASS"

SEQUENCE = [
    {
        "name": "write_to_file",
        "args": {
            "TargetFile": TOUCHED,
            "CodeContent": "lab",
            "Description": "lab",
            "Overwrite": True,
            "toolSummary": "Lab write",
            "toolAction": "Writing file",
        },
    },
    {
        "name": "run_command",
        "args": {
            "CommandLine": "git -C /work/other-repo status",
            "Cwd": "/work/project",
            "WaitMsBeforeAsync": 2000,
            "toolSummary": "Command execution",
            "toolAction": "Running command",
        },
    },
]

#: The hook UZE would generate: stdin captured as evidence, the env it saw,
#: and the `TargetFile` appended to the trail. `tr` + `sed` because the image
#: has no JSON tool; a path with an escaped quote would defeat it, which is
#: a limitation of the probe, not of the harness.
TRAIL_HOOK = r"""#!/bin/sh
in=$(cat)
printf '%s\n' "$in" >> /work/evidence/launch-hook-stdin.log
printf 'UZE_AGENT_TRAIL=%s\n' "${UZE_AGENT_TRAIL-<unset>}" >> /work/evidence/launch-hook-env.log
f=$(printf '%s' "$in" | tr -d '\n' | sed -n 's/.*"TargetFile" *: *"\([^"]*\)".*/\1/p')
if [ -n "$f" ] && [ -n "$UZE_AGENT_TRAIL" ]; then
  printf '%s\n' "$f" >> "$UZE_AGENT_TRAIL"
fi
printf '{}'
"""
USER_HOOK = r"""#!/bin/sh
cat > /dev/null
echo fired >> /work/evidence/user-hook.log
printf '{}'
"""


def world(inject):
    """The machine before launch: two repositories, the user's own hook,
    and — for the injected session — the launch directory UZE would own."""
    lines = [
        f"mkdir -p /work/project /work/other-repo {TRACE_DIR} {EVIDENCE} /work/home/.gemini/config",
        "git -C /work/project init -q && git -C /work/other-repo init -q",
        "echo seed > /work/other-repo/seed.txt",
        "cat > /work/user-hook.sh <<'EOF'\n" + USER_HOOK + "EOF",
        "cat > /work/home/.gemini/config/hooks.json <<'EOF'\n"
        '{"user-own":{"PostToolUse":[{"matcher":"*","hooks":[{"command":"sh /work/user-hook.sh"}]}]}}\n'
        "EOF",
        "sha256sum /work/home/.gemini/config/hooks.json > /work/evidence/user-config.before",
    ]
    if inject:
        lines += [
            f"mkdir -p {LAUNCH_DIR}/.agents",
            f"cat > {LAUNCH_DIR}/trail.sh <<'EOF'\n" + TRAIL_HOOK + "EOF",
            f"cat > {LAUNCH_DIR}/.agents/hooks.json <<'EOF'\n"
            '{"uze-trail":{"PostToolUse":[{"matcher":"*",'
            f'"hooks":[{{"command":"sh {LAUNCH_DIR}/trail.sh","timeout":5}}]}}]}}}}\n'
            "EOF",
        ]
    return "\n".join(lines)


def launch(inject):
    add = f"--add-dir {LAUNCH_DIR}" if inject else ""
    env = f"UZE_AGENT_TRAIL={TRAIL} GIT_TRACE2_EVENT={TRACE_DIR}"
    # What the process registered, read by the harness itself before the
    # TUI starts — the same flags, headless, which is fine for a listing.
    return f"""cd /work/project
env {env} agy {add} -p "/hooks" --output-format json --print-timeout 60s \\
  --log-file /work/evidence/hooks-listing.log > /work/evidence/hooks-listing.json 2>&1 || true
exec env {env} agy {add} --log-file /work/agy.log"""


INSPECT = r"""
cd /work
echo '----- hooks listing -----'; cat evidence/hooks-listing.json 2>&1 | head -c 2500; echo
echo '----- trail -----'; cat trail.log 2>&1
echo '----- launch hook env -----'; cat evidence/launch-hook-env.log 2>&1
echo '----- launch hook stdin -----'; cat evidence/launch-hook-stdin.log 2>&1 | head -c 4000; echo
echo '----- user hook -----'; cat evidence/user-hook.log 2>&1
echo '----- touched -----'; ls -la /work/other-repo 2>&1; git -C /work/other-repo status --short 2>&1
echo '----- user config -----'; sha256sum -c evidence/user-config.before 2>&1
echo '----- config tree -----'; find /work/home/.gemini/config | sort
echo '----- state naming launch dir -----'; grep -rl 'uze-launch' /work/home/.gemini 2>/dev/null | grep -v '/brain/' || true
echo '----- trace files -----'; ls trace 2>&1 | wc -l
echo '----- trace worktrees -----'; cat trace/* 2>/dev/null | grep -o '"event":"def_repo"[^}]*"worktree":"[^"]*"' | sed 's/.*"worktree":"//; s/"$//' | sort | uniq -c
echo '----- trace argv -----'; cat trace/* 2>/dev/null | grep -o '"event":"start"[^}]*"argv":\[[^]]*\]' | sed 's/.*"argv"://' | sort | uniq -c | head -20
echo '----- continued hooks listing -----'
(export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin SSL_CERT_FILE=/app/ca.crt SSL_CERT_DIR=/app AGY_CLI_DISABLE_AUTO_UPDATE=1
 cd /work/project && timeout 90 agy --continue -p "/hooks" --output-format json --print-timeout 60s --log-file /work/evidence/continued.log 2>&1 | head -c 2500; echo
 grep -n 'workspaceDirs\|hooks_manager' /work/evidence/continued.log | head -5)
echo '----- agy.log hooks -----'; grep -n -iE 'hook|add.?dir|workspace' agy.log 2>/dev/null | grep -v Migration | head -40
"""


def section(text, name):
    m = re.search(rf"----- {re.escape(name)} -----\n(.*?)(?=\n----- |\Z)", text, re.S)
    return m.group(1).strip() if m else ""


def session(cfg, prov_ip, tag, inject):
    common.start_provider(
        cfg,
        "toolcall",
        {"TOOL_SEQUENCE": json.dumps(SEQUENCE), "FINAL_TEXT": FINAL},
    )
    time.sleep(1)
    setup = agy_setup(
        cfg,
        prov_ip,
        include_mcp=False,
        final_cmd=launch(inject),
        plugins="flow",
        prelude=world(inject),
        auth="apikey" if os.environ.get("LAUNCH_HOOK_AUTH") == "apikey" else "consumer",
    )
    cmd = common.docker_base(cfg, prov_ip, setup)
    name = cfg.harness_container
    child = pexpect.spawn(
        cmd[0], cmd[1:], encoding="utf-8", codec_errors="replace", timeout=300
    )
    child.setwinsize(50, 160)
    child.logfile_read = common.CastRecorder(cfg.outdir, f"launch-hook-{tag}")
    screen = common.make_screen(child)

    first = answer_first_run(child, screen) or ""
    # A second trust question, for the added directory, would be a prompt
    # the injection causes: answer it on screen like the first, and count it.
    extra_trust = 0
    for _ in range(3):
        if "trust the contents" in first[-1500:] and PROMPT_MARKER not in first[-400:]:
            extra_trust += 1
            child.send("\r")
            first, _ = await_screen(child, ["trust the contents", PROMPT_MARKER], 8)
        else:
            break
    with open(f"{cfg.outdir}/{tag}_start.raw", "w") as f:
        f.write(first)

    for ch in "update the other repository":
        child.send(ch)
        time.sleep(0.08)
    child.send("\r")
    seen_raw = ""
    seen = ""
    prompts = []
    for _ in range(40):
        t, p = screen(2.0)
        seen_raw += t
        seen += p
        rendered = common.render_screen(seen_raw)
        if FINAL in seen or FINAL in rendered:
            break
        if not child.isalive():
            break
        if any(m in p for m in PERMISSION_PROMPTS):
            question = next(
                (
                    q
                    for q in ("Run this command?", "Allow", "file", "edit", "write")
                    if q in p
                ),
                "prompt",
            )
            prompts.append(question)
            child.send("\r")
            time.sleep(1.5)
        elif "How's the CLI experience" in p:
            child.send("0\r")
            time.sleep(1.0)
    settled = FINAL in seen or FINAL in common.render_screen(seen_raw)
    with open(f"{cfg.outdir}/{tag}_turn.raw", "w") as f:
        f.write(f"{seen}\n===== rendered =====\n{common.render_screen(seen_raw)}")
    common.settle_and_quiet(screen)

    inspected = subprocess.run(
        ["docker", "exec", name, "sh", "-c", INSPECT],
        capture_output=True,
        text=True,
        errors="replace",
        timeout=60,
    ).stdout
    with open(f"{cfg.outdir}/{tag}_inspect.txt", "w") as f:
        f.write(inspected)
    print(f"===== {tag} =====\n{inspected[-7000:]}", flush=True)
    child.close(force=True)
    subprocess.run(["docker", "rm", "-f", name], capture_output=True)
    struct = common.provider_struct(cfg)
    with open(f"{cfg.outdir}/{tag}_struct.json", "w") as f:
        json.dump(struct, f, indent=1)
    return {
        "settled": settled,
        "prompts": prompts,
        "extra_trust": extra_trust,
        "inspect": inspected,
    }


def run(cfg, prov_ip):
    inject = session(cfg, prov_ip, "inject", inject=True)
    control = session(cfg, prov_ip, "control", inject=False)

    listing = section(inject["inspect"], "hooks listing")
    common.check(
        "launch-hook-turn-settled",
        inject["settled"],
        f"inject turn reached {FINAL}; prompts answered: {inject['prompts']}",
    )
    # 1. Injection: loaded for this process, from the added directory.
    common.check(
        "launch-hook-loaded-via-add-dir",
        "uze-trail" in listing and f"{LAUNCH_DIR}/.agents/hooks.json" in listing,
        "`agy --add-dir` /hooks lists uze-trail from the launch dir"
        if "uze-trail" in listing
        else f"not listed: {listing[:300]}",
    )
    hook_env = section(inject["inspect"], "launch hook env")
    common.check(
        "launch-hook-fired-in-tui",
        bool(hook_env),
        f"PostToolUse runs: {len(hook_env.splitlines())}",
    )
    # 2. Coexistence with the user's own hook, file untouched.
    # The harness itself creates files under ~/.gemini/config on any run
    # (`.migrated`, `agents/`, `projects/`, …), so "untouched" is judged as
    # the user's hooks.json byte-identical and the tree identical to the
    # control session's: whatever the injection adds is the difference.
    config = section(inject["inspect"], "user config")
    tree, c_tree = (section(x["inspect"], "config tree") for x in (inject, control))
    common.check(
        "launch-hook-coexists-with-user-hook",
        "user-own" in listing
        and bool(section(inject["inspect"], "user hook"))
        and ": OK" in config
        and tree == c_tree,
        f"user hook runs={len(section(inject['inspect'], 'user hook').splitlines())}; "
        f"{config}; config tree same as control: {tree == c_tree}",
    )
    # The conversation records its workspace directories, the added one
    # included; anything else naming it would outlive the process.
    persisted = section(inject["inspect"], "state naming launch dir").splitlines()
    outside = [
        p
        for p in persisted
        if not re.search(
            r"/(conversations/|conversation_summaries|jetbox_summaries)", p
        )
    ]
    common.check(
        "launch-hook-dir-only-in-conversation-state",
        not outside,
        "recorded in: " + "; ".join(persisted) if persisted else "named nowhere",
    )
    # Whether resuming that conversation without the flag revives the hook.
    continued = section(inject["inspect"], "continued hooks listing")
    common.check(
        "launch-hook-not-revived-by-continue",
        '"status":"SUCCESS"' in continued and "uze-trail" not in continued,
        continued[:600].replace("\n", " "),
    )
    # 3. The path: absolute TargetFile on stdin, env inherited, trail appended.
    stdin = section(inject["inspect"], "launch hook stdin")
    trail = section(inject["inspect"], "trail")
    common.check(
        "launch-hook-stdin-has-targetfile",
        f'"TargetFile":"{TOUCHED}"' in stdin.replace(" ", ""),
        stdin[:400].replace("\n", " "),
    )
    common.check(
        "launch-hook-sees-trail-env",
        f"UZE_AGENT_TRAIL={TRAIL}" in hook_env,
        hook_env.splitlines()[0] if hook_env else "hook never ran",
    )
    common.check(
        "launch-hook-trail-appended",
        TOUCHED in trail,
        f"trail: {trail[:200]!r}",
    )
    # 4. Prompts the injection caused: the difference between the sessions.
    common.check(
        "launch-hook-adds-no-prompt",
        inject["extra_trust"] == control["extra_trust"]
        and len(inject["prompts"]) == len(control["prompts"]),
        f"inject: trust+{inject['extra_trust']} prompts={inject['prompts']}; "
        f"control: trust+{control['extra_trust']} prompts={control['prompts']}",
    )
    # 5. Sanity: without the flag, nothing of ours runs; the user's hook does.
    c_listing = section(control["inspect"], "hooks listing")
    common.check(
        "launch-hook-control-settled",
        control["settled"],
        f"control turn reached {FINAL}",
    )
    common.check(
        "launch-hook-absent-without-injection",
        control["settled"]
        and "uze-trail" not in c_listing
        and not section(control["inspect"], "launch hook env").startswith("UZE")
        and not section(control["inspect"], "trail").startswith("/")
        and bool(section(control["inspect"], "user hook")),
        f"control trail={section(control['inspect'], 'trail')[:120]!r}; "
        f"user hook runs={len(section(control['inspect'], 'user hook').splitlines())}",
    )
    # 6. trace2 from the launch env reaches the agent's git.
    worktrees = section(inject["inspect"], "trace worktrees")
    common.check(
        "launch-trace2-names-other-repo",
        "/work/other-repo" in worktrees,
        f"files={section(inject['inspect'], 'trace files')}; worktrees: "
        + worktrees.replace("\n", "; "),
    )
