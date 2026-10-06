"""What a portable hook must do, on every harness.

A package declares a hook once, against portable aliases (`shell`,
`file.write`), events and effects; each integration compiles it into its
harness's own hook form. The hook vertical phases used to each script one
`shell` call through one event, and assert the outcome with markers that
could hold when no hook ran. That is how three harnesses delivered hooks
that never fired while every leg stayed green.

So this contract is a matrix, and every cell is proven by the handler
itself. Each fixture handler (`scripts/probe`) writes a record into the
harness container holding every `HOOK_*` value it received; a cell holds
only when that record exists with the values the alias promises. An
absence — a second handler that must not run, a tool that must not
execute — is read only beside a record proving the first one did.

What is exercised comes from the Lab's own expectation of this harness
(`harnesses/<h>/vocabulary.json`): every alias it binds, every event and
effect UZE claims for it. The deterministic suite holds that expectation
equal to the integration's claims, so a claim with no cell here fails
`cargo test`.

The scenes run in the harness's TUI and are answered the way a person
answers them: every permission or review the harness puts on screen is
accepted through its own keys (`bindings.hook_turn`), never by a flag.
"""

from contract import declared
from shared import common, vocabulary
from shared.common import check, check_absence, declare, describe, provider_struct

#: Where the scripted tools leave their side effects, inside the project.
SIDE = "lab-side"

#: The reason a denying probe relays (`scripts/probe`).
DENIED = "lab-hook-denied:"

#: Carried by a command or path the effect guard must deny.
MARKED = "lab-deny"


def assert_contract(cfg, prov_ip, bindings):
    expected = vocabulary.load_expectation(bindings.harness)
    with describe("hooks"):
        _rows(cfg, bindings, expected)
        effects = set(expected["effects"])
        if "deny" in effects:
            _deny(cfg, bindings, expected)
            _fail_closed(cfg, bindings, expected)
        if "ask" in effects:
            _ask(cfg, bindings, expected)
        if "transform" in effects:
            _transform(cfg, bindings, expected)
        events = set(expected["events"])
        if events - {"pre_tool_use"}:
            _events(cfg, bindings, expected, events)
        if "deny" in effects and "post_tool_use" in events:
            _post_tool_deny(cfg, bindings, expected)
    common.start_provider(cfg, "static")


def records(cfg):
    """{label: [{HOOK_*: value}, ...]} for every handler that ran."""
    found = {}
    for name, content in common.harness_files(cfg).items():
        label = name.split(".", 1)[0]
        values = dict(
            line.split("=", 1) for line in content.splitlines() if "=" in line
        )
        found.setdefault(label, []).append(values)
    return found


def side_effects(cfg, bindings):
    return set(common.harness_files(cfg, f"{bindings.hook_project}/{SIDE}"))


def relayed(cfg, text):
    """Whether any request the harness sent the model carried `text` — a
    denial reason reaching the conversation (`markers.HOOK_DENIALS`)."""
    return bool(common.observed_markers(provider_struct(cfg), "hook_denials").get(text))


#: What every hook scene's person asks. The provider scripts its calls only
#: for a request carrying it, so a subagent's own turn does not restart them.
PROMPT = "run the lab checks"


def _scene(cfg, bindings, tag, plugin, calls, prompt=PROMPT):
    """Runs one turn in which the model makes `calls`, with `plugin`
    installed. Returns (records, side effects, turn) read while the harness
    is still alive."""
    mode, env = bindings.sequence(calls, prompt)
    prov_ip = common.start_provider(cfg, mode, env)
    with bindings.hook_session(cfg, prov_ip, plugin, tag) as tui:
        plain, ready = bindings.prepare(tui)
        check(
            f"hooks-{tag}-ready",
            bool(ready),
            "the session reached its prompt"
            if ready
            else plain[-160:].replace("\n", " "),
        )
        if not ready:
            return {}, set(), None
        turn = bindings.hook_turn(tui, prompt)
        tui.snapshot(f"hooks-{tag}", turn.plain)
        return records(cfg), side_effects(cfg, bindings), turn


