"""What a project's rules must do, on every harness.

A checkout keeps its rules in `.agents/rules/`, one Markdown file each, and
the frontmatter's `trigger` says when one applies. UZE's promise is that
every harness hands each rule to the model at that moment: natively where
the vendor reads the directory, through the rules engine `uze setup`
delivers where it does not. Which of the two a harness needs is the
product's business; the outcome is what the model request carries, so the
synthetic provider's captured requests are the evidence. What a model then
does with a rule's text is not measured here.

The scene is a project a person has: a Git checkout with a `glob` rule
(`**/ui/**`), an `always_on` rule and a `model_decision` rule, each body
carrying its own marker and the last one's description another. Before the
session the harnesses are set up the way opening the workspace sets up a
harness it finds installed, from the executable already there: `uze
setup` would also update it, which an offline world cannot.

- One headless turn, scripted: write a file in another directory (the
  control), write a file the glob matches, read the `model_decision` rule's
  file. Each request is placed by how many tool results it carries back,
  which tells the first request from the one after each step.
  - `glob`: the body arrives once the matching write returned, and in no
    request before it, the one after the control's write included.
  - `always_on`: the body is in the turn's first request.
  - `model_decision`: the description is in the first request, and the
    body in none until the read returned it.
- One live session, compacted the way a person compacts it: a turn, the
  harness's compaction command, a second turn. Every provider answers a
  compaction with a summary carrying its own marker, so the second turn's
  request carrying it is what says the compaction happened (a harness may
  keep the person's earlier prompts across one); the always-on rule must be
  in that request.
- Two checkouts on one machine, one declaring the `rules-plugin` fixture,
  whose one rule is a `glob` (`**/ui/**`): the same scripted turn (the
  control's write, then the matching one) runs in the declaring checkout
  in one world and in the other checkout in another, each world's machine
  holding both. The declaring checkout holds the copy under the plugin's
  folder and the `.agents/rules.json` entry registering it, read off the
  disk, and the other holds neither. The plugin's rule body arrives after
  the matching write in the declaring checkout, once per request (a rule
  read both flat and through the folder would arrive twice), and in no
  request of the other, whose turn reached the same step.
"""

import copy
import json
import os
import shlex

from contract import declared
from shared import vocabulary
from shared.common import check, concurrently, describe, provider_struct, start_provider
from shared.markers import COMPACTION, RULES, RULES_TURNS

#: The file the glob rule matches, and the control beside it that it must
#: not: same name, another directory.
MATCHING = "src/ui/view.txt"
CONTROL = "src/docs/view.txt"
DECIDE = ".agents/rules/decide.md"

#: How many tool results a request carries back once each scripted step
#: returned: the control's write, the matching write, the read.
AFTER_CONTROL, AFTER_MATCH, AFTER_READ = 1, 2, 3

#: What the static provider answers every turn of the live session with.
STATIC_ANSWER = "UZE_CONFORMANCE_OK"

#: The workspace sets up every harness it finds installed and not set up,
#: then opens; it is ended as soon as its setup report says it is done.
SETUP = """
cd /work
script -qfc 'uze workspace' /work/rules-setup.typescript </dev/null >/dev/null 2>&1 &
setup_pid=$!
i=0
while [ $i -lt 120 ] && ! grep -aq ' ready' /work/rules-setup.typescript 2>/dev/null; do
  sleep 0.5; i=$((i+1))
done
kill $setup_pid 2>/dev/null
"""


def _rule(name, front, body):
    lines = ["---", *front, "---", "", body]
    words = " ".join(shlex.quote(line) for line in lines)
    return f"printf '%s\\n' {words} > .agents/rules/{name}"


def prelude(project):
    """The checkout, committed, and the harnesses set up."""
    rules = [
        _rule(
            "ui.md",
            ["trigger: glob", "globs: **/ui/**"],
            f"Every view under ui follows the lab convention {RULES['glob']}.",
        ),
        _rule(
            "always.md",
            ["trigger: always_on"],
            f"Every answer in this project follows {RULES['always']}.",
        ),
        _rule(
            "decide.md",
            [
                "trigger: model_decision",
                f"description: {RULES['decide_index']} read before changing lab widgets",
            ],
            f"Lab widgets follow {RULES['decide_body']}.",
        ),
    ]
    return "\n".join(
        [
            f"mkdir -p {project}/.agents/rules && cd {project}",
            "git init -q -b main .",
            "git config user.name lab && git config user.email lab@uze.invalid",
            "printf 'lab project\\n' > README.md",
            *rules,
            "git add -A && git commit -q -m init",
            SETUP,
            f"cd {project}",
        ]
    )


