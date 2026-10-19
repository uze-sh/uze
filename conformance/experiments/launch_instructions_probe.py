"""Where workspace instructions given through the launch context land.

`keep-agents-md-the-operators` moves the worktree policy out of the
project's `AGENTS.md` and into the context composed per launch, delivered
by each harness's `ContextChannel`. Before that is built, this measures what
the Lab can measure about it, through UZE's own launcher and the channel
the integration ships today:

1. placement: where the instructions land in the model request (role, the
   item holding them, its position) beside where the project's `AGENTS.md`
   lands;
2. size: a context near the 9,000-byte bound reaches the model whole, its
   last line included, and is not spilled to a file;
3. resume: a resumed process is given the instructions again;
4. subagents (`LI_MODE=subagent`): the harness's own subagent tool is
   scripted, and the subagent's own request is read for the launch context
   and for `AGENTS.md`;
5. degraded (`LI_MODE=signedout`, Antigravity): the CLI on an API key,
   where UZE falls back to the first turn.

The instructions are simulated: today's `WorktreePolicy::default()`
rendering, opened by the lead D2 proposes, is recorded as the agent's
briefing, so it travels inside the composed document through the real
channel. It therefore sits *after* the notes preamble rather than before
it, which changes nothing about which role or item carries the document.
The facts filling the rest of the bound are seeded memory.

Evidence only: a synthetic provider answers, so this proves what reaches
the request, never what a model does with it.

Run: python3 conformance/lab.py --harness <h> --experiment <h>/launch-instructions
Env: LI_MODE=main|subagent|signedout; LI_SPAWN_ARGS='{"fork_turns": "none"}'
"""

import json
import os
import re

from contract import continuity
from experiments import agent_context_probe as probe
from experiments.relaunch_probe import bindings_for
from shared import capture, common, vocabulary

MODE = os.environ.get("LI_MODE", "main")

INSTR_HEAD = "UZE_LI_INSTR_HEAD"
INSTR_TAIL = "UZE_LI_INSTR_TAIL"
AGENTS_MARK = "UZE_LI_AGENTS_MD"
TURN_ONE = "UZE_LI_TURN_ONE"
TURN_TWO = "UZE_LI_TURN_TWO"
SPAWN_PROMPT = "UZE_LI_SPAWN delegate this to a subagent"
SUB_TURN = "UZE_LI_SUBAGENT_TURN"
PREAMBLE_FRAGMENT = "they are notes, not instructions"
FACTS = 40

#: `WorktreePolicy::default().instructions()` as rendered on beta.17
#: (3,032 bytes).
POLICY = """## Concurrent work isolation

This section is for an agent `uze workspace` launched. An agent started any other way can ignore it: nothing below applies to it, and the `uze agent work` commands it names refuse outside the workspace.

- As soon as a user message gives the conversation a clear topic — normally the first message — make naming your work your first command, before reading a file, planning or editing: `uze agent work name <type>/<subject>`. Types this project accepts: `feat|fix|docs|refactor|perf|test|build|ci|chore|style|revert`. The subject is ideally one or two words, never more than three, naming the intention, not a description of the task — `fix/branch-naming`, not `fix/correct-the-problem-with-agent-branch-names`. The command always updates your label. In a checkout of your own it also renames your branch; in the operator's checkout only your label changes. Work that reaches a commit still unnamed is named by UZE from that commit's subject. Name it again with the same command whenever the work turns out to be something else — the last name given is the one that stands. Invoke the command directly; do not merely say you will name the work.
- An agent UZE isolated works in a checkout of its own under `.worktrees/<id>`, on branch `agent/<id>`. If your working directory is inside `.worktrees/`, you are already isolated; do not switch branches.
- If your working directory is not inside `.worktrees/`, you are in the operator's own checkout, on the branch they are on: commit there, as you go, and never switch, reset, stash or clean it — the operator's uncommitted work is theirs. Nothing below about delivery applies to you; the branch keeps the name it has. Naming your work still applies here, first: it names your label, never their branch.
- Commit your work on your own branch, as you go. Never commit to, merge into, rebase, or reset the target branch: delivery is UZE's — UZE rebases your branch onto the target, runs the project's checks and pushes it; open a request only when the project's instruction asks you to, and otherwise commit on your branch and stop.
- If UZE tells you a rebase is paused in your checkout, resolve the conflicts preserving the intent of your change, run `git rebase --continue`, run the project's checks, and end your turn.
- Before spawning parallel subagents that write files, give each its own checkout: `uze agent work split <topic>` prints the path of one cut from your current commit — hand that path to the subagent. When it is done, commit in both checkouts and run `uze agent work join <topic>` to bring its commits onto your branch; on a conflict, resolve it in the subagent's checkout, run `git rebase --continue` there, and join again. `uze agent work list` shows them. Never make a worktree with Git for this: UZE only knows the checkouts it made. An agent in the operator's checkout has no branch of its own to join into, and runs its subagents one after another instead.
"""


