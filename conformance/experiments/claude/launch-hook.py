"""Whether a hook injected at launch, scoped to one Claude Code process,
can report every file the agent edits, anywhere on the machine.

The workspace wants a trail of what an agent it launched touched, without
writing into the person's own configuration. The candidate mechanism is
entirely on the command line and in the launch environment:

- `claude --settings /work/uze-launch/settings.json` (a file outside
  `~/.claude`), carrying a `PostToolUse` hook on the file-editing tools;
- `UZE_AGENT_TRAIL=<file>` in the launch environment, which the hook
  appends each edited path to;
- `GIT_TRACE2_EVENT=<dir>` in the launch environment, so every Git the
  agent runs writes a trace naming its repository.

The hook is hand-written POSIX `sh` (the image has no Python or jq): no UZE
is in the path. The person's own `~/.claude/settings.json` carries a
different `PostToolUse` hook, so the run also says whether `--settings`
merges with it or replaces it.

Each variation is one world of its own:

- `injected`: headless `claude -p` with the injection, scripting a Write
  and an Edit in another repository outside the working directory, a Write
  inside it, and `git -C <other> status` through Bash;
- `plain`: the same turn with no injection, the sanity case;
- `tui`: the injection on an interactive session, answered as a person
  answers it, to see whether the injected hook draws any prompt.

Run: python3 conformance/lab.py --harness claude --experiment claude/launch-hook
Env: LAUNCH_HOOK_VARIATIONS=injected,plain,tui
"""

import json
import os
import shlex
import subprocess
import time

from contract.tui import Tui
from harnesses.claude.bindings import ClaudeBindings
from harnesses.claude.scenarios import claude_container, generate_certs
from shared import common

VARIATIONS = os.environ.get("LAUNCH_HOOK_VARIATIONS", "injected,plain,tui").split(",")
PROMPT = "LAB_LAUNCH_TRAIL_TURN edit the files"
PROJECT = "/work/project"
OTHER = "/work/other-repo"
EVIDENCE = "/work/evidence"
TRAIL = f"{EVIDENCE}/trail.log"
TRACE = "/work/trace"
LAUNCH_SETTINGS = "/work/uze-launch/settings.json"
EDIT_MATCHER = "Write|Edit|MultiEdit|NotebookEdit"
NOTEBOOK = f"{OTHER}/lab.ipynb"
#: A file written by the shell rather than by an editing tool: no
#: `PostToolUse` on the editing tools can see it.
BASH_WRITTEN = f"{OTHER}/by-shell.txt"

CALLS = [
    {"tool": "Write", "args": {"file_path": f"{OTHER}/edited.txt", "content": "one\n"}},
    {
        "tool": "Edit",
        "args": {
            "file_path": f"{OTHER}/edited.txt",
            "old_string": "one",
            "new_string": "two",
            "replace_all": False,
        },
    },
    {
        "tool": "Write",
        "args": {"file_path": f"{PROJECT}/inside.txt", "content": "in\n"},
    },
    {"tool": "ToolSearch", "args": {"query": "select:NotebookEdit", "max_results": 1}},
    {"tool": "Read", "args": {"file_path": NOTEBOOK}},
    {
        "tool": "NotebookEdit",
        "args": {
            "notebook_path": NOTEBOOK,
            "new_source": "x = 1",
            "cell_type": "code",
            "edit_mode": "insert",
        },
    },
    {
        "tool": "Bash",
        "args": {
            "command": f"git -C {OTHER} status --short; echo shell > {BASH_WRITTEN}",
            "description": "status of the other repository",
        },
    },
]