#: The plugin whose rules are copied into the checkouts that declare it,
#: and the checkout beside the hook project on the same machine.
RULES_PLUGIN = "rules-plugin@uze-lab"
OTHER_PROJECT = "/work/rules-other"
#: The folder a declaration copies the plugin's rules into, the manifest
#: that registers it, and the rule copied there.
PLUGIN_FOLDER = f".agents/rules/{RULES_PLUGIN}"
MANIFEST = ".agents/rules.json"
PLUGIN_COPY = f"{PLUGIN_FOLDER}/ui.md"
#: Printed around each checkout's rule files and its manifest once the
#: declaration ran, so the run's evidence is what the disk held.
COPIES_BEGIN = "UZE_RULES_COPIES_BEGIN"
COPIES_END = "UZE_RULES_COPIES_END"
MANIFEST_BEGIN = "UZE_RULES_MANIFEST_BEGIN"
MANIFEST_END = "UZE_RULES_MANIFEST_END"


def plugin_prelude(project, declaring):
    """The hook project and another checkout on one machine, `project`
    declaring the plugin when `declaring` and the other one otherwise, and
    the harnesses set up."""
    declared = project if declaring else OTHER_PROJECT
    lines = []
    for root in (project, OTHER_PROJECT):
        lines += [
            f"mkdir -p {root} && cd {root}",
            "git init -q -b main .",
            "git config user.name lab && git config user.email lab@uze.invalid",
            "printf 'lab project\\n' > README.md",
            "git add -A && git commit -q -m init",
        ]
    return "\n".join(
        [
            *lines,
            f"cd {declared}",
            f"uze {RULES_PLUGIN} >/work/rules-plugin-add.log 2>&1 || cat /work/rules-plugin-add.log",
            "git add -A && git commit -q -m 'declare the rules plugin' || true",
            SETUP,
            *(_disk(root) for root in (project, OTHER_PROJECT)),
            f"cd {project}",
        ]
    )


def _disk(root):
    """Prints `root`'s rule files and its manifest on one line each."""
    return (
        f"cd {root}; echo {COPIES_BEGIN} {root}; "
        f"find .agents -type f 2>/dev/null | sort | tr '\\n' ' '; echo {COPIES_END}; "
        f"echo {MANIFEST_BEGIN} {root}; tr -d '\\n' 2>/dev/null < {MANIFEST} || true; echo; "
        f"echo {MANIFEST_END}"
    )


def _read_disk(output, root):
    """What `_disk` printed for `root`: its files under `.agents`, and its
    manifest's text (`None` when it had none), or `None` for a turn whose
    output does not carry the prelude's."""
    copies = _between(output, f"{COPIES_BEGIN} {root}", COPIES_END)
    manifest = _between(output, f"{MANIFEST_BEGIN} {root}", MANIFEST_END)
    if copies is None or manifest is None:
        return None
    return {"files": copies.split(), "manifest": manifest.strip() or None}


def _between(output, begin, end):
    if begin not in output:
        return None
    rest = output.split(begin, 1)[1]
    return rest.split(end, 1)[0] if end in rest else None


def _registered(disk):
    """Whether the manifest registers the plugin's folder, and why not."""
    if disk["manifest"] is None:
        return False, "no manifest"
    try:
        entries = json.loads(disk["manifest"]).get("entries", [])
    except (ValueError, AttributeError):
        return False, f"the manifest does not parse: {disk['manifest'][:120]}"
    paths = [e.get("path") for e in entries if isinstance(e, dict)]
    return PLUGIN_FOLDER in paths, f"entries {paths}"


def _write(bindings, expected, relative):
    row = expected["aliases"]["file.write"]
    return {
        "tool": vocabulary.call_tool(row),
        "args": bindings.call(row["call"], side=relative),
    }


