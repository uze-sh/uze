# Agent work is kept in Git refs, never in a checkout

Status: Accepted

## Context

A slot was meant to make isolation cheap, by bounding checkouts to peak
concurrency and letting build output survive from one agent to the next. But
a checkout was also where unfinished work was kept, so every agent that ended
with anything in its directory pinned that directory for good, and the next
agent paid for a new worktree and a fresh `setup`. The repository UZE is
developed in reached 27 checkouts and 152 GB, 20 of them holding uncommitted
changes that never reached a commit.

## Decision

**A checkout is capacity. When an agent ends, its uncommitted work is recorded
as a snapshot commit under `refs/uze/shelf/<task>`, cut from the branch tip,
and the directory goes back to the pool clean.**

- A shelf moves no branch, uses no stash and stays on this machine. A task
  holds one shelf, and a later shelf keeps the earlier one reachable.
- Resuming places the branch in any free checkout and puts the shelf back as
  uncommitted changes, all unstaged, then removes the shelf.
- Only a paused rebase or merge, a process that is not UZE's working in the
  directory, or work a shelf cannot capture pins a checkout.

The rejected alternatives were the repository's stash, whose stack every
worktree and the operator share; a work-in-progress commit on the task's
branch, which could reach the reviewed history and would change whether a
branch reads as integrated; and keeping the work in the directory, which
leaves the number of directories unbounded.

## Consequences

The number of checkouts follows how many agents ran at once, and reuse is the
normal outcome of every release. Resume, preserved work and collection build
on one UZE-owned ref namespace, so `shelf/` is reserved under `refs/uze/`.

Staged and unstaged are no longer told apart. A shelf can hold secrets the
repository does not ignore, and `git push --mirror` or a `refs/*` refspec
would publish it. A conversation a harness keys by directory does not follow
its work to another checkout.

Source change: openspec/changes/archive/2026-11-02-shelve-work-not-checkouts/
