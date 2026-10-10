"""Can a launch hand OpenCode an observer of the files its agent edits?

The workspace wants to know which files and repositories an agent edits
anywhere on the machine. The plan is an observer injected at launch,
scoped to that one harness process and never written into the user's own
configuration: for OpenCode, a plugin whose `execute.after` tool hook
appends each written/edited path to the file named by `UZE_AGENT_TRAIL`,
plus `GIT_TRACE2_EVENT=<dir>` so every git the agent runs leaves a trace
naming its repository. This measures, rather than assumes:

1. injection: a plugin named only by the launch's environment
   (`OPENCODE_CONFIG_CONTENT`, `OPENCODE_CONFIG`, `OPENCODE_CONFIG_DIR`)
   loads and its tool hook fires;
2. coexistence: the user's own plugins (one auto-discovered in
   `~/.config/opencode/plugins/`, one listed in the global `plugins`
   array) still load next to it;
3. path: what the hook is handed for a write/edit into another Git
   repository outside the working directory (absolute and relative), that
   the plugin reads `UZE_AGENT_TRAIL` and appends — and whether the shared
   background service (a default launch, which is how UZE launches
   OpenCode) keeps the observation per launch;
4. prompts: whether the injection adds any (headless, both modes);
5. sanity: with no injection nothing extra loads;
6. trace2: a scripted `git -C /work/other-repo status` leaves a trace file
   whose `def_repo` names that worktree.

Observed on opencode 2.0.24 (2026-10-10), 29/31 checks, the two red ones
being the service's scoping, measured:

- `OPENCODE_CONFIG_CONTENT='{"plugins":["<dir>"]}'` and
  `OPENCODE_CONFIG=<file>` are additive: the `plugins` arrays merge, and
  the user's auto-discovered and listed plugins load beside the injected
  one. A configured entry must be a package *directory* (a `.ts`/`.js`
  file or `file://` URL is refused: "configured plugin path must be a
  directory"). `OPENCODE_CONFIG_DIR` replaces ~/.config/opencode (the
  user's plugins and provider are gone).
- `execute.after` hands `{tool, sessionID, agent, messageID, id, input,
  status, result}`; write/edit carry `input.path` verbatim as the model
  wrote it (absolute stays absolute, `../other-repo/x` stays relative —
  no resolved path, no cwd in the event), and the plugin reads the
  launch's `UZE_AGENT_TRAIL` from `process.env`.
- No prompt comes from the injection, headless or in the TUI; the one
  prompt is the harness's own "Access external directory" for a write
  outside the working directory.
- `--standalone` holds it all per launch. The background service a
  default launch attaches to does not: plugins and their environment are
  the service's, from the launch that started it, so every later launch
  (injected with its own trail, or not injected at all) is appended to the
  first launch's trail, and a service started without the injection never
  loads it. `GIT_TRACE2_EVENT` does follow each launch: the shell tool's
  git wrote into the second launch's own trace directory, with
  `def_repo.worktree` naming /work/other-repo (OpenCode's own snapshot
  gits add ~40 traces naming the project per turn).

Run:
  python3 conformance/lab.py --harness opencode --experiment opencode/launch-hook
"""

import json
import re
import subprocess
import time

import pexpect

from harnesses.opencode.scenarios import make_screen, make_waiter, opencode_setup
from shared import common
from shared.common import check

TRIGGER = "UZE_EXP_LH"
BEGIN, END = "=== LAUNCH-HOOK ===", "=== LAUNCH-HOOK-END ==="

SEQUENCE = [
    {
        "name": "write",
        "args": json.dumps(
            {"path": "/work/other-repo/abs-write.txt", "content": "UZE_EXP_LH_ABS"}
        ),
    },
    {
        "name": "edit",
        "args": json.dumps(
            {
                "path": "/work/other-repo/README.md",
                "oldString": "lab other",
                "newString": "lab other, edited",
            }
        ),
    },
    {
        "name": "write",
        "args": json.dumps(
            {"path": "../other-repo/rel-write.txt", "content": "UZE_EXP_LH_REL"}
        ),
    },
    {
        "name": "shell",
        "args": json.dumps({"command": "git -C /work/other-repo status"}),
    },
]

