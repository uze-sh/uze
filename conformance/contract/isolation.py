"""What an agent UZE placed must do, on every harness.

UZE isolates at launch: every agent it starts works in a checkout of its
own under `.worktrees/<id>`, and the shared instruction file tells it so.
None of that needs a harness to cooperate — which is exactly why the Lab
has to prove the harness does not get in the way. Three things only a real
harness can answer:

    it starts and works inside a linked worktree, not just a repository root;
    the projected declaration reaches the model from that directory;
    text UZE types into its pane reaches the model as the agent's own turn.

The slot itself is not built by `uze` here: slot mechanics are proven in
the deterministic suite against real Git, and the scene lays the same
shape down by hand so the run measures the harness, never the engine.
"""

import time

from shared.common import check, describe, observed_markers, provider_struct

#: Where the scene's project lives inside the container, and its one slot.
PROJECT = "/work/project"
SLOT = f"{PROJECT}/.worktrees/t0lab"

#: A phrase only the projected declaration carries
#: (`uze-workspace`'s `WorktreePolicy::instructions`).
DECLARATION_MARKER = "already isolated"
#: A sentinel only the typed message carries.
MESSAGE_MARKER = "UZE_CONFORMANCE_REBASE"


#: The workspace's section of `AGENTS.md`, laid down by hand like the slot:
#: the workspace projects it when it opens and places an agent, never the
#: package manager (`worktree-policy`: "The package manager leaves the
#: region alone"), and the deterministic suite holds the projection —
#: `uze-workspace`'s `the_projected_text_never_asks_for_a_top_level_worktree`
#: pins the phrase the check looks for. What only a harness can answer is
#: whether it reads that region from inside the slot.
DECLARATION = f"""<!-- uze:begin project:worktree-policy/lab -->
## Concurrent work isolation

- An agent UZE isolated works in a checkout of its own under `.worktrees/<id>`, on branch `agent/<id>`. If your working directory is inside `.worktrees/`, you are {DECLARATION_MARKER}; do not switch branches.
<!-- uze:end project:worktree-policy/lab -->
"""


def prelude():
    """The shell that lays the scene down before the harness starts: a
    project as a person has it — an `AGENTS.md` carrying the workspace's
    section, and an `agents.yaml` declaring the workspace's policy — made
    ready by `uze install`, which projects whatever bridge each harness
    needs; then one slot to start the harness in. No bridge is written by
    hand: a scene that did measured the Lab's file, not UZE's projection."""
    return f"""
mkdir -p {PROJECT} && cd {PROJECT}
git init -q -b main .
git config user.name lab
git config user.email lab@uze.invalid
cat > AGENTS.md <<'UZE_EOF'
# Lab project

{DECLARATION}UZE_EOF
printf 'workspace:\\n  delivery: merge\\n' > agents.yaml
uze install >/work/isolation-install.log 2>&1 || true
git add -A && git commit -q -m init
git worktree add -q -b agent/t0lab .worktrees/t0lab HEAD
printf '/.worktrees/\\n' >> .git/info/exclude
"""


def assert_contract(cfg, prov_ip, bindings):
    with describe("isolation"):
        _assert_in_slot(cfg, prov_ip, bindings)


def _assert_in_slot(cfg, prov_ip, bindings):
    with bindings.session_in(cfg, prov_ip, SLOT, prelude()) as tui:
        plain, matched = bindings.prepare(tui)
        check(
            "isolation-tui-ready-in-slot",
            bool(matched),
            f"{bindings.harness} reached its prompt inside a linked worktree"
            if matched
            else plain[-160:].replace("\n", " "),
        )
        if not matched:
            return
        time.sleep(bindings.warmup)

        # One turn, typed the way UZE types a notice into a pane: the
        # request it produces has to carry the declaration (context from
        # the slot's checkout) and the sentence itself (input reached the
        # agent's own turn, not a menu or a status line).
        tui.type(
            f"{MESSAGE_MARKER}: a rebase is paused in your checkout; resolve the "
            "conflicts, run git rebase --continue, run the checks, and end your turn"
        )
        tui.submit()
        turn = tui.collect(reads=6)
        tui.snapshot("isolation-turn", turn)

        seen = observed_markers(provider_struct(cfg), "isolation_markers")
        check(
            "isolation-declaration-reaches-model",
            seen.get(DECLARATION_MARKER, False),
            "the projected declaration is in the model request from the slot",
        )
        check(
            "isolation-message-reaches-model",
            seen.get(MESSAGE_MARKER, False),
            "text typed into the pane is the agent's own turn",
        )
        # Deliberately not asked: whether the harness declines to make a
        # worktree of its own. That is the agent's business — UZE never
        # adopts, sweeps or removes a worktree it did not create, which the
        # deterministic suite proves without a harness.
