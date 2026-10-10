"""Can a launcher give one Codex process a file-trail hook, and nobody else?

UZE wants to know which files and repositories an agent it launched edits,
anywhere on the machine. The hook that tells it has to arrive with the
launch and leave with it: never written into the person's `~/.codex`, never
running for a Codex they start themselves. `CODEX_HOME` is not an answer —
it replaces the person's configuration instead of adding to it.

What Codex 0.161 offers (codex-rs `hooks/src/engine/discovery.rs`,
`hooks/src/config_rules.rs`, tag `rust-v0.161.0`):

- hooks are read from every config layer, inline `[hooks]` included, and
  `-c` is a layer of its own (`SessionFlags`), so
  `-c 'hooks.PostToolUse=[...]'` declares one for that process only;
- its trust key is `/<session-flags>/config.toml:<event>:<group>:<handler>`
  and its trust is read from `hooks.state` in the user layer *and* the
  session layer, so `-c 'hooks.state={"<key>"={trusted_hash="sha256:..."}}'`
  trusts exactly that handler for exactly that process. The hash is Codex's
  identity of the handler (`version_for_toml`), computed here as UZE's
  `codex/trust.rs` computes it;
- `--dangerously-bypass-hook-trust` is the blunt alternative: it trusts
  every hook of the invocation, the person's untrusted ones included.

The hook is a hand-written POSIX sh handler (the image has no Python or
jq): it keeps every payload it is handed as evidence, and appends each path
an `apply_patch` names (`*** Add File:`, `*** Update File:`,
`*** Delete File:`, `*** Move to:`) to the file `UZE_AGENT_TRAIL` names,
resolving a relative one against the payload's `cwd`. The person's own
hook, in `~/.codex/hooks.json` and trusted in their `config.toml` the way a
review records it, rides along in every phase.

Phases (headless unless named; each its own container and provider):

- `inject`: hook + session-layer trust; edits in `/work/other-repo` (a
  second Git repository outside the cwd `/work/project`), absolute,
  relative and in the cwd, a move and a delete; then a shell call running
  `git -C /work/other-repo status` with `GIT_TRACE2_EVENT=/work/trace`.
- `untrusted`: the same hook with no trust given: does exec run it?
- `bypass`: no trust state, `--dangerously-bypass-hook-trust`.
- `none`: no injection — the sanity control.
- `sandboxed`: `inject` under `workspace-write`: does the trace reach the
  disk? `sandboxed-root` adds `/work/trace` to its writable roots.
- `tui`: `inject` in the interactive TUI: which prompts appear?

Select with `LAUNCH_HOOK_PHASES=inject,none` (default: all).

Run:
  python3 conformance/lab.py --harness codex --experiment codex/launch-hook
"""

import json
import os
import shlex
import subprocess

from contract.tui import Tui
from harnesses.codex.bindings import CodexBindings
from harnesses.codex.scenarios import drive_onboarding
from shared import common

PROJECT = "/work/project"
OTHER = "/work/other-repo"
TRAIL = "/work/trail/trail.log"
TRACE = "/work/trace"
HOOK = "/work/uze/trail.sh"
SESSION_KEY = "/<session-flags>/config.toml:post_tool_use:0:0"
USER_HOOKS = "/work/home/.codex/hooks.json"
USER_KEY = f"{USER_HOOKS}:post_tool_use:0:0"
TRIGGER = "edit the other repository please"

#: The launcher's hook. A payload whose tool is not `apply_patch` is kept as
#: evidence and otherwise ignored.
HOOK_SCRIPT = r"""#!/bin/sh
payload=$(cat)
n=$(date +%s%N)-$$
mkdir -p /work/evidence
printf '%s' "$payload" > "/work/evidence/payload-$n.json"
printf 'UZE_AGENT_TRAIL=%s\n' "${UZE_AGENT_TRAIL-<unset>}" > "/work/evidence/env-$n.txt"
printf '%s' "$payload" | grep -q '"tool_name" *: *"apply_patch"' || exit 0
[ -n "$UZE_AGENT_TRAIL" ] || exit 0
cwd=$(printf '%s' "$payload" | sed -n 's/.*"cwd" *: *"\([^"]*\)".*/\1/p' | head -n 1)
printf '%s' "$payload" | sed 's/\\n/\n/g' \
  | sed -n 's/^.*\*\*\* \(Add File\|Update File\|Delete File\|Move to\): //p' \
  | while IFS= read -r path; do
      case "$path" in /*) ;; *) path="$cwd/$path" ;; esac
      printf '%s\n' "$path" >> "$UZE_AGENT_TRAIL"
    done
exit 0
"""

