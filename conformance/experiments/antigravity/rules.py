"""How Antigravity applies the rules in a checkout's `.agents/rules/`.

`project-rules` makes Antigravity's rule format canonical and leaves
Antigravity to read `.agents/rules/` natively; `plugin-rules` puts a
plugin's rules in a namespace inside it. Both wait on what `agy` itself
does with a rule file, which this measures with plain `agy`, no UZE in the
loop: the harness decides whether a rule's text reaches the model request,
so the synthetic provider's capture is the evidence. What a model then does
with the text is not measured here.

Each world is one headless `agy --print` turn, signed in, whose provider
scripts a `view_file` per path in `READS`, in order. Each rule's body
carries its own marker; the judge records, per marker, the first model
request carrying it, how many reads had returned by then, and where it sat
(`systemInstruction`, a user-role step, a function response).

- `glob` (project-rules 1.1): one `trigger: glob` rule per construct
  (`**`, `*`, `?`, `{a,b}`, `[ab]`, a leading `/`, a space, a comma list,
  a YAML list) and a path each should match; `star/*.txt` is read first
  through `star/sub/b.txt`, a path a segment-bound `*` must not match;
- `edge` (project-rules 1.2, plugin-rules 1.1/1.2 and the baseline): the
  `always_on`, `model_decision` and `manual` baselines, a file with no
  frontmatter, an unknown `trigger`, `trigger: glob` without `globs`, and
  rules in `<plugin>@<market>/`, in a plain subdirectory and as
  `<plugin>@<market>--<rule>.md`.

Run: python3 conformance/lab.py --harness antigravity --experiment antigravity/rules
Env: RULES_WORLDS=glob,anchored,anchor,edge
"""

import json
import os
import shlex
import subprocess

from experiments import agent_context_probe as probe
from harnesses.antigravity.bindings import AntigravityBindings
from shared import capture, common

PROJECT = "/work/rules-project"
PROMPT = "UZE_RULES_TURN read the files I name"
WORLDS = [
    w
    for w in os.environ.get("RULES_WORLDS", "glob,anchored,anchor,edge").split(",")
    if w
]

#: (rule file, frontmatter lines or None, marker)
GLOB_RULES = [
    ("dstar.md", ["trigger: glob", "globs: deep/**"], "UZE_RULE_DSTAR"),
    ("dstar-mid.md", ["trigger: glob", "globs: mid/**/leaf.txt"], "UZE_RULE_DSTAR_MID"),
    ("dstar-lead.md", ["trigger: glob", "globs: **/*.dsx"], "UZE_RULE_DSTAR_LEAD"),
    ("dstar-zero.md", ["trigger: glob", "globs: **/*.zro"], "UZE_RULE_DSTAR_ZERO"),
    ("star.md", ["trigger: glob", "globs: star/*.txt"], "UZE_RULE_STAR"),
    ("bare.md", ["trigger: glob", "globs: *.bare"], "UZE_RULE_BARE"),
    ("qmark.md", ["trigger: glob", "globs: q/fil?.txt"], "UZE_RULE_QMARK"),
    ("brace.md", ["trigger: glob", "globs: brace/*.{ts,tsx}"], "UZE_RULE_BRACE"),
    ("class.md", ["trigger: glob", "globs: cls/[ab].txt"], "UZE_RULE_CLASS"),
    ("lead.md", ["trigger: glob", "globs: /lead/*.txt"], "UZE_RULE_LEAD"),
    ("space.md", ["trigger: glob", "globs: space dir/*.txt"], "UZE_RULE_SPACE"),
    ("comma.md", ["trigger: glob", "globs: m1/**, m2/**"], "UZE_RULE_COMMA"),
    ("ylist.md", ["trigger: glob", "globs:", "  - ylist/**"], "UZE_RULE_YLIST"),
    ("abs.md", ["trigger: glob", f"globs: {PROJECT}/abs/*.txt"], "UZE_RULE_ABS"),
]
#: Read in this order: the control and the negative come first, so a rule
#: that arrives with them is a wider match than the construct promises.
GLOB_READS = [
    "none/x.txt",
    "star/sub/b.txt",
    "deep/a/b/c.txt",
    "mid/leaf.txt",
    "x/y/z.dsx",
    "top.zro",
    "star/a.txt",
    "nest/x.bare",
    "q/file.txt",
    "brace/a.tsx",
    "cls/a.txt",
    "lead/a.txt",
    "space dir/a.txt",
    "m2/a.txt",
    "ylist/a.txt",
    "abs/a.txt",
]

