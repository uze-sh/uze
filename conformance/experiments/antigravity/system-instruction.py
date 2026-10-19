"""Which launch-scoped route puts text where Antigravity puts `AGENTS.md`?

`experiments/<h>/launch-instructions` measured UZE's Antigravity channel
(a `PreInvocation` hook answering `injectSteps[].systemMessage`) landing as
a user-role `<SYSTEM_MESSAGE>` after the person's first request, while the
project's `AGENTS.md` sits in `systemInstruction`. This measures the
candidates that stay scoped to one process: everything lives in a
directory the launch adds with `--add-dir`, outside the project and
outside every location each session reads. No UZE in the loop: plain
`agy`, so what is measured is the harness.

One world per candidate (`SI_WORLDS`, comma-separated), each one TUI:
`agy --add-dir /work/launch`, a first turn the provider answers with an
`invoke_subagent` call, then the process ends and `agy -c` resumes it with
a second turn. Per world, for its marker: where it lands in the parent's
request, in the subagent's own request and in the resumed request, beside
the project's `AGENTS.md` marker; and what `workspacePaths` the hooks were
handed (a logger hook prints `{}` in every world).

- `agents-md`, `gemini-md`: `/work/launch/AGENTS.md` / `GEMINI.md`;
- `rule`: `/work/launch/.agents/rules/uze-li.md`, `trigger: always_on`;
- `plugin-rule`: a plugin in `/work/launch/.agents/plugins/uze-li/` with
  `rules/AGENTS.md`;
- `session-start`: a `SessionStart` hook answering `systemMessage`;
- `preinv-every`: a `PreInvocation` hook answering `systemMessage` on
  every invocation, with no once-per-launch marker (the control for the
  subagent miss of UZE's handler).

Run: python3 conformance/lab.py --harness antigravity --experiment antigravity/system-instruction
"""

import json
import os
import shlex

from contract import continuity
from contract.tui import Tui
from experiments import agent_context_probe as probe
from experiments.launch_instructions_probe import _in_tool_call, placement
from harnesses.antigravity.bindings import AntigravityBindings
from harnesses.antigravity.scenarios import agy_setup
from shared import capture, common, vocabulary

WORLDS = [
    w
    for w in os.environ.get(
        "SI_WORLDS",
        "agents-md,gemini-md,rule,plugin-rule,session-start,preinv-every",
    ).split(",")
    if w
]
#: `SI_LAUNCH=/work/zlaunch` sorts the added directory after the project,
#: to tell "added first" from "sorted first".
LAUNCH = os.environ.get("SI_LAUNCH", "/work/launch")
PROJECT = "/work/project"
HOOK_LOG = "/work/hooklog"
PROJECT_MARK = "UZE_SI_PROJECT_AGENTS"
SPAWN = "UZE_SI_SPAWN delegate this to a subagent"
SUB_TURN = "UZE_SI_SUBAGENT_TURN"
TURN_TWO = "UZE_SI_TURN_TWO"


def marker(world):
    return "UZE_SI_" + world.upper().replace("-", "_")


def _hooks(world, mark):
    log = {
        "type": "command",
        "command": f"mkdir -p {HOOK_LOG}; cat > {HOOK_LOG}/pre-$$.json; printf '{{}}'",
        "timeout": 10,
    }
    spec = {"PreInvocation": [log]}
    step = json.dumps(
        {"injectSteps": [{"systemMessage": {"systemMessage": f"Launch rule {mark}."}}]}
    )
    if world == "session-start":
        spec["SessionStart"] = [
            {
                "type": "command",
                "command": f"mkdir -p {HOOK_LOG}; cat > {HOOK_LOG}/start-$$.json; printf '%s' {shlex.quote(step)}",
                "timeout": 10,
            }
        ]
    if world == "preinv-every":
        spec["PreInvocation"].append(
            {
                "type": "command",
                "command": f"printf '%s' {shlex.quote(step)}",
                "timeout": 10,
            }
        )
    return {"uze-si": spec}


def _candidate(world, mark):
    text = f"Every agent launched here follows the launch rule {mark}."
    if world == "agents-md":
        return f"printf '%s\\n' {shlex.quote(text)} > {LAUNCH}/AGENTS.md"
    if world == "gemini-md":
        return f"printf '%s\\n' {shlex.quote(text)} > {LAUNCH}/GEMINI.md"
    if world == "rule":
        return (
            f"mkdir -p {LAUNCH}/.agents/rules && printf '%s\\n' '---' 'trigger: always_on' "
            f"'---' '' {shlex.quote(text)} > {LAUNCH}/.agents/rules/uze-li.md"
        )
    if world == "plugin-rule":
        root = f"{LAUNCH}/.agents/plugins/uze-li"
        return (
            f'mkdir -p {root}/rules && printf \'{{"name": "uze-li"}}\\n\' > {root}/plugin.json '
            f"&& printf '%s\\n' {shlex.quote(text)} > {root}/rules/AGENTS.md"
        )
    return ""