#: The person's own hook: it only records that it ran.
USER_HOOK_COMMAND = "mkdir -p /work/userhook && cat > /work/userhook/$(date +%s%N).json"

#: One `exec` cell: every kind of patch header, each call on its own so a
#: refused one does not hide the next, then the shell call git traces.
PATCHES = [
    f"*** Begin Patch\\n*** Add File: {OTHER}/added.txt\\n+lab\\n*** Update File: {OTHER}/README.md\\n@@\\n-lab other\\n+lab other, edited\\n*** End Patch\\n",
    "*** Begin Patch\\n*** Add File: ../other-repo/relative.txt\\n+rel\\n*** End Patch\\n",
    "*** Begin Patch\\n*** Add File: local.txt\\n+loc\\n*** End Patch\\n",
    f"*** Begin Patch\\n*** Update File: {OTHER}/added.txt\\n*** Move to: {OTHER}/moved.txt\\n@@\\n-lab\\n+lab moved\\n*** End Patch\\n",
    f"*** Begin Patch\\n*** Delete File: {OTHER}/relative.txt\\n*** End Patch\\n",
]
SHELL = f"git -C {OTHER} status --short; env | grep -E '^(UZE_AGENT_TRAIL|GIT_TRACE2_EVENT)=' || echo no-env"
SCRIPT = (
    "const out = [];\n"
    + "".join(
        f'try {{ out.push(JSON.stringify(await tools.apply_patch("{p}"))); }} catch (e) {{ out.push("ERR " + e); }}\n'
        for p in PATCHES
    )
    + f'try {{ out.push(JSON.stringify(await tools.exec_command({{cmd: {json.dumps(SHELL)}}}))); }} catch (e) {{ out.push("ERR " + e); }}\n'
    + 'text(out.join("\\n"));'
)

#: Every path the patches above name, as the trail must hold them.
EXPECTED = {
    f"{OTHER}/added.txt",
    f"{OTHER}/README.md",
    f"{PROJECT}/../other-repo/relative.txt",
    f"{PROJECT}/local.txt",
    f"{OTHER}/moved.txt",
    f"{OTHER}/relative.txt",
}


def identity(event, command, timeout, matcher=None):
    """Codex's trust hash of one command handler (`version_for_toml` over
    the normalized group), as UZE's `codex/trust.rs` computes it."""
    group = {
        "event_name": event,
        "hooks": [
            {"async": False, "command": command, "timeout": timeout, "type": "command"}
        ],
    }
    if matcher is not None:
        group["matcher"] = matcher
    canonical = json.dumps(
        group, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    )
    import hashlib

    return "sha256:" + hashlib.sha256(canonical.encode()).hexdigest()


def toml_string(value):
    return json.dumps(value)


def injection(trust=True):
    """The `-c` overrides a launcher adds: the hook, and its trust."""
    args = [
        "-c",
        f'hooks.PostToolUse=[{{hooks=[{{type="command",command={toml_string(HOOK)},timeout=10}}]}}]',
    ]
    if trust:
        digest = identity("post_tool_use", HOOK, 10)
        args += [
            "-c",
            f"hooks.state={{{toml_string(SESSION_KEY)}={{trusted_hash={toml_string(digest)}}}}}",
        ]
    return args