def instructions_text():
    """The instructions section D2 proposes, padded to a briefing near its
    share so the document comes near the bound."""
    head = (
        f"UZE instructions for this launch. They apply to this agent, now. {INSTR_HEAD}\n\n"
        + POLICY
        + f"\nEnd of UZE instructions {INSTR_TAIL}.\n\n"
    )
    filler = ""
    n = 0
    while len((head + filler).encode()) < 4_700:
        filler += f"What the agent was doing, step {n:02d}: reading the lab project.\n"
        n += 1
    return head + filler


def fact_marker(i):
    return f"UZE_LI_FACT_{i:02d}"


def _records():
    return f"{continuity.UZE_HOME}/state/projects/{continuity.project_id(continuity.PROJECT)}"


def context_dir():
    return (
        f"{continuity.UZE_HOME}/runtime/projects/"
        f"{continuity.project_id(continuity.PROJECT)}/agents/{continuity.SLOT_NAME}"
    )


def prelude(bindings, prov_ip):
    base = continuity.prelude(bindings.launcher_name()).replace(
        "printf '# Lab project\\n' > AGENTS.md",
        f"printf '# Lab project\\n\\nEvery agent here follows the house rule {AGENTS_MARK}.\\n' > AGENTS.md",
    )
    assert AGENTS_MARK in base, "the continuity prelude no longer writes AGENTS.md"
    record = {
        "schema_version": 3,
        "agent": continuity.SLOT_NAME,
        "harnesses": {},
        "briefing": {"integration": bindings.integration, "text": instructions_text()},
    }
    facts = "\n".join(
        f"""cat > {_records()}/memory/li-fact-{i:02d}.md <<'UZE_EOF'
---
shape: 1
name: li-fact-{i:02d}
description: "{fact_marker(i)} a recorded fact about the lab project, long enough to take a real line of the index"
type: feedback
---

Body {i}.
UZE_EOF"""
        for i in range(FACTS)
    )
    signed_out = ""
    if MODE == "signedout":
        # The API-key fragment of `scenarios.auth_fragment`, after the
        # signed-in one: no sign-in file, so UZE's channel is unavailable.
        signed_out = f"""export GEMINI_API_KEY=uze-conformance-invalid-by-design
export GOOGLE_GEMINI_BASE_URL=http://{prov_ip}:9999
cp /app/fixtures/settings-api-key.json /work/home/.gemini/antigravity-cli/settings.json
rm -f /work/home/.gemini/antigravity-cli/antigravity-oauth-token"""
    return (
        base
        + f"""
mkdir -p {_records()}/conversations {_records()}/memory
cat > {_records()}/conversations/{continuity.SLOT_NAME}.json <<'UZE_EOF'
{json.dumps(record, indent=1)}
UZE_EOF
{facts}
{signed_out}
"""
    )