# One template for every probe; only its id differs. No import of
# `@opencode-ai/plugin` (2.0.18 refused one from the plugin directory).
# Every probe records what it saw in one shared log; only `uze-trail`
# appends to the trail, and only from the environment of its process.
PLUGIN = r"""import { appendFileSync } from "node:fs";
const ID = "__ID__";
const DEBUG = "/work/plugin-debug.jsonl";
const log = (o) => { try { appendFileSync(DEBUG, JSON.stringify({ id: ID, pid: process.pid, phase: process.env.UZE_EXP_PHASE ?? null, ...o }) + "\n"); } catch (e) {} };
export default {
  id: ID,
  async setup(ctx) {
    log({ kind: "setup", trail: process.env.UZE_AGENT_TRAIL ?? null, trace: process.env.GIT_TRACE2_EVENT ?? null, cwd: process.cwd() });
    await ctx.tool.hook("execute.after", async (event) => {
      const input = event.input ?? {};
      const path = input.path ?? input.filePath ?? null;
      log({ kind: "after", tool: event.tool, keys: Object.keys(event), input, status: event.status ?? null, trail: process.env.UZE_AGENT_TRAIL ?? null });
      const trail = process.env.UZE_AGENT_TRAIL;
      if (ID === "uze-trail" && trail && (event.tool === "write" || event.tool === "edit") && path) {
        appendFileSync(trail, path + "\n");
      }
    });
  },
};
"""


def plugin_file(path, plugin_id):
    return f"cat > {path} <<'PLUGIN_EOF'\n{PLUGIN.replace('__ID__', plugin_id)}PLUGIN_EOF\n"


def plugin_package(directory, plugin_id):
    """A configured plugin entry: 2.0.24 refuses a file ("configured plugin
    path must be a directory", measured) and loads a package directory."""
    return (
        f"mkdir -p {directory}\n"
        f'printf \'{{"name":"{plugin_id}","version":"1.0.0","main":"index.ts"}}\' '
        f"> {directory}/package.json\n"
        + plugin_file(f"{directory}/index.ts", plugin_id)
    )


def fresh_other_repo():
    return (
        "rm -rf /work/other-repo && mkdir -p /work/other-repo && "
        "(cd /work/other-repo && git init -q . && printf 'lab other\\n' > README.md "
        "&& git add . && git -c user.email=l@b -c user.name=lab commit -qm init)"
    )


def phase(name, env, flags, *, prompt_tail="go"):
    """One headless launch from /work/project, with its outcome dumped."""
    return f"""
echo '@@@ phase {name}'
{fresh_other_repo()}
mkdir -p /work/trace-{name}
cd /work/project && env UZE_EXP_PHASE={name} {env} timeout 150 opencode run {flags} "{TRIGGER} {name} {prompt_tail}" </dev/null >/work/out-{name}.txt 2>&1
rc=$?
echo '>> exit'; echo $rc
echo '>> out'; tail -c 1500 /work/out-{name}.txt
echo '>> files'; ls /work/other-repo; cat /work/other-repo/README.md
echo '>> trail'; cat /work/trail-{name}.log 2>/dev/null
echo '>> trace'; ls /work/trace-{name} | wc -l; grep -l '"worktree":"/work/other-repo"' /work/trace-{name}/* 2>/dev/null | wc -l; grep -ho '"worktree":"[^"]*"' /work/trace-{name}/* 2>/dev/null | sort | uniq -c
"""


CONTENT = '{"plugins":["/work/uze-inject/trail"]}'