def setup(launch):
    user_hooks = json.dumps(
        {
            "hooks": {
                "PostToolUse": [
                    {
                        "hooks": [
                            {
                                "type": "command",
                                "command": USER_HOOK_COMMAND,
                                "timeout": 10,
                            }
                        ]
                    }
                ]
            }
        }
    )
    user_trust = identity("post_tool_use", USER_HOOK_COMMAND, 10)
    return f"""
set -e
export PATH=/usr/local/bin:/usr/bin:/bin:/usr/local/.local/bin
export HOME=/work/home CODEX_HOME=/work/home/.codex
export OPENAI_API_KEY=uze-conformance-invalid-by-design
export CODEX_CA_CERTIFICATES=/app/ca.crt SSL_CERT_FILE=/app/ca.crt
mkdir -p /work/home/.codex /work/uze /work/trail {TRACE} {PROJECT} {OTHER}
# decision: synthetic-credentials
cp /app/fixtures/auth.json /work/home/.codex/auth.json
# The person's own configuration: the Lab's sandbox decision, their own
# hook, and the trust their review of it recorded.
# decision: the container is the sandbox (as scenarios.codex_setup)
printf 'sandbox_mode = "danger-full-access"\\n\\n[hooks.state.%s]\\ntrusted_hash = %s\\n' \\
  {shlex.quote(toml_string(USER_KEY))} {shlex.quote(toml_string(user_trust))} > /work/home/.codex/config.toml
printf '%s' {shlex.quote(user_hooks)} > {USER_HOOKS}
cat > {HOOK} <<'UZE_HOOK_EOF'
{HOOK_SCRIPT}UZE_HOOK_EOF
chmod +x {HOOK}
git config --global user.email lab@example.invalid
git config --global user.name lab
git config --global init.defaultBranch main
git -C {PROJECT} init -q && printf 'lab project\\n' > {PROJECT}/README.md
git -C {OTHER} init -q && printf 'lab other\\n' > {OTHER}/README.md
git -C {OTHER} add -A && git -C {OTHER} commit -qm init
(cd /work/home/.codex && sha256sum config.toml hooks.json) > /work/codex-home.sum
ls -la /work/home/.codex > /work/codex-home.before
cd {PROJECT}
export UZE_AGENT_TRAIL={TRAIL} GIT_TRACE2_EVENT={TRACE}
set +e
{launch}
{DUMP}
"""


DUMP = f"""
echo '@@SECTION trail'; cat {TRAIL} 2>/dev/null
echo '@@SECTION payloads'; for f in /work/evidence/payload-*; do [ -f "$f" ] && {{ cat "$f"; echo; }}; done
echo '@@SECTION hook-env'; cat /work/evidence/env-* 2>/dev/null
echo '@@SECTION userhook'; ls /work/userhook 2>/dev/null | wc -l
echo '@@SECTION files'; ls -la {OTHER} {PROJECT}
echo '@@SECTION trace-files'; ls {TRACE} 2>/dev/null | wc -l
echo '@@SECTION trace-repos'; cat {TRACE}/* 2>/dev/null | grep -o '"event":"def_repo"[^}}]*"worktree":"[^"]*"' | sed 's/.*"worktree":"//; s/"$//' | sort | uniq -c
echo '@@SECTION trace-children'; cat {TRACE}/* 2>/dev/null | grep -o '"argv":\\[[^]]*\\]' | sort | uniq -c | head -40
echo '@@SECTION home-unchanged'; (cd /work/home/.codex && sha256sum -c /work/codex-home.sum) 2>&1
echo '@@SECTION home-listing'; ls -la /work/home/.codex
echo '@@SECTION config'; cat /work/home/.codex/config.toml
echo '@@SECTION end'
"""


def sections(output):
    found, name = {}, None
    for line in output.splitlines():
        if line.startswith("@@SECTION "):
            name = line.split(" ", 1)[1].strip()
            found[name] = []
        elif name:
            found[name].append(line)
    return {k: "\n".join(v).strip() for k, v in found.items()}


def container(cfg, prov_ip, script, tty):
    cmd = common.docker_base(cfg, prov_ip, script, tty=tty)
    ca_crt, _, _ = common.generate_certs(cfg)
    i = cmd.index(common.HARNESS_IMAGE)
    return cmd[:i] + ["-v", f"{ca_crt}:/app/ca.crt:ro"] + cmd[i:]


def provider(cfg):
    mode, env = CodexBindings().sequence(
        [{"tool": "functions.exec", "args": SCRIPT}], TRIGGER
    )
    return common.start_provider(cfg, mode, env)


def headless(cfg, phase, args):
    prov_ip = provider(cfg)
    launch = (
        "timeout 240 codex exec "
        + " ".join(shlex.quote(a) for a in args)
        + f' {shlex.quote(TRIGGER)} 2>&1; echo "@@EXIT $?"'
    )
    out = subprocess.run(
        container(cfg, prov_ip, setup(launch), tty=False),
        capture_output=True,
        text=True,
        errors="replace",
        timeout=420,
    ).stdout
    with open(os.path.join(cfg.outdir, f"launch-hook-{phase}.txt"), "w") as f:
        f.write(out)
    return out, sections(out)