def _rows(cfg, bindings, expected):
    """Every bound alias fires its group with the fields it promises."""
    rows = {alias: row for alias, row in expected["aliases"].items() if row is not None}
    # Each row's call, and — for a freeform call whose input is code — the
    # values that code hands the tool (`values`), rendered together so they
    # name the same side file.
    rendered = {
        alias: bindings.call(
            {"call": row["call"], "values": row.get("values", {})},
            side=f"{SIDE}/{alias}",
        )
        for alias, row in rows.items()
    }
    calls = {alias: parts["call"] for alias, parts in rendered.items()}
    with describe("rows"):
        # `flow` beside the rows: a dispatch names an agent the harness
        # has, and some harnesses refuse one that names none before any
        # hook runs.
        found, _, turn = _scene(
            cfg,
            bindings,
            "rows",
            "flow hook-rows",
            [
                step
                for alias, args in calls.items()
                for step in (
                    # A tool the harness defers is loaded first, the way a
                    # model loads it (`load`, measured per harness).
                    *([rows[alias]["load"]] if rows[alias].get("load") else []),
                    {"tool": vocabulary.call_tool(rows[alias]), "args": args},
                )
            ],
        )
        # The census group has no matcher: its records are every call as
        # the hook system named it, which is the hook side of the
        # vocabulary (`shared/vocabulary.py`), measured here and judged
        # where the run's vocabulary is.
        vocabulary.record_hooked(cfg.outdir, vocabulary.census(found.get("census", [])))
        refused = {call["tool"] for call in common.provider_undeclared_calls(cfg)}
        check(
            "hooks-rows-turn-settled",
            turn is not None and turn.settled,
            turn.detail if turn else "the session never reached its prompt",
        )
        for alias, row in rows.items():
            label = "row-" + alias.replace(".", "-")
            seen = found.get(label, [])
            given = rendered[alias]["values"] or calls[alias]
            promised = {
                f"HOOK_{portable.upper()}": str(given[native])
                for portable, native in row.get("fields", {}).items()
            }
            matching = [
                values
                for values in seen
                if values.get("HOOK_TOOL") == alias
                and all(values.get(k) == v for k, v in promised.items())
            ]
            if row.get("unsupported"):
                # The harness has the tool, measured unable to carry this
                # alias: UZE delivers no group for it, and a group that ran
                # anyway with the alias would be the declaration escalating.
                declare(
                    f"hooks-row-{alias}",
                    not any(values.get("HOOK_TOOL") == alias for values in seen),
                    row["unsupported"],
                    f"records: {seen or 'none'}",
                )
                continue
            if row.get("optional") and vocabulary.call_tool(row) in refused:
                declare(
                    f"hooks-row-{alias}",
                    not seen,
                    row["optional"],
                    f"the session did not offer `{vocabulary.call_tool(row)}`",
                )
                continue
            check(
                f"hooks-row-{alias}",
                bool(matching),
                f"the `{alias}` group ran on `{row['tools'][0]}` with {promised}"
                if matching
                else f"records for {label}: {seen or 'none'}",
            )


def _deny(cfg, bindings, expected):
    """First deny wins, and the denied tool never runs; an allowed call
    runs every handler and the tool."""
    shell = expected["aliases"]["shell"]
    denied = bindings.call(shell["call"], mark=MARKED, side=f"{SIDE}/denied")
    allowed = bindings.call(shell["call"], side=f"{SIDE}/allowed")
    calls = [
        {"tool": vocabulary.call_tool(shell), "args": a} for a in (denied, allowed)
    ]
    with describe("deny"):
        found, side, turn = _scene(cfg, bindings, "deny", "hook-effects", calls)
        settled = turn is not None and turn.settled
        guard = found.get("effect-guard", [])
        guard_denied = [r for r in guard if MARKED in r.get("HOOK_COMMAND", "")]
        guard_allowed = [r for r in guard if MARKED not in r.get("HOOK_COMMAND", "")]
        reason = relayed(cfg, f"{DENIED}effect-guard")
        check(
            "hooks-deny-guard-ran",
            bool(guard_denied),
            "the guard ran on the marked command"
            if guard_denied
            else f"records: {guard}",
        )
        check(
            "hooks-deny-reason-relayed",
            reason,
            "the denial reached the conversation"
            if reason
            else "no request carried it",
        )
        check_absence(
            "hooks-deny-tool-blocked",
            "denied" not in side,
            settled,
            proof=bool(guard_denied) and reason,
            detail="the denied command never ran" if "denied" not in side else "it ran",
        )
        # A denial is a decision, and a harness that has a way to show one
        # must be answered in it; a denial rendered as a failed hook tells
        # the person their guard is broken when it worked.
        shown_as_error = turn is not None and bindings.hook_error(turn.plain)
        declared.absence(
            bindings,
            "hooks-deny-not-reported-as-error",
            "hooks.deny-rendered-as-decision",
            shown_as_error,
            settled=settled,
            proof=bool(guard_denied) and reason,
            detail="the harness showed the denial as an error"
            if shown_as_error
            else "the harness showed the denial as a decision",
        )
        after = found.get("effect-after", [])
        check_absence(
            "hooks-deny-first-deny-wins",
            not any(MARKED in r.get("HOOK_COMMAND", "") for r in after),
            settled,
            proof=bool(guard_denied),
            detail=f"handlers after the denial: {after}",
        )
        check(
            "hooks-allow-every-handler-ran",
            bool(guard_allowed)
            and any(MARKED not in r.get("HOOK_COMMAND", "") for r in after),
            f"guard {len(guard_allowed)}, after {len(after)}",
        )
        check(
            "hooks-allow-tool-ran",
            "allowed" in side,
            "the allowed command left its file"
            if "allowed" in side
            else f"side: {sorted(side)}",
        )


