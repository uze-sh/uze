"""Whether `.agents/rules.json` makes Antigravity read rules in subfolders.

`experiments/antigravity/rules` measured that `agy` reads only the
immediate `.md` children of `.agents/rules/`, so `plugin-rules` copies a
plugin's rules flat, as `<plugin>@<market>--<rule>.md`. Antigravity's docs
(antigravity.google/docs/rules) say a nested file is ignored "unless you
explicitly register them in `.agents/rules.json`", with `entries[].path`,
`include_only` and `exclude`, and leave open what `path` is relative to,
whether it may name a directory, and what the filters match. This measures
it with plain `agy`, no UZE in the loop, the same way: each rule's body
carries a marker, and the provider's capture says where it landed.

Headless worlds (one `agy --print` turn each; the provider scripts a read
of `other/a.txt`, a read of `src/a.txt`, then a write of `wr/new.txt`):

- `control`: `.agents/rules/acme@corp/{style,ui,wr}.md`, no `rules.json`;
- `rel-rules`, `rel-dot`, `rel-agents`, `rel-abs`: the same, with one entry
  whose `path` is `rules/acme@corp`, `./rules/acme@corp`,
  `.agents/rules/acme@corp` or the absolute path;
- `file-entry`: an entry naming `rules/acme@corp/style.md` itself;
- `noat`: the folder named `acme-corp`, to tell the `@` from the rest;
- `nested`: whether a directory entry reaches `sub/inner.md` below it;
- `multi`: two plugin folders and an operator's own folder outside
  `.agents/`, each its own entry;
- `dup`, `dup-file`: a flat rule beside `acme@corp/`, and an entry for
  `.agents/rules` or for the flat rule itself;
- `inc-name`, `inc-rel`, `exc-name`, `exc-dir`: `include_only`/`exclude`
  on a directory entry, by file name, relative path and `sub/`;
- `badjson`: a `rules.json` that does not parse, beside a flat rule.

TUI world `resume`: a folder rule registered by `rules.json`, a first turn
the provider answers with `invoke_subagent`, then `agy -c`: where the rule
lands in the parent's, the subagent's and the resumed request.

Run: python3 conformance/lab.py --harness antigravity --experiment antigravity/rules-manifest
Env: RM_WORLDS=control,rel-rules,... (default: all); RM_PATH=<the prefix every world
past `rel-*` names a folder with> (default `.agents/rules/`, the one `rel-*` found).
"""

import json
import os
import shlex
import subprocess

from contract import continuity
from contract.tui import Tui
from experiments import agent_context_probe as probe
from experiments.launch_instructions_probe import placement
from harnesses.antigravity.bindings import AntigravityBindings
from harnesses.antigravity.scenarios import agy_setup
from shared import capture, common, vocabulary

PROJECT = "/work/rules-project"
PROMPT = "UZE_RM_TURN read the files I name"
READS = ["other/a.txt", "src/a.txt"]
WRITE = "wr/new.txt"
#: The prefix an entry path takes to name a folder under `.agents/rules/`:
#: settled by the `rel-*` worlds, and used by every world after them.
PREFIX = os.environ.get("RM_PATH", ".agents/rules/")

ALWAYS = ["trigger: always_on"]
SRC = ["trigger: glob", "globs: **/src/**"]
WR = ["trigger: glob", "globs: **/wr/**"]


def plugin(folder, tag):
    """A plugin's three rules: always_on, a glob a read fires, a glob a
    write fires."""
    return {
        f".agents/rules/{folder}/style.md": (ALWAYS, f"UZE_RM_{tag}_STYLE"),
        f".agents/rules/{folder}/ui.md": (SRC, f"UZE_RM_{tag}_UI"),
        f".agents/rules/{folder}/wr.md": (WR, f"UZE_RM_{tag}_WR"),
    }


def entries(*items):
    return json.dumps({"entries": list(items)})


ACME = plugin("acme@corp", "ACME")
FLAT = {".agents/rules/flat.md": (ALWAYS, "UZE_RM_FLAT")}
FILTERED = {
    ".agents/rules/acme@corp/a.md": (ALWAYS, "UZE_RM_F_A"),
    ".agents/rules/acme@corp/b.md": (ALWAYS, "UZE_RM_F_B"),
    ".agents/rules/acme@corp/sub/a.md": (ALWAYS, "UZE_RM_F_SUB_A"),
    ".agents/rules/acme@corp/sub/c.md": (ALWAYS, "UZE_RM_F_SUB_C"),
}


def _filtered(**filters):
    return (FILTERED, entries({"path": f"{PREFIX}acme@corp", **filters}))