SCRIPT = (
    "set +e\n"
    "mkdir -p /work/project /work/uze-inject/dir/plugins /work/home/.config/opencode/plugins\n"
    "(cd /work/project && git init -q . && printf 'lab project\\n' > README.md)\n"
    + plugin_file("/work/home/.config/opencode/plugins/user-dir.ts", "user-dir")
    + plugin_package("/work/home/user-listed", "user-listed")
    + plugin_package("/work/uze-inject/trail", "uze-trail")
    + plugin_file("/work/uze-inject/dir/plugins/trail.ts", "uze-trail")
    + "printf '%s' '"
    + CONTENT
    + "' > /work/uze-inject/opencode.json\n"
    # The user's own `plugins` array: whether an injected array merges with
    # it or replaces it is half the coexistence question.
    + 'node -e \'const fs=require("fs");const p="/work/home/.config/opencode/opencode.json";'
    'const d=JSON.parse(fs.readFileSync(p,"utf8"));d.plugins=["/work/home/user-listed"];'
    "fs.writeFileSync(p,JSON.stringify(d,null,1));'\n"
    "cp /work/home/.config/opencode/opencode.json /work/user-config.before\n"
    f"echo '{BEGIN}'\n"
    + phase("sanity", "UZE_AGENT_TRAIL=/work/trail-sanity.log", "--standalone --auto")
    + phase(
        "content",
        f"OPENCODE_CONFIG_CONTENT='{CONTENT}' UZE_AGENT_TRAIL=/work/trail-content.log "
        "GIT_TRACE2_EVENT=/work/trace-content",
        "--standalone --auto",
    )
    + phase(
        "file",
        "OPENCODE_CONFIG=/work/uze-inject/opencode.json UZE_AGENT_TRAIL=/work/trail-file.log",
        "--standalone --auto",
    )
    + phase(
        "dir",
        "OPENCODE_CONFIG_DIR=/work/uze-inject/dir UZE_AGENT_TRAIL=/work/trail-dir.log",
        "--standalone --auto",
    )
    # Prompts: the same launch with and without the injection, neither
    # auto-approving. Whatever differs between the two is the injection's.
    + phase("plain-ask", "UZE_AGENT_TRAIL=/work/trail-plain-ask.log", "--standalone")
    + phase(
        "inject-ask",
        f"OPENCODE_CONFIG_CONTENT='{CONTENT}' UZE_AGENT_TRAIL=/work/trail-inject-ask.log",
        "--standalone",
    )
    # The background service a default launch attaches to (how UZE
    # launches OpenCode): `svc-a` starts it injected; `svc-b` is a second
    # injected launch with its own trail; `svc-c` a launch with no
    # injection at all, as a person's own would be.
    + "opencode service stop >/dev/null 2>&1\n"
    + phase(
        "svc-a",
        f"OPENCODE_CONFIG_CONTENT='{CONTENT}' UZE_AGENT_TRAIL=/work/trail-svc-a.log "
        "GIT_TRACE2_EVENT=/work/trace-svc-a",
        "--auto",
    )
    + phase(
        "svc-b",
        f"OPENCODE_CONFIG_CONTENT='{CONTENT}' UZE_AGENT_TRAIL=/work/trail-svc-b.log "
        "GIT_TRACE2_EVENT=/work/trace-svc-b",
        "--auto",
    )
    + phase("svc-c", "UZE_AGENT_TRAIL=/work/trail-svc-c.log", "--auto")
    + "opencode service stop >/dev/null 2>&1\n"
    + "echo '@@@ svc-a-final'\ncat /work/trail-svc-a.log\n"
    + "echo '@@@ svc-a-trace-final'\n"
    'grep -ho \'"worktree":"[^"]*"\' /work/trace-svc-a/* | sort | uniq -c\n'
    # And the other order: a person's own launch started the service, then
    # UZE launches injected into it.
    + phase("svc-plain", "UZE_AGENT_TRAIL=/work/trail-svc-plain.log", "--auto")
    + phase(
        "svc-late",
        f"OPENCODE_CONFIG_CONTENT='{CONTENT}' UZE_AGENT_TRAIL=/work/trail-svc-late.log",
        "--auto",
    )
    + "opencode service stop >/dev/null 2>&1\n"
    + "echo '@@@ user-config'\n"
    "cmp /work/user-config.before /work/home/.config/opencode/opencode.json && echo unchanged\n"
    "ls /work/home/.config/opencode /work/home/.config/opencode/plugins\n"
    "echo '@@@ debug'\ncat /work/plugin-debug.jsonl\n"
    f"echo '{END}'\n"
)