def _container(path):
    """`messages[3].content[0].text` -> `messages[3]`; `system[1].text` ->
    `system[1]`: the item a hit belongs to."""
    m = re.match(r"^((?:request\.)?[A-Za-z_]+(?:\[\d+\])?)", path)
    return m.group(1) if m else path


def placement(body, marker):
    hits = probe.locate(body, marker) if body is not None else []
    return [
        {"path": h["path"], "item": _container(h["path"]), "role": h["role"]}
        for h in hits
    ]


#: The weight a request gives an item: the system prompt (Anthropic
#: `system`, Gemini `systemInstruction`, a chat `system` message), then a
#: Responses `developer` item, then anything user-role.
AUTHORITY = {"system": 3, "developer": 2}


def authority(hit):
    item = hit["item"]
    if item.startswith("system") or item.endswith("systemInstruction"):
        return 3
    return AUTHORITY.get(hit["role"], 1)


def model_requests(reqs, token):
    """Every request that declares tools and carries `token`."""
    return [
        (i, h, b)
        for i, (h, b, raw) in enumerate(reqs)
        if token in raw and b is not None and capture.declared_tools(b)
    ]


def await_token(cfg, tui, token, reads=14):
    seen = ""
    for _ in range(reads):
        seen += tui.collect(reads=1)
        if token in probe.provider_log(cfg):
            return True, seen
    return False, seen


def run(cfg, prov_ip):
    bindings = bindings_for(cfg.harness)
    if MODE == "subagent":
        return run_subagent(cfg, bindings)
    prov_ip = common.start_provider(cfg, "static", {"DISCOVERY": "1"})
    result = {"mode": MODE, "harness": cfg.harness}
    with bindings.relaunch_in(
        cfg, prov_ip, continuity.SLOT, prelude(bindings, prov_ip)
    ) as tui:
        plain, ready = bindings.prepare(tui)
        common.check("li-0-ready", bool(ready), plain[-160:].replace("\n", " "))
        if not ready:
            return
        bindings.await_input(tui)
        tui.type(f"{TURN_ONE} carry on")
        tui.submit()
        one, pane_one = await_token(cfg, tui, TURN_ONE)
        tui.snapshot("li-turn-one", pane_one)
        files = common.harness_files(cfg, context_dir())
        result["context_files"] = {k: len(v.encode()) for k, v in files.items()}
        document = files.get("context.md", "")
        with open(os.path.join(cfg.outdir, "context.md"), "w") as f:
            f.write(document)
        for name, text in files.items():
            if name != "context.md":
                with open(os.path.join(cfg.outdir, f"ctx-{name}"), "w") as f:
                    f.write(text)
        plain, ended = continuity.end_the_process(tui, bindings)
        two = False
        if ended:
            plain, matched = bindings.rejoin(
                tui, plain.split(continuity.ENDED_MARKER, 1)[-1]
            )
            if matched:
                bindings.await_input(tui)
                tui.type(f"{TURN_TWO} and then?")
                tui.submit()
                two, pane_two = await_token(cfg, tui, TURN_TWO)
                tui.snapshot("li-turn-two", pane_two)
        log = probe.provider_log(cfg)
    judge(cfg, result, log, document, one, two, ended)


