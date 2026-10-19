"""Is a context link inside `.worktrees/` a safe `--add-dir` for Antigravity?

`experiments/antigravity/workspace-order` found that agy sorts its
workspace paths after cleaning them (symlinks unresolved) and works in the
first. The proposed fix keeps the checkout first by handing agy a symlink
to UZE's context directory whose path sorts after the checkout:

- an agent in the slot `/work/project/.worktrees/t0lab` gets
  `--add-dir /work/project/.worktrees/t0lab~uze`;
- an agent in the primary `/work/project` gets
  `--add-dir /work/project/.worktrees/~uze/a1`.

The context directory (`/work/home/.uze/runtime/ctx`) holds a
`PreInvocation` logger in `.agents/hooks.json` and an `always_on` rule.
The primary and the slot carry different `AGENTS.md` and `.agents/rules`
markers, as two branches would. Plain `agy`, no UZE in the loop.

TUI worlds (`slot`, `primary`), each launch → `invoke_subagent` → end →
`agy -c` → second turn: where every marker lands in the parent's, the
subagent's and the resumed request, and the "Command Working Directory".
`slot-plain` is the slot with no `--add-dir`: what the slot loads on its
own, the control for any leak. `slot-gitfile` and `slot-gitdir` are the
slot world with the context directory ending the walk up from it: a `.git`
file naming no repository, or an empty `.git` directory.

Headless worlds measuring where agy's walk up from an added directory
stops (`B_*`): an added `/work/bN/sub/ctx` whose ancestor `/work/bN` holds
an `AGENTS.md` and a rule, with no Git (`b1`), inside a Git repository
(`b2`), the added directory itself a repository (`b3`), and holding a
`.git` file like a worktree (`b4`).

Run: python3 conformance/lab.py --harness antigravity --experiment antigravity/context-link
Env: CL_WORLDS=slot,primary,slot-plain,slot-gitfile,slot-gitdir,b1,b2,b3,b4
"""

import json
import os
import re
import shlex
import subprocess

from contract import continuity
from contract.tui import Tui
from experiments import agent_context_probe as probe
from experiments.launch_instructions_probe import _in_tool_call, placement
from harnesses.antigravity.bindings import AntigravityBindings
from harnesses.antigravity.scenarios import agy_setup
from shared import capture, common, vocabulary

PROJECT = "/work/project"
SLOT = f"{PROJECT}/.worktrees/t0lab"
CTX = "/work/home/.uze/runtime/ctx"
HOOK_LOG = "/work/hooklog"
SPAWN = "UZE_CL_SPAWN delegate this to a subagent"
SUB_TURN = "UZE_CL_SUBAGENT_TURN"
TURN_TWO = "UZE_CL_TURN_TWO"
HEADLESS = "UZE_CL_HEADLESS where do you work?"
MARKS = {
    "primary-agents-md": "UZE_CL_PRIMARY_AGENTS",
    "primary-rule": "UZE_CL_PRIMARY_RULE",
    "slot-agents-md": "UZE_CL_SLOT_AGENTS",
    "slot-rule": "UZE_CL_SLOT_RULE",
    "context-rule": "UZE_CL_CONTEXT_RULE",
    "ancestor-agents-md": "UZE_CL_ANCESTOR_AGENTS",
    "ancestor-rule": "UZE_CL_ANCESTOR_RULE",
}
#: TUI world: (process cwd, `--add-dir` or None, link to make: (target, path))
TUI_WORLDS = {
    "slot": (SLOT, f"{SLOT}~uze", (CTX, f"{SLOT}~uze")),
    "primary": (
        PROJECT,
        f"{PROJECT}/.worktrees/~uze/a1",
        (CTX, f"{PROJECT}/.worktrees/~uze/a1"),
    ),
    "slot-plain": (SLOT, None, None),
    "slot-gitfile": (SLOT, f"{SLOT}~uze", (CTX, f"{SLOT}~uze")),
    "slot-gitdir": (SLOT, f"{SLOT}~uze", (CTX, f"{SLOT}~uze")),
}
#: How the context directory ends the walk up from it (`b4` found that a
#: `.git` in the added directory stops it): a `.git` file naming no
#: repository, or an empty `.git` directory.
STOPS = {
    "slot-gitfile": f"printf 'gitdir: /nonexistent\\n' > {CTX}/.git",
    "slot-gitdir": f"mkdir -p {CTX}/.git",
}
#: Headless world: what the added directory `/work/bN/sub/ctx` sits in.
BOUNDARY = {
    "b1": "no Git anywhere",
    "b2": "/work/b2 is a Git repository",
    "b3": "/work/b2-like repository, and the added directory is one too",
    "b4": "/work/b2-like repository, and the added directory holds a .git file",
}
SELECTED = [
    w
    for w in os.environ.get("CL_WORLDS", ",".join([*TUI_WORLDS, *BOUNDARY])).split(",")
    if w
]


def _write(path, text):
    return (
        f"mkdir -p {shlex.quote(os.path.dirname(path))} && "
        f"printf '%s\\n' {shlex.quote(text)} > {shlex.quote(path)}"
    )


