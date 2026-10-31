"""Where a hook says a call runs: the workspace, and a shell command's own
directory.

`HOOK_CWD` is the session's workspace and `HOOK_WORKDIR` the directory a
`shell` command runs in. The second is the call's to choose (a `workdir`,
a `cd` in a shell that keeps it); the first never is. One turn in the hook
project, with `hook-rows` (every call, before it runs) and `hook-events`
(after a `shell` call, a session start, a stop) installed, runs a shell
command in each directory the harness's command tool can name and after a
`cd`; each command writes the directory it started in. Every record must
name the project as its workspace, and every shell call the directory its
command started in.

What a harness's command tool cannot express is declared, measured on the
same turn: a working directory its tool does not declare
(`hooks.shell-workdir-parameter`), and a post-tool directory reported
after the command moved its shell (`hooks.post-workdir-is-start`).
"""

from contract import declared
from shared import common
from shared.common import check, describe

ELSEWHERE = "/work/elsewhere"

#: Where the scripted commands write the directory they started in.
SIDE = "lab-side"

#: Each call naming a working directory, scripted where the harness's
#: command tool can carry one.
NAMED = (
    ("sub-abs", "{project}/sub"),
    ("sub-rel", "sub"),
    ("other", ELSEWHERE),
)

#: A `cd` in each direction, each followed by a call that names nothing.
MOVES = (("cd-sub", "{project}/sub"), ("cd-other", ELSEWHERE))


def _plan(bindings, project):
    out = f"{project}/{SIDE}"

    def started(tag, then=""):
        line = f"mkdir -p {out} && pwd -P > {out}/{tag}.pwd"
        return f"{line}{' && ' + then if then else ''} # case-{tag}"

    plan = [("none", bindings.shell_in(started("none")))]
    named = []
    for tag, workdir in NAMED:
        call = bindings.shell_in(started(tag), workdir.format(project=project))
        if call is not None:
            plan.append((tag, call))
            named.append(tag)
    for tag, target in MOVES:
        target = target.format(project=project)
        plan.append((tag, bindings.shell_in(started(tag, f"cd {target}"))))
        plan.append((f"after-{tag}", bindings.shell_in(started(f"after-{tag}"))))
    return plan, named


def _case(record, tags):
    text = record.get("HOOK_COMMAND", "") + record.get("HOOK_INPUT", "")
    for tag in sorted(tags, key=len, reverse=True):
        if f"case-{tag}" in text:
            return tag
    return None


def _values(records, key):
    return sorted({str(r.get(key, "")) for r in records})


def assert_contract(cfg, bindings, expected, scene):
    """`scene` is the hooks contract's own scene runner."""
    project = bindings.hook_project
    plan, named = _plan(bindings, project)
    tags = [tag for tag, _ in plan]
    with describe("directories"):
        found, _, turn, started = scene(
            cfg,
            bindings,
            "directories",
            "hook-rows hook-events",
            [call for _, call in plan],
            before=f"mkdir -p {project}/sub {ELSEWHERE}",
            # Read while the harness is alive: its container goes with it.
            probe=lambda cfg: {
                name.split(".", 1)[0]: body.strip()
                for name, body in common.harness_files(cfg, f"{project}/{SIDE}").items()
                if name.endswith(".pwd")
            },
        )
        started = started or {}
        settled = turn is not None and turn.settled
        check(
            "hooks-directories-turn-settled",
            settled,
            turn.detail if turn else "the session never reached its prompt",
        )
        before, after = {}, {}
        for record in found.get("census", []):
            tag = _case(record, tags)
            if tag and record.get("HOOK_TOOL") == "shell":
                before.setdefault(tag, []).append(record)
        for record in found.get("event-post", []):
            tag = _case(record, tags)
            if tag:
                after.setdefault(tag, []).append(record)
        posts = "post_tool_use" in expected["events"]

        def told(records, tag):
            return (
                bool(records)
                and _values(records, "HOOK_CWD") == [project]
                and started.get(tag) is not None
                and _values(records, "HOOK_WORKDIR") == [started[tag]]
            )

        def said(records, tag):
            return (
                f"started in {started.get(tag)}, HOOK_CWD {_values(records, 'HOOK_CWD')}, "
                f"HOOK_WORKDIR {_values(records, 'HOOK_WORKDIR')}"
            )

        for tag, _ in plan:
            if tag in named:
                continue
            check(
                f"hooks-directories-{tag}",
                told(before.get(tag, []), tag),
                said(before.get(tag, []), tag),
            )
            if posts and not tag.startswith("cd-"):
                check(
                    f"hooks-directories-post-{tag}",
                    told(after.get(tag, []), tag),
                    said(after.get(tag, []), tag),
                )

        # A call naming its own directory: where the command tool declares
        # one, the handler is told it; where it does not, the harness
        # refuses the call before any hook runs.
        if named:
            declared.presence(
                bindings,
                "hooks-directories-named",
                "hooks.shell-workdir-parameter",
                all(told(before.get(tag, []), tag) for tag in named),
                "; ".join(f"{tag}: {said(before.get(tag, []), tag)}" for tag in named),
                measured=settled,
            )

        # After a command that moved its shell, the directory reported is
        # the one it started in, as for every other call.
        if posts:
            moved = [tag for tag, _ in MOVES]
            declared.presence(
                bindings,
                "hooks-directories-post-is-start",
                "hooks.post-workdir-is-start",
                all(told(after.get(tag, []), tag) for tag in moved),
                "; ".join(f"{tag}: {said(after.get(tag, []), tag)}" for tag in moved),
                measured=settled and all(after.get(tag) for tag in moved),
            )

        # A session start and a stop carry no call: the workspace, and no
        # command's directory.
        for event, label in (
            ("session_start", "event-session"),
            ("stop", "event-stop"),
        ):
            if event not in expected["events"]:
                continue
            seen = found.get(label, [])
            check(
                f"hooks-directories-{event.replace('_', '-')}",
                bool(seen)
                and _values(seen, "HOOK_CWD") == [project]
                and _values(seen, "HOOK_WORKDIR") == [""],
                f"HOOK_CWD {_values(seen, 'HOOK_CWD')}, HOOK_WORKDIR {_values(seen, 'HOOK_WORKDIR')}",
            )