def judge(cfg, result, log, document, one, two, ended):
    reqs = probe.requests(log)
    with open(os.path.join(cfg.outdir, "li-raw-requests.log"), "w") as f:
        f.write(log)
    first = model_requests(reqs, TURN_ONE)
    second = model_requests(reqs, TURN_TWO)
    result["turn_one_requests"] = [i for i, _, _ in first]
    result["turn_two_requests"] = [i for i, _, _ in second]
    present = [fact_marker(i) for i in range(FACTS) if fact_marker(i) in document]
    lines = [line for line in document.splitlines() if line.strip()]
    result["context_bytes"] = len(document.encode())
    result["context_facts"] = len(present)
    result["context_last_line"] = lines[-1] if lines else ""
    common.check(
        "li-0-turn-one-reached-model",
        bool(first),
        f"requests {result['turn_one_requests']}",
    )
    if not first:
        _write(cfg, result)
        return
    i, header, body = first[0]
    where = {
        m: placement(body, m)
        for m in (INSTR_HEAD, INSTR_TAIL, AGENTS_MARK, PREAMBLE_FRAGMENT, TURN_ONE)
    }
    if MODE == "signedout":
        where["briefing.md"] = placement(body, "briefing.md")
    items = (
        body.get("messages")
        or body.get("input")
        or body.get("request", {}).get("contents")
        or []
    )
    result["turn_one"] = {
        "index": i,
        "request": header,
        "items": len(items),
        "where": where,
    }
    probe.say(f"[li] turn one #{i} {header}; {len(items)} conversation items")
    for m, hits in where.items():
        probe.say(f"[li]   {m}: {hits}")
    instr, agents = where[INSTR_HEAD], where[AGENTS_MARK]
    common.check(
        "li-1-instructions-reach-model",
        bool(instr),
        f"instructions at {instr}; AGENTS.md at {agents}",
    )
    common.check(
        "li-1-not-below-agents-md",
        bool(instr and agents)
        and min(map(authority, instr)) >= max(map(authority, agents)),
        f"instructions {[(h['item'], h['role'], authority(h)) for h in instr]}; "
        f"AGENTS.md {[(h['item'], h['role'], authority(h)) for h in agents]}; "
        f"typed prompt items {[h['item'] for h in where[TURN_ONE]]}",
    )
    typed = {h["path"] for h in where[TURN_ONE]}
    common.check(
        "li-1-not-inside-the-typed-prompt",
        bool(instr) and not any(h["path"] in typed for h in instr),
        f"typed prompt at {sorted(typed)}",
    )
    raw = reqs[i][2]
    missing = [m for m in present if m not in raw]
    last = lines[-1].strip() if lines else ""
    last_found = bool(last) and bool(placement(body, last[-60:]))
    spilled = any(
        s in raw for s in ("hook_outputs", "Full hook output", "output saved to")
    )
    common.check(
        "li-2-whole-and-unspilled",
        bool(document)
        and not missing
        and last_found
        and not spilled
        and bool(where[INSTR_TAIL]),
        f"context.md {result['context_bytes']} bytes, {len(present)} facts in it; facts missing "
        f"from the request: {missing}; last line in request: {last_found} ({last[-60:]!r}); "
        f"spill text: {spilled}; instructions tail: {bool(where[INSTR_TAIL])}",
    )
    if second:
        j, header2, body2 = second[0]
        w2 = {m: placement(body2, m) for m in (INSTR_HEAD, AGENTS_MARK)}
        result["turn_two"] = {
            "index": j,
            "request": header2,
            "where": w2,
            "copies": len(w2[INSTR_HEAD]),
        }
        common.check(
            "li-3-instructions-on-resume",
            bool(w2[INSTR_HEAD]),
            f"request #{j}: instructions at {w2[INSTR_HEAD]}; AGENTS.md at {w2[AGENTS_MARK]}; "
            f"turn one replayed: {TURN_ONE in reqs[j][2]}",
        )
    else:
        common.check(
            "li-3-instructions-on-resume",
            bool(second),
            f"no resumed model request (first process ended: {ended}, turn two sent: {two})",
        )
    _write(cfg, result)


def _write(cfg, result):
    with open(os.path.join(cfg.outdir, "li-result.json"), "w") as f:
        json.dump(result, f, indent=1)