TUI_SETUP = (
    "mkdir -p /work/project /work/home/.config/opencode/plugins\n"
    "(cd /work/project && git init -q . && printf 'lab project\\n' > README.md)\n"
    + fresh_other_repo()
    + "\n"
    + plugin_file("/work/home/.config/opencode/plugins/user-dir.ts", "user-dir")
    + plugin_package("/work/uze-inject/trail", "uze-trail")
    + f"cd /work/project && exec env UZE_EXP_PHASE=tui OPENCODE_CONFIG_CONTENT='{CONTENT}' "
    "UZE_AGENT_TRAIL=/work/trail-tui.log opencode --standalone\n"
)


def tui_phase(cfg, prov_ip):
    """The injected launch in the TUI, nothing auto-approved: what does a
    person meet that the injection put there?"""
    cmd = common.docker_base(
        cfg, prov_ip, opencode_setup(cfg, prov_ip, TUI_SETUP, plugins="flow")
    )
    container = cfg.harness_container
    child = pexpect.spawn(
        cmd[0], cmd[1:], encoding="utf-8", codec_errors="replace", timeout=300
    )
    child.setwinsize(50, 160)
    try:
        child.logfile_read = common.CastRecorder(cfg.outdir, "tui-launch-hook")
    except Exception:
        pass
    screen = make_screen(child)
    wait_for = make_waiter(screen)
    boot, tail, marker = wait_for(["Ask anything"], tries=16, stop_on_death=True)
    with open(f"{cfg.outdir}/tui_boot.raw", "w") as f:
        f.write(boot)
    check(
        "tui-injected-boots-to-prompt",
        marker == "Ask anything",
        "reached 'Ask anything' with the injection in the environment; boot screen: "
        + boot[-300:].replace("\n", " "),
    )
    # Plugin loading finishes after the prompt renders (the permission
    # experiment's warmup), or typed input is lost.
    time.sleep(25)
    for character in f"{TRIGGER} tui go":
        child.send(character)
        time.sleep(0.04)
    time.sleep(1)
    child.send("\r")
    turn, tail, marker = wait_for(
        ["UZE_CONFORMANCE_PASS", "Permission", "permission", "Allow", "external"],
        tries=24,
        gap=2.5,
        accumulate=True,
    )
    first_prompt = common.render_screen(turn)
    with open(f"{cfg.outdir}/tui_turn.raw", "w") as f:
        f.write(turn)
    # Each external-directory prompt is answered as a person would, with
    # the default "Allow once", so the turn reaches its end.
    answered = 0
    while marker != "UZE_CONFORMANCE_PASS" and answered < 6:
        child.send("\r")
        answered += 1
        more, tail, marker = wait_for(
            ["UZE_CONFORMANCE_PASS", "Permission required"],
            tries=12,
            gap=2.5,
            accumulate=True,
        )
        turn += more
    with open(f"{cfg.outdir}/tui_turn_answered.raw", "w") as f:
        f.write(turn)
    debug = subprocess.run(
        [
            "docker",
            "exec",
            container,
            "sh",
            "-c",
            "cat /work/plugin-debug.jsonl; echo '@@@ trail'; cat /work/trail-tui.log",
        ],
        capture_output=True,
        text=True,
        errors="replace",
    ).stdout
    with open(f"{cfg.outdir}/tui_debug.out", "w") as f:
        f.write(debug)
    loaded = '"id":"uze-trail","pid"' in debug and '"kind":"setup"' in debug
    check(
        "tui-injected-plugin-loads",
        loaded and '"id":"user-dir"' in debug,
        debug[:400].replace("\n", " "),
    )
    prompt_lines = [
        line.strip(" ┃")
        for line in first_prompt.splitlines()
        if "Permission" in line or "external" in line
    ]
    check(
        "tui-only-prompt-is-the-external-directory-one",
        any(
            "Access external directory /work/other-repo" in line
            for line in prompt_lines
        )
        and "plugin" not in first_prompt.lower(),
        f"prompt lines: {prompt_lines}",
    )
    trail_lines = [
        line for line in debug.split("@@@ trail", 1)[-1].splitlines() if line.strip()
    ]
    check(
        "tui-trail-appended",
        "/work/other-repo/abs-write.txt" in trail_lines,
        f"{answered} prompt(s) answered 'Allow once', turn ended={marker == 'UZE_CONFORMANCE_PASS'}; "
        f"trail-tui.log: {trail_lines}",
    )
    child.send("\x03")
    time.sleep(0.6)
    child.close(force=True)


