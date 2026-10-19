"""Which directory Antigravity takes as the working directory, and why.

`experiments/antigravity/system-instruction` found that Antigravity 1.4.2
lists the session's directories under "Active Workspaces" in path order,
not in the order they were given, and takes the first as "Command Working
Directory". UZE's launch adds `~/.uze/runtime/.../agents/<id>` with
`--add-dir`, which sorts before most checkouts, so agy works in UZE's
runtime directory instead of the slot. This measures the rule with plain
`agy`, no UZE in the loop, one headless `agy --print` world each:

- `before` / `after`: `--add-dir /work/launch` (sorts before the project)
  and `/work/zlaunch` (sorts after): the controls;
- `dotdot`: `--add-dir /work/project/../a-ctx`, a string that sorts after
  the project and resolves to a directory that sorts before it;
- `symlink`: `--add-dir /work/zz-link`, a link sorting after the project
  that points at `/work/a-ctx`;
- `project-first` / `project-last`: the project given explicitly too,
  before and after the context directory;
- `cwd-ctx`: launched from the context directory with
  `--add-dir /work/project`;
- `cwd-zctx`: launched from `/work/zlaunch` with `--add-dir /work/project`.

Each context directory holds a `PreInvocation` hook logging what it is
handed and an `always_on` rule with a marker, so each world also says
whether the added directory's customizations still load through that
form. The judge reads "Active Workspaces" and "Command Working Directory"
from the model request.

Run: python3 conformance/lab.py --harness antigravity --experiment antigravity/workspace-order
Env: WO_WORLDS=before,after,...
"""

import json
import os
import re
import shlex
import subprocess

from experiments import agent_context_probe as probe
from harnesses.antigravity.scenarios import agy_setup
from shared import capture, common

PROJECT = "/work/project"
HOOK_LOG = "/work/hooklog"
RULE_MARK = "UZE_WO_CTX_RULE"
PROMPT = "UZE_WO_TURN where do you work?"

#: world: (directory the process starts in, `--add-dir` arguments, the
#: context directory the customizations are written to, a link to make)
WORLDS = {
    "before": (PROJECT, ["/work/launch"], "/work/launch", None),
    "after": (PROJECT, ["/work/zlaunch"], "/work/zlaunch", None),
    "dotdot": (PROJECT, ["/work/project/../a-ctx"], "/work/a-ctx", None),
    "symlink": (
        PROJECT,
        ["/work/zz-link"],
        "/work/a-ctx",
        ("/work/a-ctx", "/work/zz-link"),
    ),
    "project-first": (PROJECT, [PROJECT, "/work/launch"], "/work/launch", None),
    "project-last": (PROJECT, ["/work/launch", PROJECT], "/work/launch", None),
    "cwd-ctx": ("/work/launch", [PROJECT], "/work/launch", None),
    "cwd-zctx": ("/work/zlaunch", [PROJECT], "/work/zlaunch", None),
}
SELECTED = [w for w in os.environ.get("WO_WORLDS", ",".join(WORLDS)).split(",") if w]


def _prelude(name):
    cwd, dirs, ctx, link = WORLDS[name]
    hooks = {
        "uze-wo": {
            "PreInvocation": [
                {
                    "type": "command",
                    "command": f"mkdir -p {HOOK_LOG}; cat > {HOOK_LOG}/pre-$$.json; printf '{{}}'",
                    "timeout": 10,
                }
            ]
        }
    }
    lines = [
        f"mkdir -p {PROJECT} {ctx}/.agents/rules {HOOK_LOG}",
        f"cd {PROJECT} && git init -q -b main . && printf '# Lab project\\n' > AGENTS.md",
        f"printf '%s' {shlex.quote(json.dumps(hooks))} > {ctx}/.agents/hooks.json",
        f"printf '%s\\n' '---' 'trigger: always_on' '---' '' 'Lab convention {RULE_MARK}.' "
        f"> {ctx}/.agents/rules/ctx.md",
    ]
    if link:
        lines.append(f"ln -s {link[0]} {link[1]}")
    lines.append(f"cd {cwd}")
    return "\n".join(lines)


def job(name):
    def run_world(world):
        prov_ip = common.start_provider(world, "static", {"DISCOVERY": "1"})
        _, dirs, _, _ = WORLDS[name]
        adds = " ".join(f"--add-dir {shlex.quote(d)}" for d in dirs)
        final = f"""{_prelude(name)}
set +e
# decision: experiment-isolation
timeout 240 agy {adds} --print {shlex.quote(PROMPT)} --dangerously-skip-permissions --print-timeout 120s 2>&1
echo '=== hooks'
cat {HOOK_LOG}/*.json 2>/dev/null
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
        return {"out": proc.stdout + proc.stderr, "log": probe.provider_log(world)}

    return run_world


def _user_information(reqs):
    for _, body, _ in reqs:
        if not isinstance(body, dict) or not capture.declared_tools(body):
            continue
        inner = body.get("request") if isinstance(body.get("request"), dict) else body
        text = json.dumps(inner.get("systemInstruction", ""))
        if PROMPT.split()[0] not in json.dumps(inner.get("contents", "")):
            continue
        active = re.search(r"Active Workspaces:((?:\\n- [^\\]+)+)", text)
        cwd = re.search(r"Command Working Directory: ([^\\]+)", text)
        return {
            "active": [p for p in active.group(1).split("\\n- ") if p]
            if active
            else None,
            "cwd": cwd.group(1) if cwd else None,
            "rule_loaded": RULE_MARK in text,
        }
    return {"active": None, "cwd": None, "rule_loaded": None}


def judge(cfg, name, out):
    with open(os.path.join(cfg.outdir, f"wo-{name}.out"), "w") as f:
        f.write(out["out"])
    with open(os.path.join(cfg.outdir, f"wo-{name}-raw-requests.log"), "w") as f:
        f.write(out["log"])
    seen = _user_information(probe.requests(out["log"]))
    hooks = out["out"].split("=== hooks", 1)[-1]
    paths = sorted(set(re.findall(r'"workspacePaths":(\[[^\]]*\])', hooks)))
    cwd, dirs, ctx, _ = WORLDS[name]
    result = {
        "world": name,
        "process_cwd": cwd,
        "add_dir": dirs,
        **seen,
        "hook_ran": bool(paths),
        "hook_workspacePaths": paths,
    }
    with open(os.path.join(cfg.outdir, f"wo-{name}-result.json"), "w") as f:
        json.dump(result, f, indent=1)
    probe.say(f"[{name}] {json.dumps(result)}")
    common.check(
        f"wo-{name}-project-is-working-directory",
        seen["cwd"] == PROJECT,
        f"Command Working Directory {seen['cwd']!r}; Active Workspaces {seen['active']}",
    )
    common.check(
        f"wo-{name}-context-loads",
        bool(seen["rule_loaded"]) and bool(paths),
        f"rule in systemInstruction={seen['rule_loaded']}, hook ran={bool(paths)} {paths}",
    )


def run(cfg, prov_ip):
    results = common.concurrently(cfg, {name: job(name) for name in SELECTED})
    for name in SELECTED:
        judge(cfg, name, results[name])