def assert_contract(cfg, prov_ip, bindings):
    expected = vocabulary.load_expectation(bindings.harness)
    with describe("rules"):
        found = concurrently(
            cfg,
            {
                "rules-turn": lambda world: _turn(world, bindings, expected),
                "rules-compact": lambda world: _compacted(world, bindings),
                "rules-plugin-declared": lambda world: _plugin_turn(
                    world, bindings, expected, True
                ),
                "rules-plugin-undeclared": lambda world: _plugin_turn(
                    world, bindings, expected, False
                ),
            },
        )
        _assert_turn(bindings, *found["rules-turn"])
        _assert_compacted(bindings, found["rules-compact"])
        _assert_plugin(
            bindings, found["rules-plugin-declared"], found["rules-plugin-undeclared"]
        )
    start_provider(cfg, "static")


def _turn(world, bindings, expected):
    """The scripted headless turn; returns the model requests it made, in
    order, and the container's output."""
    project = bindings.hook_project
    calls = [
        _write(bindings, expected, CONTROL),
        _write(bindings, expected, MATCHING),
        bindings.read_call(expected, f"{bindings.hook_project}/{DECIDE}"),
    ]
    mode, env = bindings.sequence(calls, RULES_TURNS[0])
    prov_ip = start_provider(world, mode, env)
    prompt = f"{RULES_TURNS[0]} make the edits I name"
    output = bindings.project_turn(world, prov_ip, prelude(project), prompt, project)
    requests = _model_requests(provider_struct(world))
    with open(os.path.join(world.outdir, "rules-turn.out"), "w") as f:
        f.write(output)
    with open(os.path.join(world.outdir, "rules-turn-requests.json"), "w") as f:
        json.dump(_evidence(requests), f, indent=1)
    return requests, output


def _model_requests(struct):
    """What the model had in front of it at each of its calls, in order:
    the requests that offered it tools or continued one that did, not a
    title or a summary asked on the side. A request that only continues
    the one before it (it names it and carries what was added since) is
    folded onto it, so every entry reads as the whole conversation so far."""
    seen = []
    for record in struct:
        request = record.get("summary", {})
        if request.get("continues") and seen:
            before = seen[-1]
            markers = dict(before.get("rule_markers", {}))
            for marker, present in request.get("rule_markers", {}).items():
                markers[marker] = markers.get(marker, False) or present
            counts = dict(before.get("rule_counts", {}))
            for marker, count in request.get("rule_counts", {}).items():
                counts[marker] = counts.get(marker, 0) + count
            seen.append(
                {
                    **request,
                    "rule_markers": markers,
                    "rule_counts": counts,
                    "tool_results": _results(before) + _results(request),
                }
            )
        elif request.get("offers_tools"):
            seen.append(request)
    return seen


def _carries(request, marker):
    return bool(request.get("rule_markers", {}).get(marker))


def _count(request, marker):
    return request.get("rule_counts", {}).get(marker, 0)


def _results(request):
    return request.get("tool_results") or 0


def _assert_turn(bindings, requests, output):
    turn = [r for r in requests if _carries(r, RULES_TURNS[0])]
    check(
        "rules-turn-reached-model",
        bool(turn),
        f"{len(turn)} model requests in the turn"
        if turn
        else f"the turn never reached the model: {output[-200:]}".replace("\n", " "),
    )
    if not turn:
        return
    steps = sorted({_results(r) for r in turn})
    first = turn[0]

    glob = RULES["glob"]
    after_match = [r for r in turn if _results(r) >= AFTER_MATCH]
    before_match = [r for r in turn if _results(r) < AFTER_MATCH]
    reached = any(_carries(r, glob) for r in after_match)
    declared.presence(
        bindings,
        "rules-glob-reaches-model",
        "rules-glob-reaches-model",
        reached,
        f"the glob rule's body is in a request after the matching write returned "
        f"(tool results per request: {steps})",
        measured=bool(after_match),
    )
    early = [_results(r) for r in before_match if _carries(r, glob)]
    declared.absence(
        bindings,
        "rules-glob-control-never-triggers",
        "rules-glob-reaches-model",
        bool(early),
        settled=any(_results(r) == AFTER_CONTROL for r in turn),
        proof=reached,
        detail="no request before the matching write carried the glob rule, "
        "the one after the control's write included"
        if not early
        else f"the glob rule arrived early, with {early} tool results",
    )

    declared.presence(
        bindings,
        "rules-always-on-at-session-start",
        "rules-always-on-at-session-start",
        _results(first) == 0 and _carries(first, RULES["always"]),
        f"the always-on rule is in the turn's first request "
        f"({_results(first)} tool results)",
    )

    indexed = _results(first) == 0 and _carries(first, RULES["decide_index"])
    declared.presence(
        bindings,
        "rules-model-decision-indexed",
        "rules-model-decision-indexed",
        indexed,
        "the model_decision rule's description is in the turn's first request",
    )
    body = RULES["decide_body"]
    read_back = any(_carries(r, body) for r in turn if _results(r) >= AFTER_READ)
    leaked = [
        _results(r) for r in turn if _results(r) < AFTER_READ and _carries(r, body)
    ]
    declared.absence(
        bindings,
        "rules-model-decision-body-withheld",
        "rules-model-decision-indexed",
        bool(leaked),
        settled=read_back,
        proof=indexed,
        detail="the model_decision rule's body reached the model only once the "
        "agent read its file"
        if not leaked
        else f"the body was in a request with {leaked} tool results, before the read",
    )