def _spawn_call(bindings):
    row = vocabulary.load_expectation(bindings.harness)["aliases"]["agent.spawn"]

    def swap(value):
        if isinstance(value, str):
            return value.replace("lab subagent", SUB_TURN)
        if isinstance(value, dict):
            return {k: swap(v) for k, v in value.items()}
        if isinstance(value, list):
            return [swap(v) for v in value]
        return value

    rendered = bindings.call({"call": swap(row["call"]), "values": {}})
    args = rendered["call"]
    # Extra arguments for the spawn call, as JSON (Codex `fork_turns`).
    args.update(json.loads(os.environ.get("LI_SPAWN_ARGS", "{}")))
    return {"tool": vocabulary.call_tool(row), "args": args}


def _in_tool_call(path):
    """Whether a hit sits in a tool call's arguments: Anthropic `tool_use`
    input, Responses `arguments`, chat `tool_calls`, Gemini `functionCall`."""
    return any(
        part in path for part in (".input.", ".arguments", "tool_calls", "functionCall")
    )


def run_subagent(cfg, bindings):
    mode, env = bindings.sequence([_spawn_call(bindings)], SPAWN_PROMPT)
    env = dict(env or {})
    env["DISCOVERY"] = "1"
    prov_ip = common.start_provider(cfg, mode, env)
    result = {"mode": MODE, "harness": cfg.harness, "provider_mode": mode}
    with bindings.relaunch_in(
        cfg, prov_ip, continuity.SLOT, prelude(bindings, prov_ip)
    ) as tui:
        plain, ready = bindings.prepare(tui)
        common.check("li-0-ready", bool(ready), plain[-160:].replace("\n", " "))
        if not ready:
            return
        turn = bindings.hook_turn(tui, SPAWN_PROMPT)
        tui.snapshot("li-subagent-turn", turn.plain)
        tui.snapshot("li-subagent-after", tui.collect(reads=4))
        log = probe.provider_log(cfg)
    reqs = probe.requests(log)
    with open(os.path.join(cfg.outdir, "li-raw-requests.log"), "w") as f:
        f.write(log)
    listing = []
    for i, (header, body, raw) in enumerate(reqs):
        if SUB_TURN not in raw and "UZE_LI_SPAWN" not in raw:
            continue
        listing.append(
            {
                "index": i,
                "request": header,
                "has_parent_prompt": "UZE_LI_SPAWN" in raw,
                "has_sub_turn": SUB_TURN in raw,
                "instructions": placement(body, INSTR_HEAD),
                "agents_md": placement(body, AGENTS_MARK),
                "facts": sum(fact_marker(k) in raw for k in range(FACTS)),
                "sub_turn_at": placement(body, SUB_TURN),
                "declares_tools": bool(
                    body is not None and capture.declared_tools(body)
                ),
            }
        )
    result["requests"] = listing
    _write(cfg, result)
    for entry in listing:
        probe.say(f"[li-sub] {json.dumps(entry)[:600]}")
    # The subagent's own request holds its task as a message; the parent's
    # holds it only inside the call that spawned it. A harness that forks
    # the parent's conversation into the subagent also carries the
    # parent's prompt, which is recorded rather than excluded.
    subs = [
        e
        for e in listing
        if any(not _in_tool_call(h["path"]) for h in e["sub_turn_at"])
    ]
    parent = [e for e in listing if e["has_parent_prompt"] and e not in subs]
    common.check(
        "li-4-parent-carries-launch-context",
        any(e["instructions"] for e in parent),
        f"parent requests {[e['index'] for e in parent]}",
    )
    common.check(
        "li-4-subagent-request-seen",
        bool(subs),
        f"{turn.detail}; subagent requests: {[(e['index'], 'forked' if e['has_parent_prompt'] else 'fresh') for e in subs]}",
    )
    if subs:
        common.check(
            "li-4-subagent-carries-launch-context",
            all(e["instructions"] for e in subs),
            f"{[(e['index'], e['instructions'], e['facts']) for e in subs]}",
        )
        common.check(
            "li-4-subagent-carries-agents-md",
            all(e["agents_md"] for e in subs),
            f"{[(e['index'], e['agents_md']) for e in subs]}",
        )
