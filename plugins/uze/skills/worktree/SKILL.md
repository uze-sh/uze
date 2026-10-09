---
name: worktree
description: For an agent `uze workspace` launched, and no other. Works inside the checkout the workspace placed you in — knowing where you are, committing on your own branch, giving parallel subagents checkouts of their own, and handing your work to UZE's delivery instead of integrating it yourself. Use when coordinating more than one writing agent, when resuming work in an existing checkout, when UZE reports a paused rebase or failed checks on your branch, or when a conflict or an unknown checkout owner needs resolving.
---

# UZE — working where UZE placed you

**This applies only to an agent `uze workspace` launched.** An agent a
person started by hand, in a terminal, an editor or CI, was placed by
nobody: none of what follows applies to it, and the `uze agent work`
commands it names refuse with "not an agent UZE launched". If you are not
sure which you are, `uze agent work list` answers: it lists your subagents'
checkouts, or refuses. On a refusal, stop reading here.

You do not decide where to work: UZE places every agent it launches before
you start. Either you were **isolated** — a checkout of your own under
`.worktrees/<id>`, on branch `agent/<id>`, with the primary checkout left
to the operator — or you are in the operator's own checkout, on the branch
they are on, beside them. The project's `workspace.worktree` (`always` or
`manual`) decides which you got, and the operator can move you to a
worktree afterwards (**To worktree**), in which case you
are relaunched in the new checkout — with whatever their tree had
uncommitted, if they said to carry it, and possibly with a conversation
that starts over, because a harness that files a conversation under the
directory it ran in cannot resume it elsewhere.
When the project declares a policy, the "Concurrent work isolation"
section of `AGENTS.md` states the layout and what happens to finished
work; the workspace keeps it in step with `agents.yaml`.

This skill is what no harness does for you: the part of that arrangement
you have to carry yourself.

## Know where you are

```bash
git rev-parse --show-toplevel
git branch --show-current
git status --short
```

If your working directory is inside `.worktrees/`, you are isolated: work
here, commit here, and do not switch branches. A worktree you make for
yourself is yours — UZE neither sees nor delivers it, so bring its work back
onto your own branch.

If your working directory is not inside `.worktrees/`, you are in the
operator's own checkout, on their branch. Commit there, as you go, and
never switch, reset, stash, clean or move it: the operator's uncommitted
work is theirs, and so is the branch's name. Nothing below about delivery
and rebases applies to you — there is nothing to deliver, because your
commits already land where the operator is. Naming still does: in the
operator's checkout it changes only your label, never their branch. Ask again after the
operator isolates you: the answer changes, and your own working directory
is where it is written.

## Name the work before you do it

Your first action in the checkout, before you read a file or plan anything:

```bash
uze agent work name <type>/<subject>
```

The branch UZE placed you on is a generated identifier, and a reviewer
meeting it learns nothing. The subject is one or two words naming the
intention — `fix/branch-naming`, not a description of the task — and the
types the project accepts are spelled out in the "Concurrent work
isolation" section of `AGENTS.md`. A project that declares none refuses the
command, which is that project saying it does not name work; carry on.

Do it now rather than later: the request you were given is where the
intention comes from, so nothing you read afterwards makes the name easier
to choose, and work that reaches a commit unnamed is named by UZE from that
commit's subject instead. Naming renames your branch, so ask Git for its
name rather than remembering it. Name it again with the same command
whenever the work turns out to be something else: the last name given is
the one that stands, and the name UZE derives from a commit never replaces
one that was chosen.

## Commit on your branch, and stop there

Commit your work on your own branch as you go, in focused commits that each
pass the project's checks. Never commit to, merge into, rebase, or reset the
target branch: delivery is UZE's. When you are done, leave a clean tree —
uncommitted work is reported to the operator as exactly that, and is not
delivered — and end your turn. What happens next is the project's declared
completion behavior, and it is not yours to perform.

## When UZE hands work back to you

UZE rebases your branch onto the target before delivering it, and runs the
project's checks on the result. Three things come back to you, as a message
in your own session:

- **A paused rebase.** The target moved and your branch no longer applies
  cleanly. The rebase is paused in your checkout with the conflict markers
  in place. Resolve them preserving the intent of your change, run
  `git rebase --continue`, run the project's checks, and end your turn.
  Never abort the rebase to make the message go away.
- **Failed checks.** Fix them on your branch, commit, and end your turn.
- **A published branch with no request open for it**, under the `pr`
  completion. UZE has pushed the branch; opening the pull request — the
  merge request, on a forge that calls it that — is yours, because the
  title and the description are the change's argument and you are the one
  holding it. Name and describe it by this project's convention, do not
  merge it, and end your turn. UZE finds the request on the remote by
  itself afterwards, and from then on delivering the task only pushes new
  commits onto it.

Do not try to deliver again yourself; UZE re-reads your checkout when your
turn ends.

## Give parallel subagents their own checkout

Before two subagents write files, give each one its own checkout, and ask
UZE for it rather than making it with Git:

```bash
path="$(uze agent work split parser)"   # prints the checkout's path, nothing else
```

The checkout is cut from your current commit, recorded as yours, and taken
from the same pool as an agent's, so its build caches are warm. Hand the
path to the subagent and tell it to work and commit there. Asking again for
the same topic answers with the same checkout.

One checkout has exactly one writer. Split the work by file or component
boundary and state each owner's paths before they start. If their changes
cannot be made disjoint, sequence them rather than hoping Git can merge them
later. Repository-level Git metadata is shared across worktrees, so do not
rebase, force-push, or delete branches while other writers are active.

When a subagent is done, commit in both checkouts and bring its commits
onto *your* branch:

```bash
uze agent work join parser
```

Its commits are replayed onto your current commit and your branch moves
forward to them, with no merge commit. On a conflict the replay pauses in
the subagent's checkout: resolve it there, run `git rebase --continue`, and
join again. `uze agent work list` shows every checkout you split, whether it
holds uncommitted work, and how many commits your branch still lacks. UZE
delivers your branch, not theirs, and will not deliver it while a subagent
holds work you have not joined.

An agent working in the operator's own checkout has no branch of its own to
join into; run its subagents one after another instead.

## Retire checkouts safely

Checkouts are UZE's to reuse and remove; leave them. A subagent's checkout
goes back to the pool when you join it, or when you end; whatever it still
holds is kept for you on its branch and a shelf, and comes back beside you
when you are resumed. Never remove a checkout yourself, and never force
removal to discard uncommitted work. Commit as you go all the same: a shelf
keeps unfinished work, a commit says what it is.

Finish with a compact handoff: your branch, its tip commit, the checks you
ran, and any file another agent is likely to have touched too.