#: The same constructs behind `**/`, after the first world matched no
#: pattern that names a directory relative to the checkout: this tells a
#: construct Antigravity lacks from an anchoring it does not do.
ANCHORED_RULES = [
    ("a-dstar.md", ["trigger: glob", "globs: **/deep/**"], "UZE_RULE_A_DSTAR"),
    ("a-mid.md", ["trigger: glob", "globs: **/mid/**/leaf.txt"], "UZE_RULE_A_MID"),
    ("a-star.md", ["trigger: glob", "globs: **/star/*.txt"], "UZE_RULE_A_STAR"),
    ("a-qmark.md", ["trigger: glob", "globs: **/q/fil?.txt"], "UZE_RULE_A_QMARK"),
    ("a-brace.md", ["trigger: glob", "globs: **/brace/*.{ts,tsx}"], "UZE_RULE_A_BRACE"),
    ("a-class.md", ["trigger: glob", "globs: **/cls/[ab].txt"], "UZE_RULE_A_CLASS"),
    ("a-space.md", ["trigger: glob", "globs: **/space dir/*.txt"], "UZE_RULE_A_SPACE"),
    ("a-comma.md", ["trigger: glob", "globs: **/m1/**, **/m2/**"], "UZE_RULE_A_COMMA"),
    ("a-ylist.md", ["trigger: glob", "globs:", "  - **/ylist/**"], "UZE_RULE_A_YLIST"),
    ("a-dot.md", ["trigger: glob", "globs: ./dot/*.txt"], "UZE_RULE_A_DOT"),
    ("a-up.md", ["trigger: glob", "globs: ../../up/*.txt"], "UZE_RULE_A_UP"),
    ("a-onestar.md", ["trigger: glob", "globs: */x.one"], "UZE_RULE_A_ONESTAR"),
    ("a-write.md", ["trigger: glob", "globs: **/wr/*.txt"], "UZE_RULE_A_WRITE"),
]
ANCHORED_READS = [
    "star/sub/b.txt",
    "a/b/x.one",
    "deep/a/b/c.txt",
    "mid/leaf.txt",
    "star/a.txt",
    "q/file.txt",
    "brace/a.tsx",
    "cls/a.txt",
    "space dir/a.txt",
    "m2/a.txt",
    "ylist/a.txt",
    "dot/a.txt",
    "up/a.txt",
]
#: Written, not read: whether a write fires a `glob` rule as a read does.
ANCHORED_WRITE = "wr/new.txt"

#: What the pattern is matched against. A relative pattern naming a
#: directory never matched while `**/` and absolute ones did: is it the
#: absolute path (`**/` then reaches above the checkout, and the path
#: without its leading `/` does not match), or the path from `/`?
ANCHOR_RULES = [
    (
        "p-fsroot.md",
        ["trigger: glob", "globs: work/rules-project/fsroot/*.txt"],
        "UZE_RULE_P_FSROOT",
    ),
    (
        "p-above.md",
        ["trigger: glob", "globs: **/rules-project/above/*.txt"],
        "UZE_RULE_P_ABOVE",
    ),
    ("p-rootbare.md", ["trigger: glob", "globs: *.rootbare"], "UZE_RULE_P_ROOTBARE"),
    ("p-brace1.md", ["trigger: glob", "globs: **/br1/*.{tsx}"], "UZE_RULE_P_BRACE1"),
    ("p-quoted.md", ["trigger: glob", 'globs: "**/qt/*.txt"'], "UZE_RULE_P_QUOTED"),
]
ANCHOR_READS = ["fsroot/a.txt", "above/a.txt", "r.rootbare", "br1/a.tsx", "qt/a.txt"]

EDGE_RULES = [
    ("always.md", ["trigger: always_on"], "UZE_RULE_ALWAYS"),
    (
        "decide.md",
        ["trigger: model_decision", "description: UZE_RULE_DECIDE_DESC lab widgets"],
        "UZE_RULE_DECIDE_BODY",
    ),
    ("manual.md", ["trigger: manual"], "UZE_RULE_MANUAL"),
    ("nofm.md", None, "UZE_RULE_NOFM"),
    ("unknown.md", ["trigger: sometimes"], "UZE_RULE_UNKNOWN"),
    ("globless.md", ["trigger: glob"], "UZE_RULE_GLOBLESS"),
    ("lab-plugin@uze-lab/ui.md", ["trigger: always_on"], "UZE_RULE_SUBDIR_AT"),
    (
        "lab-plugin@uze-lab/ui-glob.md",
        ["trigger: glob", "globs: **/sub/**"],
        "UZE_RULE_SUBDIR_GLOB",
    ),
    ("plain/inner.md", ["trigger: always_on"], "UZE_RULE_SUBDIR_PLAIN"),
    ("lab-plugin@uze-lab--ui.md", ["trigger: always_on"], "UZE_RULE_FLAT"),
    (
        "lab-plugin@uze-lab--glob.md",
        ["trigger: glob", "globs: **/flat/**"],
        "UZE_RULE_FLAT_GLOB",
    ),
]
EDGE_READS = ["other/a.txt", "sub/a.txt", "flat/a.txt"]
#: Markers that are not a rule body: what an index of the rules shows.
EDGE_EXTRA = [("decide.md (description)", None, "UZE_RULE_DECIDE_DESC")]

SETS = {
    "glob": (GLOB_RULES, GLOB_READS),
    "edge": (EDGE_RULES, EDGE_READS),
    "anchored": (ANCHORED_RULES, ANCHORED_READS),
    "anchor": (ANCHOR_RULES, ANCHOR_READS),
}