def _rule(path, mark):
    return (
        f"mkdir -p {shlex.quote(os.path.dirname(path))} && printf '%s\\n' '---' "
        f"'trigger: always_on' '---' '' {shlex.quote(f'Lab convention {mark}.')} "
        f"> {shlex.quote(path)}"
    )


def _context(root):
    hooks = {
        "uze-cl": {
            "PreInvocation": [
                {
                    "type": "command",
                    "command": f"mkdir -p {HOOK_LOG}; cat > {HOOK_LOG}/pre-$$.json; printf '{{}}'",
                    "timeout": 10,
                }
            ]
        }
    }
    return [
        _write(f"{root}/.agents/hooks.json", json.dumps(hooks)),
        _rule(f"{root}/.agents/rules/uze-instructions.md", MARKS["context-rule"]),
    ]


GIT = "git -c user.name=lab -c user.email=lab@uze.invalid"


def _tui_prelude(name):
    cwd, _, link = TUI_WORLDS[name]
    lines = [
        f"mkdir -p {PROJECT} {HOOK_LOG}",
        f"cd {PROJECT} && git init -q -b main .",
        "printf '/.worktrees/\\n' >> .git/info/exclude",
        _write(f"{PROJECT}/AGENTS.md", f"Lab house rule {MARKS['primary-agents-md']}."),
        _rule(f"{PROJECT}/.agents/rules/house.md", MARKS["primary-rule"]),
        f"cd {PROJECT} && {GIT} add -A && {GIT} commit -q -m primary",
        f"{GIT} worktree add -q -b agent/t0lab {SLOT}",
        _write(f"{SLOT}/AGENTS.md", f"Lab house rule {MARKS['slot-agents-md']}."),
        f"rm -f {SLOT}/.agents/rules/house.md",
        _rule(f"{SLOT}/.agents/rules/slot.md", MARKS["slot-rule"]),
        f"cd {SLOT} && {GIT} add -A && {GIT} commit -q -m slot",
        *_context(CTX),
    ]
    if name in STOPS:
        lines.append(STOPS[name])
    if link:
        lines.append(
            f"mkdir -p {os.path.dirname(link[1])} && ln -s {link[0]} {link[1]}"
        )
    lines.append(f"cd {cwd}")
    return "\n".join(lines)


def _spawn_call():
    row = vocabulary.load_expectation("antigravity")["aliases"]["agent.spawn"]
    call = json.loads(json.dumps(row["call"]).replace("lab subagent", SUB_TURN))
    return {"tool": vocabulary.call_tool(row), "args": call}


def tui_job(name):
    def run_world(world):
        bindings = AntigravityBindings()
        mode, env = bindings.sequence([_spawn_call()], SPAWN)
        prov_ip = common.start_provider(world, mode, dict(env or {}, DISCOVERY="1"))
        _, add, _ = TUI_WORLDS[name]
        flag = f" --add-dir {shlex.quote(add)}" if add else ""
        final = f"agy{flag}; printf '\\n{continuity.ENDED_MARKER}\\n'; agy -c{flag}"
        setup = agy_setup(
            world,
            prov_ip,
            include_mcp=False,
            final_cmd=final,
            prelude=_tui_prelude(name),
        )
        out = {"world": name}
        tui = Tui(world, common.docker_base(world, prov_ip, setup), f"cl-{name}")
        with tui:
            plain, ready = bindings.prepare(tui)
            out["ready"] = bool(ready)
            if ready:
                out["turn"] = bindings.hook_turn(tui, SPAWN).detail
                tui.collect(reads=4)
                plain, ended = continuity.end_the_process(tui, bindings)
                out["ended"] = bool(ended)
                if ended:
                    plain, matched = bindings.rejoin(
                        tui, plain.split(continuity.ENDED_MARKER, 1)[-1]
                    )
                    if matched:
                        bindings.await_input(tui)
                        tui.type(f"{TURN_TWO} and then?")
                        tui.submit()
                        for _ in range(14):
                            tui.collect(reads=1)
                            if TURN_TWO in probe.provider_log(world):
                                break
            out["hooks"] = common.harness_files(world, HOOK_LOG)
            out["log"] = probe.provider_log(world)
        return out

    return run_world


def _boundary_prelude(name):
    root = f"/work/{name}"
    added = f"{root}/sub/ctx"
    lines = [
        f"mkdir -p {added} {HOOK_LOG} /work/plain",
        _write(f"{root}/AGENTS.md", f"Lab house rule {MARKS['ancestor-agents-md']}."),
        _rule(f"{root}/.agents/rules/ancestor.md", MARKS["ancestor-rule"]),
        *_context(added),
        "cd /work/plain && git init -q -b main .",
    ]
    if name != "b1":
        lines.append(f"cd {root} && git init -q -b main .")
    if name == "b3":
        lines.append(f"cd {added} && git init -q -b main .")
    if name == "b4":
        lines.append(f"printf 'gitdir: {root}/.git\\n' > {added}/.git")
    lines.append("cd /work/plain")
    return "\n".join(lines), added