#: The launch hook: what a workspace would inject. It keeps every payload it
#: was handed and the environment it saw, so the evidence is the hook's own.
LAUNCH_HOOK = r"""#!/bin/sh
payload=$(cat)
n=$(date +%s%N)
printf '%s\n' "$payload" > /work/evidence/launch-stdin-$n.json
printf 'trail=%s trace=%s\n' "${UZE_AGENT_TRAIL-<unset>}" "${GIT_TRACE2_EVENT-<unset>}" >> /work/evidence/launch-env.log
path=$(printf '%s' "$payload" | sed -n 's/.*"tool_input" *: *{[^}]*"\(file_path\|notebook_path\)" *: *"\([^"]*\)".*/\2/p' | head -n 1)
if [ -n "$UZE_AGENT_TRAIL" ] && [ -n "$path" ]; then
  printf '%s\n' "$path" >> "$UZE_AGENT_TRAIL"
fi
exit 0
"""

#: The person's own hook, in their own settings: it only says it ran.
USER_HOOK = r"""#!/bin/sh
tool=$(cat | sed -n 's/.*"tool_name" *: *"\([^"]*\)".*/\1/p' | head -n 1)
printf 'user-hook %s\n' "$tool" >> /work/evidence/user-hook.log
exit 0
"""


def _hooks(matcher, command):
    return {
        "hooks": {
            "PostToolUse": [
                {"matcher": matcher, "hooks": [{"type": "command", "command": command}]}
            ]
        }
    }


def world_setup(injected):
    """The shell that builds the world: two repositories, the person's own
    hook in `~/.claude/settings.json`, and — when `injected` — the launch
    settings file and hook outside `~/.claude`."""
    user_settings = json.dumps(_hooks("Write|Edit|Bash", "/work/user-hook"))
    launch_settings = json.dumps(_hooks(EDIT_MATCHER, "/work/uze-launch/trail-hook"))
    git = (
        "git -c init.defaultBranch=main -c user.name=Lab -c user.email=lab@uze.invalid"
    )
    script = f"""
mkdir -p {EVIDENCE} {PROJECT} {OTHER} /work/home/.claude
{git} -C {PROJECT} init -q && printf 'lab project\\n' > {PROJECT}/README.md
{git} -C {OTHER} init -q && printf 'other\\n' > {OTHER}/README.md
printf '%s' '{{"cells":[],"metadata":{{}},"nbformat":4,"nbformat_minor":5}}' > {NOTEBOOK}
cat > /work/user-hook <<'UZE_EOF'
{USER_HOOK}UZE_EOF
chmod +x /work/user-hook
printf '%s' {shlex.quote(user_settings)} > /work/home/.claude/settings.json
"""
    if injected:
        script += f"""
mkdir -p /work/uze-launch {TRACE}
cat > /work/uze-launch/trail-hook <<'UZE_EOF'
{LAUNCH_HOOK}UZE_EOF
chmod +x /work/uze-launch/trail-hook
printf '%s' {shlex.quote(launch_settings)} > {LAUNCH_SETTINGS}
"""
    return script


def launch(injected):
    """The launch line: the environment and the flag, or neither."""
    if not injected:
        return "claude"
    return (
        f"env UZE_AGENT_TRAIL={TRAIL} GIT_TRACE2_EVENT={TRACE} "
        f"claude --settings {LAUNCH_SETTINGS}"
    )


REPORT = f"""
echo '=== settings-user'
cat /work/home/.claude/settings.json
echo '=== trail'
cat {TRAIL} 2>&1
echo '=== launch-env'
cat {EVIDENCE}/launch-env.log 2>&1
echo '=== launch-stdin'
for f in {EVIDENCE}/launch-stdin-*.json; do [ -f "$f" ] && cat "$f"; done
echo '=== user-hook'
cat {EVIDENCE}/user-hook.log 2>&1
echo '=== trace-files'
ls -A {TRACE} 2>/dev/null | wc -l
echo '=== trace-repos'
cat {TRACE}/* 2>/dev/null | sed -n 's/.*"event":"def_repo".*"worktree":"\\([^"]*\\)".*/\\1/p' | sort | uniq -c
echo '=== trace-argv'
cat {TRACE}/* 2>/dev/null | sed -n 's/.*"event":"start".*"argv":\\(\\[[^]]*\\]\\).*/\\1/p' | sort | uniq -c
echo '=== files'
cat {OTHER}/edited.txt {PROJECT}/inside.txt {BASH_WRITTEN} {NOTEBOOK} 2>&1
"""