def trail(found):
    return {line for line in found.get("trail", "").splitlines() if line.strip()}


def ran(out):
    """The scripted cell ran: the final text arrived and git answered."""
    return "UZE_CONFORMANCE_OK" in out and "README.md" in out


def phase_inject(cfg):
    out, found = headless(cfg, "inject", injection())
    common.check("inject-turn-ran", ran(out), out[-300:].replace("\n", " "))
    paths = trail(found)
    common.check(
        "inject-hook-fired",
        bool(found.get("payloads")),
        found.get("payloads", "")[:600].replace("\n", " "),
    )
    common.check(
        "inject-trail-holds-every-patched-path",
        EXPECTED <= paths,
        f"trail={sorted(paths)} missing={sorted(EXPECTED - paths)}",
    )
    common.check(
        "inject-hook-sees-UZE_AGENT_TRAIL",
        f"UZE_AGENT_TRAIL={TRAIL}" in found.get("hook-env", ""),
        found.get("hook-env", "")[:200],
    )
    common.check(
        "inject-patch-in-tool_input-command",
        '"tool_input":{"command":"*** Begin Patch' in found.get("payloads", ""),
        "apply_patch payload carries the patch as tool_input.command",
    )
    common.check(
        "inject-user-hook-coexists",
        int(found.get("userhook", "0") or 0) > 0,
        f"user hook runs: {found.get('userhook')}",
    )
    # `codex exec` records the cwd's trust on its own (`[projects."<cwd>"]`,
    # the `none` phase too); what the injection must not leave is any trace
    # of itself in the person's files.
    common.check(
        "inject-codex-home-holds-no-injection",
        "hooks.json: OK" in found.get("home-unchanged", "")
        and "session-flags" not in found.get("config", "")
        and HOOK not in found.get("config", ""),
        found.get("home-unchanged", "").replace("\n", "; "),
    )
    common.check(
        "inject-shell-env-keeps-both-vars",
        f"UZE_AGENT_TRAIL={TRAIL}" in out and f"GIT_TRACE2_EVENT={TRACE}" in out,
        "env seen by the shell tool's command",
    )
    common.check(
        "inject-trace2-records-other-repo",
        OTHER in found.get("trace-repos", ""),
        found.get("trace-repos", "").replace("\n", "; "),
    )


def phase_untrusted(cfg):
    out, found = headless(cfg, "untrusted", injection(trust=False))
    common.check("untrusted-turn-ran", ran(out), out[-300:].replace("\n", " "))
    common.check(
        "untrusted-hook-skipped-by-exec",
        not found.get("payloads") and not trail(found),
        f"payloads={bool(found.get('payloads'))} trail={sorted(trail(found))}",
    )


def phase_bypass(cfg):
    out, found = headless(
        cfg,
        "bypass",
        # decision: experiment-isolation
        injection(trust=False) + ["--dangerously-bypass-hook-trust"],
    )
    common.check("bypass-turn-ran", ran(out), out[-300:].replace("\n", " "))
    common.check(
        "bypass-hook-fired",
        EXPECTED <= trail(found),
        f"trail={sorted(trail(found))}",
    )


def phase_none(cfg):
    out, found = headless(cfg, "none", [])
    common.check("none-turn-ran", ran(out), out[-300:].replace("\n", " "))
    common.check(
        "none-nothing-extra-runs",
        not found.get("payloads") and not trail(found),
        f"payloads={bool(found.get('payloads'))} trail={sorted(trail(found))}",
    )
    common.check(
        "none-user-hook-still-runs",
        int(found.get("userhook", "0") or 0) > 0,
        f"user hook runs: {found.get('userhook')}",
    )