#: world: (rule files {project path: (frontmatter, marker)}, rules.json text or None)
WORLDS_SPEC = {
    "control": (ACME, None),
    "rel-rules": (ACME, entries({"path": "rules/acme@corp"})),
    "rel-dot": (ACME, entries({"path": "./rules/acme@corp"})),
    "rel-agents": (ACME, entries({"path": ".agents/rules/acme@corp"})),
    "rel-abs": (ACME, entries({"path": f"{PROJECT}/.agents/rules/acme@corp"})),
    "file-entry": (ACME, entries({"path": f"{PREFIX}acme@corp/style.md"})),
    "noat": (plugin("acme-corp", "NOAT"), entries({"path": f"{PREFIX}acme-corp"})),
    "nested": (
        {
            **ACME,
            ".agents/rules/acme@corp/sub/inner.md": (ALWAYS, "UZE_RM_NESTED_INNER"),
        },
        entries({"path": f"{PREFIX}acme@corp"}),
    ),
    "multi": (
        {
            **ACME,
            **plugin("beta@corp", "BETA"),
            "team-rules/house.md": (ALWAYS, "UZE_RM_TEAM_HOUSE"),
            **FLAT,
        },
        entries(
            {"path": f"{PREFIX}acme@corp"},
            {"path": f"{PREFIX}beta@corp"},
            {"path": "team-rules"},
        ),
    ),
    "dup": ({**ACME, **FLAT}, entries({"path": PREFIX.rstrip("/")})),
    "dup-file": ({**ACME, **FLAT}, entries({"path": f"{PREFIX}flat.md"})),
    "inc-name": _filtered(include_only=["a.md"]),
    "inc-rel": _filtered(include_only=["sub/c.md"]),
    "exc-name": _filtered(exclude=["a.md"]),
    "exc-dir": _filtered(exclude=["sub/"]),
    "badjson": (
        {**ACME, **FLAT},
        '{ "entries": [ { "path": "' + PREFIX + 'acme@corp" } ',
    ),
}
TUI_WORLDS = ("resume",)
WORLDS = [
    w
    for w in os.environ.get("RM_WORLDS", ",".join([*WORLDS_SPEC, *TUI_WORLDS])).split(
        ","
    )
    if w
]


def _needle(mark):
    """A body's marker ends at its colon, so `UZE_RM_F_A` is never found
    inside `UZE_RM_F_SUB_A`."""
    return f"{mark}:"


def _rule_line(path, front, mark):
    lines = [
        "---",
        *front,
        "---",
        "",
        f"Lab convention {mark}: files here are lab fixtures.",
    ]
    return (
        f"mkdir -p {shlex.quote(os.path.dirname(path))} && printf '%s\\n' "
        + " ".join(shlex.quote(line) for line in lines)
        + f" > {shlex.quote(path)}"
    )


def _prelude(files, manifest):
    lines = [
        f"mkdir -p {PROJECT} && cd {PROJECT} && git init -q -b main .",
        "git config user.name lab && git config user.email lab@uze.invalid",
        "mkdir -p .agents",
    ]
    for rel in READS:
        lines.append(
            f"mkdir -p {shlex.quote(os.path.dirname(rel))} && "
            f"printf 'lab file\\n' > {shlex.quote(rel)}"
        )
    lines += [_rule_line(path, *spec) for path, spec in files.items()]
    if manifest is not None:
        lines.append(f"printf '%s' {shlex.quote(manifest)} > .agents/rules.json")
    lines.append("git add -A && git commit -q -m init")
    return "\n".join(lines)


def _steps():
    calls = [
        {
            "name": "view_file",
            "args": {
                "AbsolutePath": f"{PROJECT}/{rel}",
                "toolSummary": "Lab read",
                "toolAction": "Reading file",
            },
        }
        for rel in READS
    ]
    calls.append(
        {
            "name": "write_to_file",
            "args": {
                "TargetFile": f"{PROJECT}/{WRITE}",
                "CodeContent": "lab",
                "Description": "lab",
                "Overwrite": True,
                "toolSummary": "Lab write",
                "toolAction": "Writing file",
            },
        }
    )
    return calls


def headless_job(name):
    files, manifest = WORLDS_SPEC[name]

    def run_world(world):
        env = {"TOOL_SEQUENCE": json.dumps(_steps()), "DISCOVERY": "1"}
        prov_ip = common.start_provider(world, "toolcall", env)
        cmd = AntigravityBindings().headless(
            world, prov_ip, _prelude(files, manifest), PROMPT, PROJECT, plugins=""
        )
        proc = subprocess.run(
            cmd, capture_output=True, text=True, errors="replace", timeout=480
        )
        return {"out": proc.stdout + proc.stderr, "log": probe.provider_log(world)}

    return run_world


SPAWN = "UZE_RM_SPAWN delegate this to a subagent"
SUB_TURN = "UZE_RM_SUBAGENT_TURN"
TURN_TWO = "UZE_RM_TURN_TWO"
RESUME_FILES = {
    ".agents/rules/acme@corp/style.md": (ALWAYS, "UZE_RM_RESUME_STYLE"),
    **FLAT,
}


def _spawn_call():
    row = vocabulary.load_expectation("antigravity")["aliases"]["agent.spawn"]
    call = json.loads(json.dumps(row["call"]).replace("lab subagent", SUB_TURN))
    return {"tool": vocabulary.call_tool(row), "args": call}