def headless(world, prov_ip, injected):
    script = f"""
set -e
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin
export HOME=/work/home CLAUDE_CONFIG_DIR=/work/home/.claude
export ANTHROPIC_API_KEY=uze-conformance-invalid-by-design
export NODE_EXTRA_CA_CERTS=/app/ca.crt
cp /app/fixtures/claude.json /work/home/.claude.json
{world_setup(injected)}
set +e
cd {PROJECT}
echo '=== turn'
# decision: headless-permissions
timeout 150 {launch(injected)} -p {shlex.quote(PROMPT)} --permission-mode bypassPermissions --output-format json 2>&1
rc=$?; echo '=== turn-exit'; echo "exit $rc"
{REPORT}
"""
    cmd = common.docker_base(world, prov_ip, script, tty=False)
    ca_crt, _, _ = generate_certs(world)
    i = cmd.index(common.HARNESS_IMAGE)
    cmd = cmd[:i] + ["-v", f"{ca_crt}:/app/ca.crt:ro"] + cmd[i:]
    proc = subprocess.run(cmd, capture_output=True, text=True, timeout=400)
    return sections(proc.stdout + proc.stderr)


def sections(stdout):
    out, name = {}, "pre"
    for line in stdout.splitlines():
        if line.startswith("=== "):
            name = line[4:].strip()
            out[name] = []
        else:
            out.setdefault(name, []).append(line)
    return {k: "\n".join(v) for k, v in out.items()}


def headless_job(injected):
    def job(world):
        mode, env = ClaudeBindings().sequence(CALLS, PROMPT)
        prov_ip = common.start_provider(world, mode, env)
        time.sleep(1)
        return headless(world, prov_ip, injected)

    return job


def tui_job(world):
    """The injection on an interactive session; every prompt the session
    draws is recorded, and each is answered with Enter as a person does."""
    bindings = ClaudeBindings()
    mode, env = bindings.sequence(CALLS[:1], PROMPT)
    prov_ip = common.start_provider(world, mode, env)
    final = f"{world_setup(True)}\ncd {PROJECT} && exec {launch(True)}"
    tui = Tui(
        world, claude_container(world, prov_ip, final, plugins=""), "launch-hook-tui"
    )
    with tui:
        onboarding, ready = bindings.prepare(tui)
        if not ready:
            return {"ready": False, "onboarding": onboarding}
        turn = bindings.hook_turn(tui, PROMPT)
        tui.snapshot("launch-hook-tui", turn.plain)
        files = common.harness_files(world, EVIDENCE)
    return {
        "ready": True,
        "onboarding": onboarding,
        "approvals": turn.approvals,
        "settled": turn.detail,
        "plain": turn.plain,
        "files": files,
    }


def run(cfg, prov_ip):
    generate_certs(cfg)
    jobs = {}
    if "injected" in VARIATIONS:
        jobs["lh-injected"] = headless_job(True)
    if "plain" in VARIATIONS:
        jobs["lh-plain"] = headless_job(False)
    if "tui" in VARIATIONS:
        jobs["lh-tui"] = tui_job
    results = common.concurrently(cfg, jobs)
    with open(os.path.join(cfg.outdir, "launch-hook.json"), "w") as f:
        json.dump(results, f, indent=1, default=str)

    injected = results.get("lh-injected")
    if injected:
        judge_injected(injected)
    plain = results.get("lh-plain")
    if plain:
        judge_plain(plain)
    tui = results.get("lh-tui")
    if tui:
        judge_tui(tui)


def _line(text, limit=300):
    return " | ".join(line for line in text.splitlines() if line.strip())[:limit]