def boundary_job(name):
    def run_world(world):
        prov_ip = common.start_provider(world, "static", {"DISCOVERY": "1"})
        prelude, added = _boundary_prelude(name)
        final = f"""{prelude}
set +e
# decision: experiment-isolation
timeout 240 agy --add-dir {added} --print {shlex.quote(HEADLESS)} --dangerously-skip-permissions --print-timeout 120s 2>&1
"""
        setup = agy_setup(
            world, prov_ip, include_mcp=False, plugins="", final_cmd=final
        )
        proc = subprocess.run(
            common.docker_base(world, prov_ip, setup, tty=False),
            capture_output=True,
            text=True,
            errors="replace",
            timeout=480,
        )
        return {
            "world": name,
            "out": proc.stdout + proc.stderr,
            "log": probe.provider_log(world),
        }

    return run_world


def _calls(log):
    return [
        (i, b["request"] if isinstance(b.get("request"), dict) else b)
        for i, (_, b, _) in enumerate(probe.requests(log))
        if isinstance(b, dict) and capture.declared_tools(b)
    ]


def _where(body):
    text = json.dumps(body.get("systemInstruction", ""))
    cwd = re.search(r"Command Working Directory: ([^\\]+)", text)
    return {
        "cwd": cwd.group(1) if cwd else None,
        "markers": {
            name: sorted({h["path"].split(".parts")[0] for h in placement(body, mark)})
            for name, mark in MARKS.items()
            if placement(body, mark)
        },
    }


def judge_tui(cfg, name, out):
    calls = _calls(out.get("log", ""))
    roles = {
        "parent": next(
            (
                b
                for _, b in calls
                if SPAWN in json.dumps(b) and "functionCall" not in json.dumps(b)
            ),
            None,
        ),
        "subagent": next(
            (
                b
                for _, b in calls
                if any(not _in_tool_call(h["path"]) for h in placement(b, SUB_TURN))
            ),
            None,
        ),
        "resumed": next((b for _, b in calls if TURN_TWO in json.dumps(b)), None),
    }
    paths = sorted(
        {
            json.dumps(json.loads(t).get("workspacePaths"))
            for t in out.get("hooks", {}).values()
            if t.strip().startswith("{")
        }
    )
    result = {
        "world": name,
        "ready": out.get("ready"),
        "ended": out.get("ended"),
        "hook_workspacePaths": paths,
        **{role: (_where(b) if b else None) for role, b in roles.items()},
    }
    _save(cfg, name, out, result)
    cwd, _, _ = TUI_WORLDS[name]
    for role in roles:
        seen = result[role] or {}
        common.check(
            f"cl-{name}-{role}-works-in-checkout",
            seen.get("cwd") == cwd,
            f"Command Working Directory {seen.get('cwd')!r}",
        )
        if TUI_WORLDS[name][1]:
            common.check(
                f"cl-{name}-{role}-context-rule-in-system-instruction",
                "context-rule" in (seen.get("markers") or {}),
                f"markers {seen.get('markers')}",
            )
        if name != "primary":
            leaked = {"primary-agents-md", "primary-rule"} & set(
                seen.get("markers") or {}
            )
            common.check(
                f"cl-{name}-{role}-no-primary-leak",
                bool(seen) and not leaked,
                f"markers {seen.get('markers')}",
            )
    if TUI_WORLDS[name][1]:
        common.check(f"cl-{name}-hook-ran", bool(paths), f"workspacePaths {paths}")


def judge_boundary(cfg, name, out):
    calls = _calls(out.get("log", ""))
    body = next((b for _, b in calls if "UZE_CL_HEADLESS" in json.dumps(b)), None)
    result = {
        "world": name,
        "setting": BOUNDARY[name],
        **(_where(body) if body else {}),
    }
    _save(cfg, name, out, result)
    common.check(
        f"cl-{name}-added-dir-loaded",
        "context-rule" in result.get("markers", {}),
        f"markers {result.get('markers')}",
    )
    common.check(
        f"cl-{name}-ancestor-not-loaded",
        bool(body)
        and not {"ancestor-agents-md", "ancestor-rule"}
        & set(result.get("markers", {})),
        f"{BOUNDARY[name]}: markers {result.get('markers')}",
    )


def _save(cfg, name, out, result):
    with open(os.path.join(cfg.outdir, f"cl-{name}-raw-requests.log"), "w") as f:
        f.write(out.get("log", ""))
    with open(os.path.join(cfg.outdir, f"cl-{name}-result.json"), "w") as f:
        json.dump(result, f, indent=1)
    probe.say(f"[{name}] {json.dumps(result)[:1500]}")


def run(cfg, prov_ip):
    jobs = {
        name: (tui_job(name) if name in TUI_WORLDS else boundary_job(name))
        for name in SELECTED
    }
    results = common.concurrently(cfg, jobs)
    for name in SELECTED:
        (judge_tui if name in TUI_WORLDS else judge_boundary)(cfg, name, results[name])