def _compacted(world, bindings):
    """A live session: a turn, the harness's compaction, a second turn.
    Returns what the requests carried and how the turns went."""
    driver = copy.copy(bindings)
    driver.final_markers = (STATIC_ANSWER,)
    prov_ip = start_provider(world, "static")
    project = bindings.hook_project
    out = {"asked": False, "turns": []}
    tui = driver.session_in(world, prov_ip, project, prelude(project))
    with tui:
        _, ready = driver.prepare(tui)
        if ready:
            out["turns"].append(
                driver.hook_turn(tui, f"{RULES_TURNS[0]} say hi").detail
            )
            out["asked"] = driver.compact(tui)
            out["turns"].append(
                driver.hook_turn(tui, f"{RULES_TURNS[1]} and then?").detail
            )
    out["requests"] = _model_requests(provider_struct(world))
    with open(os.path.join(world.outdir, "rules-compact.json"), "w") as f:
        json.dump({**out, "requests": _evidence(out["requests"])}, f, indent=1)
    return out


def _evidence(requests):
    """What each model request carried, for the run's evidence."""
    return [
        {
            "tool_results": _results(r),
            "markers": sorted(k for k, v in r.get("rule_markers", {}).items() if v),
            "rule_counts": r.get("rule_counts", {}),
        }
        for r in requests
    ]


def _assert_compacted(bindings, out):
    after = next(
        (r for r in out["requests"] if _carries(r, RULES_TURNS[1])),
        None,
    )
    compacted = after is not None and _carries(after, COMPACTION)
    present = compacted and _carries(after, RULES["always"])
    if after is None:
        detail = (
            f"the turn after the compaction never reached the model: {out['turns']}"
        )
    elif not compacted:
        detail = "the next turn's request carries no compaction summary: " + (
            "the compaction command did not compact"
            if out["asked"]
            else "this harness has no compaction command"
        )
    else:
        detail = "the always-on rule is in the first request after the compaction"
    declared.presence(
        bindings,
        "rules-always-on-after-compaction",
        "rules-always-on-after-compaction",
        present,
        detail,
        measured=after is not None,
    )


def _plugin_turn(world, bindings, expected, declaring):
    """The control's write, then the matching one, in the hook project of a
    machine where one of its two checkouts declares the plugin."""
    project = bindings.hook_project
    calls = [
        _write(bindings, expected, CONTROL),
        _write(bindings, expected, MATCHING),
    ]
    mode, env = bindings.sequence(calls, RULES_TURNS[0])
    prov_ip = start_provider(world, mode, env)
    prompt = f"{RULES_TURNS[0]} make the edits I name"
    output = bindings.project_turn(
        world, prov_ip, plugin_prelude(project, declaring), prompt, project
    )
    requests = _model_requests(provider_struct(world))
    tag = "declared" if declaring else "undeclared"
    with open(os.path.join(world.outdir, f"rules-plugin-{tag}.out"), "w") as f:
        f.write(output)
    with open(
        os.path.join(world.outdir, f"rules-plugin-{tag}-requests.json"), "w"
    ) as f:
        json.dump(_evidence(requests), f, indent=1)
    disks = {root: _read_disk(output, root) for root in (project, OTHER_PROJECT)}
    with open(os.path.join(world.outdir, f"rules-plugin-{tag}-disk.json"), "w") as f:
        json.dump(disks, f, indent=1)
    copies = disks[project]["files"] if disks[project] else "unread"
    return {
        "requests": requests,
        "output": output,
        "copies": copies,
        "disks": disks,
        "declaring": project if declaring else OTHER_PROJECT,
    }