def judge_injected(s):
    trail = s.get("trail", "")
    stdin = s.get("launch-stdin", "")
    user = s.get("user-hook", "")
    common.check(
        "launch-hook-turn-ran",
        "two" in s.get("files", "") and "in" in s.get("files", ""),
        f"exit: {_line(s.get('turn-exit', '?'))}; files: {_line(s.get('files', ''))}",
    )
    common.check(
        "launch-hook-injected-fired",
        bool(stdin.strip()),
        f"launch hook payloads: {len([line for line in stdin.splitlines() if line.strip()])}",
    )
    common.check(
        "launch-hook-coexists-with-user-hook",
        bool(stdin.strip()) and "user-hook Write" in user,
        f"user hook: {_line(user)}",
    )
    common.check(
        "launch-hook-stdin-absolute-path",
        f'"file_path":"{OTHER}/edited.txt"' in stdin.replace(" ", ""),
        _line(stdin, 600),
    )
    common.check(
        "launch-hook-sees-trail-env",
        f"trail={TRAIL}" in s.get("launch-env", ""),
        _line(s.get("launch-env", "")),
    )
    common.check(
        "launch-hook-trail-has-other-repo",
        f"{OTHER}/edited.txt" in trail and f"{PROJECT}/inside.txt" in trail,
        f"trail: {_line(trail)}",
    )
    common.check(
        "launch-hook-trail-has-notebook",
        NOTEBOOK in trail,
        f"NotebookEdit payload names the notebook: {'notebook_path' in stdin}",
    )
    # A limitation, recorded as what was measured: a file the shell writes
    # never passes through an editing tool's hook.
    shell_written = "shell" in s.get("files", "")
    common.check(
        "launch-hook-shell-write-not-in-trail",
        shell_written and BASH_WRITTEN not in trail,
        f"the shell wrote {BASH_WRITTEN}: {shell_written}; in the trail: "
        f"{BASH_WRITTEN in trail}",
    )
    common.check(
        "launch-hook-trace2-other-repo",
        OTHER in s.get("trace-repos", ""),
        f"files: {_line(s.get('trace-files', ''))}; repos: {_line(s.get('trace-repos', ''))}",
    )


def judge_plain(s):
    common.check(
        "launch-hook-plain-turn-ran",
        "two" in s.get("files", ""),
        f"files: {_line(s.get('files', ''))}",
    )
    nothing_extra = (
        not s.get("launch-stdin", "").strip()
        and "No such file" in s.get("trail", "")
        and s.get("trace-files", "").strip() in ("0", "")
    )
    common.check(
        "launch-hook-plain-runs-nothing-extra",
        nothing_extra,
        f"trail: {_line(s.get('trail', ''))}; launch payloads: "
        f"{len(s.get('launch-stdin', '').strip())} bytes; trace files: "
        f"{_line(s.get('trace-files', ''))}",
    )
    common.check(
        "launch-hook-plain-user-hook-ran",
        "user-hook Write" in s.get("user-hook", ""),
        _line(s.get("user-hook", "")),
    )


def judge_tui(r):
    common.check(
        "launch-hook-tui-ready",
        r["ready"],
        _line(common.squash(r["onboarding"])[-200:]),
    )
    if not r["ready"]:
        return
    onboarding = r["onboarding"].lower()
    shown = onboarding + " ".join(r["approvals"]).lower()
    hook_prompt = "hook" in shown
    common.check(
        "launch-hook-tui-no-hook-prompt",
        not hook_prompt,
        f"no dialog or approval named a hook; approvals: "
        f"{[_line(common.squash(a)[-120:]) for a in r['approvals']]}"
        if not hook_prompt
        else "a dialog named a hook: " + _line(r["onboarding"][-400:]),
    )
    trail = r["files"].get("trail.log", "")
    common.check(
        "launch-hook-tui-trail",
        f"{OTHER}/edited.txt" in trail,
        f"trail: {_line(trail)}; approvals: {len(r['approvals'])}",
    )
    user = r["files"].get("user-hook.log", "")
    common.check(
        "launch-hook-tui-coexists-with-user-hook",
        "user-hook Write" in user and bool(trail),
        f"user hook: {_line(user)}",
    )