def phase_sandboxed(cfg, writable_trace):
    """`workspace-write` writes only the cwd, /tmp and $TMPDIR: a trace
    directory elsewhere is out of reach for a sandboxed git unless the
    launcher adds it to the sandbox's writable roots, another `-c`."""
    phase = "sandboxed-root" if writable_trace else "sandboxed"
    args = injection() + ["-c", 'sandbox_mode="workspace-write"']
    if writable_trace:
        args += ["-c", f'sandbox_workspace_write.writable_roots=["{TRACE}"]']
    out, found = headless(cfg, phase, args)
    common.check(
        f"{phase}-final-text",
        "UZE_CONFORMANCE_OK" in out,
        out[-300:].replace("\n", " "),
    )
    common.check(
        f"{phase}-shell-env-keeps-both-vars",
        f"GIT_TRACE2_EVENT={TRACE}" in out and f"UZE_AGENT_TRAIL={TRAIL}" in out,
        "env seen by the sandboxed shell tool's command",
    )
    traced = OTHER in found.get("trace-repos", "")
    common.check(
        f"{phase}-trace2-records-other-repo-{'yes' if writable_trace else 'no'}",
        traced == writable_trace,
        f"other-repo traced={traced}: "
        + found.get("trace-repos", "").replace("\n", "; "),
    )
    # An edit outside the workspace is refused before it runs, and a
    # refused patch fires no PostToolUse: the trail holds only the cwd's.
    common.check(
        f"{phase}-trail-only-the-cwd-edit",
        trail(found) == {f"{PROJECT}/local.txt"},
        f"trail={sorted(trail(found))}",
    )


def phase_tui(cfg):
    bindings = CodexBindings()
    prov_ip = provider(cfg)
    launch = "codex " + " ".join(shlex.quote(a) for a in injection())
    script = setup(launch + "; echo UZE_LAB_TUI_DONE")
    with Tui(
        cfg, container(cfg, prov_ip, script, tty=True), "codex-launch-hook"
    ) as tui:
        _, shown = drive_onboarding(tui.child)
        tui.snapshot("launch-hook-ready", shown)
        turn = bindings.hook_turn(tui, TRIGGER)
        tui.snapshot("launch-hook-turn", turn.plain)
        # The footer can carry "⚠ 1 warning · f2 to view": open it, so the
        # evidence says whether the injection is what it warns about.
        tui.child.send("\x1bOQ")
        warnings = tui.collect(reads=4)
        tui.snapshot("launch-hook-warnings", warnings)
        squashed = warnings.replace(" ", "").lower()
        # Measured on 0.161: any `-c` keeps the TUI off the shared
        # app-server daemon ("requires embedded mode"); nothing about hooks.
        common.check(
            "tui-injection-runs-embedded-not-on-daemon",
            "requiresembeddedmode" in squashed,
            warnings[-300:].replace("\n", " "),
        )
        common.check(
            "tui-warning-not-about-hooks",
            "hook" not in squashed and "session-flags" not in squashed,
            "the one startup warning names no hook",
        )
        transcript = tui.transcript()
        review = "Trustallandcontinue" in transcript.replace(" ", "")
        common.check(
            "tui-no-hook-review",
            not review,
            "the hook review appeared" if review else "no hook review on screen",
        )
        common.check(
            "tui-turn-settled",
            turn.settled,
            f"{turn.detail}; approvals={[a[-120:] for a in turn.approvals]}",
        )
        dump = subprocess.run(
            ["docker", "exec", cfg.harness_container, "sh", "-c", DUMP],
            capture_output=True,
            text=True,
            errors="replace",
        ).stdout
        with open(os.path.join(cfg.outdir, "launch-hook-tui.txt"), "w") as f:
            f.write(dump)
        found = sections(dump)
        common.check(
            "tui-trail-holds-every-patched-path",
            EXPECTED <= trail(found),
            f"trail={sorted(trail(found))}",
        )
        common.check(
            "tui-user-hook-coexists",
            int(found.get("userhook", "0") or 0) > 0,
            f"user hook runs: {found.get('userhook')}",
        )
        common.check(
            "tui-codex-home-holds-no-injection",
            "hooks.json: OK" in found.get("home-unchanged", "")
            and "session-flags" not in found.get("config", "")
            and HOOK not in found.get("config", ""),
            found.get("config", "").replace("\n", "; "),
        )
        common.check(
            "tui-trace2-records-other-repo",
            OTHER in found.get("trace-repos", ""),
            found.get("trace-repos", "").replace("\n", "; "),
        )


PHASES = {
    "inject": phase_inject,
    "untrusted": phase_untrusted,
    "bypass": phase_bypass,
    "none": phase_none,
    "sandboxed": lambda cfg: phase_sandboxed(cfg, False),
    "sandboxed-root": lambda cfg: phase_sandboxed(cfg, True),
    "tui": phase_tui,
}


def run(cfg, prov_ip):
    chosen = os.environ.get("LAUNCH_HOOK_PHASES", ",".join(PHASES)).split(",")
    for name in chosen:
        with common.describe(name):
            PHASES[name](cfg)