def _assert_disk(project, declared_turn, undeclared_turn):
    """Each world's declaring checkout holds the copy and the manifest entry
    `uze` wrote, and the other checkout holds neither."""
    held, bare = [], []
    for found in (declared_turn, undeclared_turn):
        for root, disk in found["disks"].items():
            if disk is None:
                (held if root == found["declaring"] else bare).append(
                    f"{root}: the turn's output does not carry the prelude's"
                )
                continue
            copied = PLUGIN_COPY in disk["files"]
            registered, entries = _registered(disk)
            line = f"{root}: copy {'present' if copied else 'absent'}, {entries}"
            if root == found["declaring"]:
                held.append((copied and registered, line))
            else:
                bare.append((not copied and not registered, line))
    for name, rows in (
        ("rules-plugin-folder-and-entry-where-declared", held),
        ("rules-plugin-no-folder-nor-entry-where-undeclared", bare),
    ):
        ok = len(rows) == 2 and all(isinstance(r, tuple) and r[0] for r in rows)
        check(name, ok, "; ".join(r[1] if isinstance(r, tuple) else r for r in rows))


def _assert_plugin(bindings, declared_turn, undeclared_turn):
    marker = RULES["plugin_glob"]
    _assert_disk(bindings.hook_project, declared_turn, undeclared_turn)
    turns = {}
    for tag, found in (("declared", declared_turn), ("undeclared", undeclared_turn)):
        turns[tag] = [r for r in found["requests"] if _carries(r, RULES_TURNS[0])]
        check(
            f"rules-plugin-{tag}-turn-reached-model",
            bool(turns[tag]),
            f"{len(turns[tag])} model requests; rules in the checkout: {found['copies']}"
            if turns[tag]
            else f"the turn never reached the model: {found['output'][-200:]}".replace(
                "\n", " "
            ),
        )
    after = [r for r in turns["declared"] if _results(r) >= AFTER_MATCH]
    early = [
        _results(r)
        for r in turns["declared"]
        if _results(r) < AFTER_MATCH and _carries(r, marker)
    ]
    fired = any(_carries(r, marker) for r in after) and not early
    declared.presence(
        bindings,
        "rules-plugin-fires-where-declared",
        "rules-plugin-fires-where-declared",
        fired,
        f"the plugin's glob rule is in a request after the matching write returned, "
        f"and in none before it (early: {early}); copies: {declared_turn['copies']}",
        measured=bool(after),
    )
    twice = [(_results(r), _count(r, marker)) for r in after if _count(r, marker) > 1]
    declared.absence(
        bindings,
        "rules-plugin-reaches-model-once",
        "rules-plugin-fires-where-declared",
        bool(twice),
        settled=bool(after),
        proof=fired,
        detail="every request after the matching write carries the plugin's rule "
        f"once (counts: {[_count(r, marker) for r in after]})"
        if not twice
        else f"the plugin's rule reached the model more than once for one write "
        f"(tool results, count): {twice}",
    )
    leaked = [_results(r) for r in turns["undeclared"] if _carries(r, marker)]
    declared.absence(
        bindings,
        "rules-plugin-never-where-undeclared",
        "rules-plugin-fires-where-declared",
        bool(leaked),
        settled=any(_results(r) >= AFTER_MATCH for r in turns["undeclared"]),
        proof=fired,
        detail="no request of the checkout that does not declare the plugin carried "
        f"its rule, the one after the matching write included; copies there: "
        f"{undeclared_turn['copies']}"
        if not leaked
        else f"the plugin's rule reached a checkout that does not declare it, with "
        f"{leaked} tool results",
    )