_WRITES = {"anchored": ANCHORED_WRITE}


def _needle(mark):
    """A body's marker ends at its colon, so `UZE_RULE_DSTAR` is never
    found inside `UZE_RULE_DSTAR_LEAD`."""
    return mark if mark.endswith("_DESC") else f"{mark}:"


def _rule_line(name, front, mark):
    lines = (["---", *front, "---", ""] if front is not None else []) + [
        f"Lab convention {mark}: files here are lab fixtures."
    ]
    path = f".agents/rules/{name}"
    return (
        f"mkdir -p {shlex.quote(os.path.dirname(path))} && printf '%s\\n' "
        + " ".join(shlex.quote(line) for line in lines)
        + f" > {shlex.quote(path)}"
    )


def _prelude(rules, reads):
    lines = [
        f"mkdir -p {PROJECT} && cd {PROJECT} && git init -q -b main .",
        "git config user.name lab && git config user.email lab@uze.invalid",
    ]
    for rel in reads:
        lines.append(
            f"mkdir -p {shlex.quote(os.path.dirname(rel) or '.')} && "
            f"printf 'lab file\\n' > {shlex.quote(rel)}"
        )
    lines += [_rule_line(*r) for r in rules]
    lines.append("git add -A && git commit -q -m init")
    return "\n".join(lines)


def _reads(reads, write=None):
    calls = [
        {
            "name": "view_file",
            "args": {
                "AbsolutePath": f"{PROJECT}/{rel}",
                "toolSummary": "Lab read",
                "toolAction": "Reading file",
            },
        }
        for rel in reads
    ]
    if write:
        calls.append(
            {
                "name": "write_to_file",
                "args": {
                    "TargetFile": f"{PROJECT}/{write}",
                    "CodeContent": "lab",
                    "Description": "lab",
                    "Overwrite": True,
                    "toolSummary": "Lab write",
                    "toolAction": "Writing file",
                },
            }
        )
    return calls


def job(world_name):
    rules, reads = SETS[world_name]

    def run_world(world):
        env = {
            "TOOL_SEQUENCE": json.dumps(_reads(reads, _WRITES.get(world_name))),
            "DISCOVERY": "1",
        }
        prov_ip = common.start_provider(world, "toolcall", env)
        cmd = AntigravityBindings().headless(
            world, prov_ip, _prelude(rules, reads), PROMPT, PROJECT, plugins=""
        )
        proc = subprocess.run(
            cmd, capture_output=True, text=True, errors="replace", timeout=480
        )
        return {"out": proc.stdout + proc.stderr, "log": probe.provider_log(world)}

    return run_world


def _responses(body):
    return sum(
        1
        for c in body.get("contents", [])
        for p in c.get("parts", [])
        if isinstance(p, dict) and "functionResponse" in p
    )


def judge(cfg, name, out):
    rules, reads = SETS[name]
    with open(os.path.join(cfg.outdir, f"rules-{name}.out"), "w") as f:
        f.write(out["out"])
    with open(os.path.join(cfg.outdir, f"rules-{name}-raw-requests.log"), "w") as f:
        f.write(out["log"])
    calls = [
        (i, b["request"] if isinstance(b.get("request"), dict) else b)
        for i, (_, b, _) in enumerate(probe.requests(out["log"]))
        if isinstance(b, dict) and capture.declared_tools(b)
    ]
    steps = reads + ([_WRITES[name]] if name in _WRITES else [])
    result = {"world": name, "model_calls": len(calls), "steps": steps, "rules": {}}
    for rule_name, front, mark in [*rules, *(EDGE_EXTRA if name == "edge" else [])]:
        first = next(
            ((i, b) for i, b in calls if probe.locate(b, _needle(mark))), (None, None)
        )
        i, body = first
        result["rules"][mark] = {
            "file": rule_name,
            "frontmatter": front,
            "first_request": i,
            "reads_returned": _responses(body) if body else None,
            "after_read": steps[_responses(body) - 1]
            if body and _responses(body)
            else None,
            "requests_carrying": sum(
                1 for _, b in calls if probe.locate(b, _needle(mark))
            ),
            "at": [
                {k: h[k] for k in ("path", "role")}
                for h in probe.locate(body, _needle(mark))
            ]
            if body
            else [],
            "excerpt": probe.locate(body, _needle(mark))[0]["excerpt"]
            if body
            else None,
        }
    with open(os.path.join(cfg.outdir, f"rules-{name}-result.json"), "w") as f:
        json.dump(result, f, indent=1)
    for mark, row in result["rules"].items():
        probe.say(
            f"[{name}] {mark}: request={row['first_request']} after={row['after_read']!r} "
            f"carrying={row['requests_carrying']} at={[a['path'] for a in row['at']][:3]}"
        )
    common.check(
        f"rules-{name}-turn-ran",
        len(calls) > len(steps),
        f"{len(calls)} model calls for {len(steps)} scripted steps",
    )


def run(cfg, prov_ip):
    results = common.concurrently(cfg, {name: job(name) for name in WORLDS})
    for name in WORLDS:
        judge(cfg, name, results[name])