def _prelude(world):
    mark = marker(world)
    return f"""
mkdir -p {PROJECT} {LAUNCH}/.agents {HOOK_LOG}
cd {PROJECT} && git init -q -b main . && git config user.name lab && git config user.email lab@uze.invalid
printf '# Lab project\\n\\nEvery agent here follows the house rule {PROJECT_MARK}.\\n' > AGENTS.md
git add . && git commit -q -m init
printf '%s' {shlex.quote(json.dumps(_hooks(world, mark)))} > {LAUNCH}/.agents/hooks.json
{_candidate(world, mark)}
"""


def _spawn_call():
    row = vocabulary.load_expectation("antigravity")["aliases"]["agent.spawn"]
    call = json.loads(json.dumps(row["call"]).replace("lab subagent", SUB_TURN))
    return {"tool": vocabulary.call_tool(row), "args": call}


def job(world_name):
    def run_world(world):
        bindings = AntigravityBindings()
        mode, env = bindings.sequence([_spawn_call()], SPAWN)
        env = dict(env or {}, DISCOVERY="1")
        prov_ip = common.start_provider(world, mode, env)
        final = (
            f"cd {PROJECT} && agy --add-dir {LAUNCH}; printf '\\n{continuity.ENDED_MARKER}\\n'; "
            f"agy -c --add-dir {LAUNCH}"
        )
        setup = agy_setup(
            world,
            prov_ip,
            include_mcp=False,
            final_cmd=final,
            prelude=_prelude(world_name),
        )
        out = {"world": world_name}
        tui = Tui(world, common.docker_base(world, prov_ip, setup), f"si-{world_name}")
        with tui:
            plain, ready = bindings.prepare(tui)
            out["ready"] = bool(ready)
            if not ready:
                out["log"] = probe.provider_log(world)
                return out
            turn = bindings.hook_turn(tui, SPAWN)
            out["turn"] = turn.detail
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


def _model_calls(reqs):
    return [
        (i, b["request"])
        for i, (_, b, _) in enumerate(reqs)
        if isinstance(b, dict)
        and isinstance(b.get("request"), dict)
        and capture.declared_tools(b)
    ]


def judge(cfg, name, out):
    mark = marker(name)
    with open(os.path.join(cfg.outdir, f"{name}-raw-requests.log"), "w") as f:
        f.write(out.get("log", ""))
    reqs = probe.requests(out.get("log", ""))
    calls = _model_calls(reqs)
    parent = next(
        (
            (i, b)
            for i, b in calls
            if SPAWN in json.dumps(b) and "functionCall" not in json.dumps(b)
        ),
        (None, None),
    )
    sub = next(
        (
            (i, b)
            for i, b in calls
            if any(not _in_tool_call(h["path"]) for h in placement(b, SUB_TURN))
        ),
        (None, None),
    )
    resumed = next(
        ((i, b) for i, b in calls if TURN_TWO in json.dumps(b)), (None, None)
    )
    paths = sorted(
        {
            json.dumps(json.loads(text).get("workspacePaths"))
            for text in out.get("hooks", {}).values()
            if text.strip().startswith("{")
        }
    )
    result = {"world": name, "marker": mark, "workspacePaths": paths}
    for role, (i, body) in (
        ("parent", parent),
        ("subagent", sub),
        ("resumed", resumed),
    ):
        result[role] = {
            "index": i,
            "candidate": placement(body, mark) if body else None,
            "agents_md": placement(body, PROJECT_MARK) if body else None,
            "first_user_request": next(
                (
                    k
                    for k, c in enumerate((body or {}).get("contents", []))
                    if "<USER_REQUEST>" in json.dumps(c)
                ),
                None,
            ),
        }
    with open(os.path.join(cfg.outdir, f"{name}-result.json"), "w") as f:
        json.dump(result, f, indent=1)
    probe.say(f"[{name}] {json.dumps(result)[:1500]}")
    for role in ("parent", "subagent", "resumed"):
        hits = result[role]["candidate"] or []
        common.check(
            f"si-{name}-{role}-in-system-instruction",
            any(h["path"].startswith("systemInstruction") for h in hits),
            f"request #{result[role]['index']}: {hits}; AGENTS.md {result[role]['agents_md']}",
        )
    common.check(
        f"si-{name}-workspace-stays-project",
        bool(paths)
        and all(json.loads(p) and json.loads(p)[0] == PROJECT for p in paths),
        f"workspacePaths handed to hooks: {paths}; ready={out.get('ready')}, "
        f"ended={out.get('ended')}, turn={out.get('turn')}",
    )


def run(cfg, prov_ip):
    results = common.concurrently(cfg, {name: job(name) for name in WORLDS})
    for name in WORLDS:
        judge(cfg, name, results[name])
