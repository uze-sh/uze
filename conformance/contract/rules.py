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
            },
        )
        _assert_turn(bindings, *found["rules-turn"])
        _assert_compacted(bindings, found["rules-compact"])
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
            seen.append(
                {
                    **request,
                    "rule_markers": markers,
                    "tool_results": _results(before) + _results(request),
                }
            )
        elif request.get("offers_tools"):
            seen.append(request)
    return seen


def _carries(request, marker):
    return bool(request.get("rule_markers", {}).get(marker))


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