def tui_job(name):
    manifest = entries({"path": f"{PREFIX}acme@corp"})

    def run_world(world):
        bindings = AntigravityBindings()
        mode, env = bindings.sequence([_spawn_call()], SPAWN)
        env = dict(env or {}, DISCOVERY="1")
        prov_ip = common.start_provider(world, mode, env)
        final = f"cd {PROJECT} && agy; printf '\\n{continuity.ENDED_MARKER}\\n'; agy -c"
        setup = agy_setup(
            world,
            prov_ip,
            include_mcp=False,
            final_cmd=final,
            prelude=_prelude(RESUME_FILES, manifest),
        )
        out = {"world": name}
        tui = Tui(world, common.docker_base(world, prov_ip, setup), f"rm-{name}")
        with tui:
            plain, ready = bindings.prepare(tui)
            out["ready"] = bool(ready)
            if not ready:
                out["log"] = probe.provider_log(world)
                return out
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
            out["log"] = probe.provider_log(world)
        return out

    return run_world


def _model_calls(log):
    return [
        (i, b["request"] if isinstance(b.get("request"), dict) else b)
        for i, (_, b, _) in enumerate(probe.requests(log))
        if isinstance(b, dict) and capture.declared_tools(b)
    ]


def _responses(body):
    return sum(
        1
        for c in body.get("contents", [])
        for p in c.get("parts", [])
        if isinstance(p, dict) and "functionResponse" in p
    )


def _save(cfg, name, out, result):
    for suffix, text in (
        ("out", out.get("out", "")),
        ("raw-requests.log", out.get("log", "")),
    ):
        with open(os.path.join(cfg.outdir, f"rm-{name}-{suffix}"), "w") as f:
            f.write(text)
    with open(os.path.join(cfg.outdir, f"rm-{name}-result.json"), "w") as f:
        json.dump(result, f, indent=1)


def judge_headless(cfg, name, out):
    files, manifest = WORLDS_SPEC[name]
    calls = _model_calls(out["log"])
    steps = [*READS, WRITE]
    result = {
        "world": name,
        "rules.json": manifest,
        "model_calls": len(calls),
        "rules": {},
    }
    for path, (front, mark) in files.items():
        needle = _needle(mark)
        carrying = [(i, b) for i, b in calls if probe.locate(b, needle)]
        i, body = carrying[0] if carrying else (None, None)
        last = calls[-1][1] if calls else {}
        result["rules"][mark] = {
            "file": path,
            "trigger": front[0],
            "first_request": i,
            "after_step": steps[_responses(body) - 1]
            if body and _responses(body)
            else None,
            "requests_carrying": len(carrying),
            "copies_in_last_request": json.dumps(last).count(needle),
            "at": sorted({h["path"].split("[")[0] for h in probe.locate(body, needle)})
            if body
            else [],
        }
    _save(cfg, name, out, result)
    for mark, row in result["rules"].items():
        probe.say(
            f"[{name}] {mark}: request={row['first_request']} after={row['after_step']!r} "
            f"carrying={row['requests_carrying']} copies={row['copies_in_last_request']} at={row['at']}"
        )
    common.check(
        f"rm-{name}-turn-ran",
        len(calls) > len(steps),
        f"{len(calls)} model calls for {len(steps)} scripted steps",
    )


def judge_tui(cfg, name, out):
    calls = _model_calls(out.get("log", ""))
    parent = next(
        (
            (i, b)
            for i, b in calls
            if SPAWN in json.dumps(b) and "functionCall" not in json.dumps(b)
        ),
        (None, None),
    )
    sub = next(
        ((i, b) for i, b in calls if SUB_TURN in json.dumps(b.get("contents", [])[:1])),
        (None, None),
    )
    resumed = next(
        ((i, b) for i, b in calls if TURN_TWO in json.dumps(b)), (None, None)
    )
    result = {"world": name, "ready": out.get("ready"), "ended": out.get("ended")}
    for role, (i, body) in (
        ("parent", parent),
        ("subagent", sub),
        ("resumed", resumed),
    ):
        result[role] = {
            "index": i,
            **{
                mark: placement(body, _needle(mark)) if body else None
                for _, mark in RESUME_FILES.values()
            },
        }
    _save(cfg, name, out, result)
    probe.say(f"[{name}] {json.dumps(result)[:2000]}")
    for role in ("parent", "subagent", "resumed"):
        for _, mark in RESUME_FILES.values():
            hits = result[role][mark] or []
            common.check(
                f"rm-{name}-{role}-{mark}-in-system-instruction",
                any(h["path"].startswith("systemInstruction") for h in hits),
                f"request #{result[role]['index']}: {hits}",
            )


def run(cfg, prov_ip):
    jobs = {
        name: (tui_job(name) if name in TUI_WORLDS else headless_job(name))
        for name in WORLDS
    }
    results = common.concurrently(cfg, jobs)
    for name in WORLDS:
        (judge_tui if name in TUI_WORLDS else judge_headless)(cfg, name, results[name])