def sections(text):
    """`@@@ name` headed sections of the dump."""
    found, name = {}, None
    for line in text.splitlines():
        if line.startswith("@@@ "):
            name = line[4:].strip().removeprefix("phase ")
            found[name] = []
        elif name:
            found[name].append(line)
    return {k: "\n".join(v) for k, v in found.items()}


def part(section, label):
    """The `>> label` block of one phase section."""
    m = re.search(rf"^>> {label}\n(.*?)(?=^>> |\Z)", section, re.S | re.M)
    return m.group(1) if m else ""


def run(cfg, prov_ip):
    prov_ip = common.start_provider(
        cfg,
        "toolcall",
        {"TOOL_SEQUENCE": json.dumps(SEQUENCE), "TOOL_TRIGGER": TRIGGER},
    )
    time.sleep(1)
    cmd = common.docker_base(
        cfg, prov_ip, opencode_setup(cfg, prov_ip, SCRIPT, plugins="flow"), tty=False
    )
    proc = subprocess.run(
        cmd, capture_output=True, text=True, errors="replace", timeout=1800
    )
    text = proc.stdout + proc.stderr
    with open(f"{cfg.outdir}/launch_hook.out", "w") as f:
        f.write(text)
    text = text[text.find(BEGIN) :] if BEGIN in text else text
    parts = sections(text)

    events = []
    for line in parts.get("debug", "").splitlines():
        try:
            events.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    with open(f"{cfg.outdir}/plugin_events.json", "w") as f:
        json.dump(events, f, indent=1)

    def setups(ph, plugin):
        return [
            e
            for e in events
            if e["kind"] == "setup" and e["id"] == plugin and e["phase"] == ph
        ]

    def afters(ph, plugin):
        return [
            e
            for e in events
            if e["kind"] == "after" and e["id"] == plugin and e["phase"] == ph
        ]

    def trail(ph):
        return [
            line
            for line in part(parts.get(ph, ""), "trail").splitlines()
            if line.strip()
        ]

    def tools_ran(ph):
        files = part(parts.get(ph, ""), "files")
        return "abs-write.txt" in files and "lab other, edited" in files

    def summary(ph):
        return (
            f"setups={[e['id'] for e in events if e['kind'] == 'setup' and e['phase'] == ph]} "
            f"afters={[(e['id'], e['tool']) for e in events if e['kind'] == 'after' and e['phase'] == ph]} "
            f"trail={trail(ph)} tools_ran={tools_ran(ph)}"
        )

    # 5. sanity
    check(
        "sanity-tools-ran",
        tools_ran("sanity"),
        f"the scripted write/edit landed in /work/other-repo: {summary('sanity')}",
    )
    check(
        "sanity-no-trail-plugin-without-injection",
        tools_ran("sanity")
        and not setups("sanity", "uze-trail")
        and not trail("sanity"),
        summary("sanity"),
    )
    check(
        "sanity-user-plugins-load",
        bool(afters("sanity", "user-dir")) and bool(afters("sanity", "user-listed")),
        summary("sanity"),
    )

    # 1 + 2 + 3 per injection route
    for route in ("content", "file"):
        loaded = bool(setups(route, "uze-trail"))
        fired = bool(afters(route, "uze-trail"))
        check(f"{route}-trail-plugin-loads", loaded, summary(route))
        check(f"{route}-trail-hook-fires", fired, summary(route))
        check(
            f"{route}-user-dir-plugin-coexists",
            bool(afters(route, "user-dir")),
            summary(route),
        )
        check(
            f"{route}-user-listed-plugin-coexists",
            bool(afters(route, "user-listed")),
            summary(route),
        )
        check(
            f"{route}-trail-appended",
            "/work/other-repo/abs-write.txt" in trail(route)
            and "/work/other-repo/README.md" in trail(route),
            f"trail-{route}.log: {trail(route)}",
        )

    # OPENCODE_CONFIG_DIR is not an extra layer: it stands in for
    # ~/.config/opencode, so the user's plugins and provider go with it.
    model_lines = re.findall(r"> build · [^\n]*", part(parts.get("dir", ""), "out"))
    check(
        "dir-loads-the-injected-plugin",
        bool(setups("dir", "uze-trail")),
        summary("dir"),
    )
    check(
        "dir-replaces-the-user-config",
        not setups("dir", "user-dir")
        and not setups("dir", "user-listed")
        and "uze-model" not in part(parts.get("dir", ""), "out"),
        f"{summary('dir')}; model line: {model_lines[:1]}",
    )

    after = afters("content", "uze-trail")
    write_inputs = [e["input"] for e in after if e["tool"] in ("write", "edit")]
    check(
        "path-arg-is-path-as-given",
        any(i.get("path") == "../other-repo/rel-write.txt" for i in write_inputs),
        f"event keys {after[0]['keys'] if after else None}; write/edit inputs: "
        f"{json.dumps(write_inputs)[:600]}",
    )
    check(
        "plugin-sees-launch-env",
        any(
            e.get("trail") == "/work/trail-content.log"
            for e in setups("content", "uze-trail")
        ),
        f"setup env: {json.dumps(setups('content', 'uze-trail'))[:300]}",
    )

    # 3. the shared background service
    svc = {ph: trail(ph) for ph in ("svc-a", "svc-b", "svc-c")}
    # Events carry the environment of the process that ran the hook, so
    # in the service every launch's events read as the one that started it.
    svc_a_final = [
        line for line in parts.get("svc-a-final", "").splitlines() if line.strip()
    ]
    check(
        "service-first-launch-observed",
        "/work/other-repo/abs-write.txt" in svc["svc-a"],
        f"trails: {svc}; {summary('svc-a')}",
    )
    check(
        "service-second-launch-gets-its-own-trail",
        "/work/other-repo/abs-write.txt" in svc["svc-b"],
        f"trails: {svc}; svc-b {summary('svc-b')}",
    )
    check(
        "service-started-uninjected-ignores-later-injection",
        tools_ran("svc-late")
        and not trail("svc-late")
        and not any(
            e["id"] == "uze-trail"
            for e in events
            if e["phase"] in ("svc-plain", "svc-late")
        ),
        f"svc-late {summary('svc-late')}; svc-plain {summary('svc-plain')}",
    )
    check(
        "service-shell-git-runs-in-the-launch-env",
        "/work/other-repo" in part(parts.get("svc-b", ""), "trace"),
        f"svc-b's own trace dir: {part(parts.get('svc-b', ''), 'trace').strip()!r}; "
        f"svc-a's once stopped: {parts.get('svc-a-trace-final', '').strip()!r}",
    )
    check(
        "service-uninjected-launch-unobserved",
        tools_ran("svc-c") and svc_a_final.count("/work/other-repo/abs-write.txt") <= 1,
        f"svc-c {summary('svc-c')}; trail-svc-a once the service stopped: {svc_a_final}",
    )

    # 4. prompts
    check(
        "injection-adds-no-prompt-headless",
        tools_ran("plain-ask") == tools_ran("inject-ask")
        and part(parts.get("plain-ask", ""), "exit")
        == part(parts.get("inject-ask", ""), "exit"),
        f"plain: tools_ran={tools_ran('plain-ask')} "
        f"{part(parts.get('plain-ask', ''), 'out')[-300:]!r} | inject: "
        f"tools_ran={tools_ran('inject-ask')} {part(parts.get('inject-ask', ''), 'out')[-300:]!r}",
    )

    # 6. trace2
    for ph in ("content", "svc-a", "svc-b"):
        trace = part(parts.get(ph, ""), "trace")
        lines = trace.splitlines()
        hits = (
            int(lines[1].strip())
            if len(lines) > 1 and lines[1].strip().isdigit()
            else 0
        )
        check(
            f"trace2-{ph}-names-other-repo",
            hits > 0,
            f"trace dir: {' / '.join(line.strip() for line in lines)}",
        )

    tui_phase(cfg, prov_ip)

    check(
        "user-config-untouched",
        "unchanged" in parts.get("user-config", ""),
        parts.get("user-config", "")[:300],
    )