def _fail_closed(cfg, bindings, expected):
    """A guard that cannot run denies: a deny group whose handler is
    missing must block the call, saying why, rather than let it through."""
    shell = expected["aliases"]["shell"]
    call = bindings.call(shell["call"], side=f"{SIDE}/fail-closed")
    with describe("fail-closed"):
        _, side, turn = _scene(
            cfg,
            bindings,
            "fail-closed",
            "hook-fail-plugin",
            [{"tool": vocabulary.call_tool(shell), "args": call}],
        )
        reason = relayed(cfg, "handler failed (exit")
        check(
            "hooks-fail-closed-reason-relayed",
            reason,
            "the guard's failure reached the conversation as the denial"
            if reason
            else "no request carried the failure",
        )
        check_absence(
            "hooks-fail-closed-tool-blocked",
            "fail-closed" not in side,
            turn is not None and turn.settled,
            proof=reason,
            detail="the call never ran while its guard could not"
            if "fail-closed" not in side
            else "it ran",
        )


def _ask(cfg, bindings, expected):
    """An `ask` group puts the harness's own approval in front of the
    person, with the handler's reason; approving runs the tool."""
    shell = expected["aliases"]["shell"]
    asked = bindings.call(shell["call"], mark=MARKED, side=f"{SIDE}/asked")
    with describe("ask"):
        found, side, turn = _scene(
            cfg,
            bindings,
            "ask",
            "hook-ask",
            [{"tool": vocabulary.call_tool(shell), "args": asked}],
        )
        ran = bool(found.get("effect-ask"))
        prompted = turn is not None and any(
            DENIED in common.squash(approval) for approval in turn.approvals
        )
        check("hooks-ask-guard-ran", ran, f"records: {found.get('effect-ask')}")
        declared.presence(
            bindings,
            "hooks-ask-prompted-with-reason",
            "hooks.ask-shows-reason",
            prompted,
            "the harness asked, naming the handler's reason"
            if prompted
            else f"approvals seen: {turn.approvals if turn else 'none'}",
        )
        check(
            "hooks-ask-approved-tool-ran",
            "asked" in side,
            "approving it ran the command"
            if "asked" in side
            else f"side: {sorted(side)}",
        )


def _transform(cfg, bindings, expected):
    """A `transform` group rewrites the call before it runs: the tool does
    what the rewrite says and never what the model asked for. Read from the
    disk, where only the command that actually ran can leave its file."""
    shell = expected["aliases"]["shell"]
    asked = bindings.call(shell["call"], side=f"{SIDE}/from")
    with describe("transform"):
        found, side, turn = _scene(
            cfg,
            bindings,
            "transform",
            "hook-transform",
            [{"tool": vocabulary.call_tool(shell), "args": asked}],
        )
        ran = bool(found.get("effect-transform"))
        check(
            "hooks-transform-guard-ran",
            ran,
            f"records: {found.get('effect-transform')}",
        )
        check_absence(
            "hooks-transform-original-not-run",
            "from" not in side,
            turn is not None and turn.settled,
            proof=ran and "to" in side,
            detail="the command the model asked for never ran"
            if "from" not in side
            else "it ran as asked",
        )
        check(
            "hooks-transform-rewrite-ran",
            "to" in side,
            "the rewritten command left its file"
            if "to" in side
            else f"side: {sorted(side)}",
        )


def _events(cfg, bindings, expected, events):
    """Each event UZE claims fires its group, saying which event it is."""
    shell = expected["aliases"]["shell"]
    calls = [
        {
            "tool": vocabulary.call_tool(shell),
            "args": bindings.call(shell["call"], side=f"{SIDE}/event"),
        }
    ]
    with describe("events"):
        found, _, _ = _scene(cfg, bindings, "events", "hook-events", calls)
        for event, label in (
            ("session_start", "event-session"),
            ("post_tool_use", "event-post"),
            ("stop", "event-stop"),
        ):
            if event not in events:
                continue
            seen = [r for r in found.get(label, []) if r.get("HOOK_EVENT") == event]
            check(
                f"hooks-event-{event}",
                bool(seen),
                f"`{label}` ran as {event}"
                if seen
                else f"records: {found.get(label) or 'none'}",
            )


def _post_tool_deny(cfg, bindings, expected):
    """A deny after the tool ran cannot undo it; what it promises is that
    the reason reaches the model, in the harness's own dialect — a decision
    the harness reads as an error is a hook that failed, not one that
    denied."""
    shell = expected["aliases"]["shell"]
    marked = bindings.call(shell["call"], mark=MARKED, side=f"{SIDE}/post")
    with describe("post-tool-deny"):
        found, _, turn = _scene(
            cfg,
            bindings,
            "post-deny",
            "hook-post-deny",
            [{"tool": vocabulary.call_tool(shell), "args": marked}],
        )
        ran = [
            r
            for r in found.get("post-guard", [])
            if r.get("HOOK_EVENT") == "post_tool_use"
        ]
        reason = relayed(cfg, f"{DENIED}post-guard")
        check(
            "hooks-post-deny-guard-ran",
            bool(ran),
            f"records: {found.get('post-guard')}",
        )
        check(
            "hooks-post-deny-reason-relayed",
            reason,
            "the reason reached the conversation",
        )
        check_absence(
            "hooks-post-deny-no-hook-error",
            turn is not None and not bindings.hook_error(turn.plain),
            turn is not None and turn.settled,
            proof=bool(ran),
            detail="the harness reported no hook error",
        )
