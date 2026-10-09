## Context

See proposal.md for why. Today a slot's state is derived in
`checkout::slot_state` (`crates/uze-workspace/src/checkout/accounting.rs`)
as `Occupied`, `Free` or `Parked`, and `Parked` is what any work in the
directory produces. `checkout::release` only rewrites the task record; it
touches no directory. The collection (`checkout::collect`) prunes integrated
branches and then trims free slots beyond `Pool::spare`. The holder of a
reused slot is `AgentStore::slot_owner`, the task naming the slot with the
greatest `created_at_unix`. The workspace client reaches all of this from
`Workspace::reconcile_occupancy`, on a thread, when a tab closes and once on
attach; placements call `checkout::reconcile` under the task store's lock.

The regression suite for every scenario is
`crates/uze-application/tests/slot_lifecycle.rs`, driven through the calls
the workspace client makes.

## Goals / Non-Goals

**Goals:**
- A checkout carries no work once its agent ends, so reuse is the normal
  outcome of every release.
- Every fact that decides a slot's holder or a slot's freedom is one Git,
  the process table or the task store holds at the moment it is read, never
  an ordering by wall-clock time and never one client's view alone.
- Existing parked checkouts are freed by the same code path that frees a
  new one; no migration code of its own.
- No compatibility kept for what this replaces: no alias for `parked`, no
  tolerance for `spare`. UZE is in beta with few users, and the record's
  ladder is the only bridge.

**Non-Goals:**
- Pruning ended tasks from the task store.
- Preserving the staging area across a shelf.
- Pushing branches or shelves anywhere.
- Carrying a conversation a harness keys by directory across directories.
- A size guard on what is shelved (see Risks).

## Decisions

### A shelf is a commit under `refs/uze/shelf/<task-id>`, built with plumbing

On release, in the slot, under the repository's write lock:

1. The slot's own index is copied to a temporary file in the slot's
   administrative directory (unique per process), which keeps its stat
   cache, skip-worktree bits and intent-to-add entries. `add -A` runs with
   `GIT_INDEX_FILE` pointing at the copy, so it records exactly what the
   repository would report: tracked changes, deletions, and untracked files
   that are not ignored. Unmerged entries never reach this point: a merge in
   progress pins the slot first.
2. Content UZE derives (`checkout-accounting`) is reset to `HEAD` in that
   index, path by path, by the same predicate that decides it is not work
   today.
3. Nothing to shelve when the index's tree equals `HEAD`'s *and* `HEAD` is
   reachable from a branch. Otherwise `commit-tree` writes the snapshot with
   `HEAD` as first parent and, when the task already has a shelf, that shelf
   as second parent, so no earlier shelf is ever dropped. The subject names
   the label; trailers `Uze-Task:` and `Uze-Branch:` name the task and its
   branch. The commit is written with a fixed UZE identity, `--no-gpg-sign`
   and hooks disabled, so neither the operator's signing setup nor a missing
   identity can block or fail it.
4. `update-ref refs/uze/shelf/<task-id> <new> <expected-old-or-zero>`
   compare-and-swaps the ref.
5. The temporary index is deleted.

The slot's own index and working tree are untouched until the ref exists.
`refs/uze/*` is shared by every worktree of the repository, so a shelf made
in one slot is visible to a resume in another. `refs/uze/<name>` is also
where `uze_git::fetch_private` puts what it fetches; `shelf/` is reserved
there so no fetch name can land on a shelf.

These Git calls go through `crate::git` in `uze-workspace` (the slot guard),
and `GIT_INDEX_FILE` through `uze_git::write_with_env`, which already
exists.

Alternatives considered:
- **The repository's stash.** Its stack is shared by every worktree and by
  the operator; AGENTS.md already forbids bare `git stash` for that reason.
- **A WIP commit on the task's branch.** It would land in the reviewed
  history unless stripped, could be pushed by the agent's next delivery, and
  would change the "is this branch integrated" answer that collection and
  settlement depend on.
- **Keep the work in the directory (today).** Unbounded directories.

### What cannot be shelved pins the slot

A submodule with changes or commits of its own, and an untracked directory
that is itself a repository, record only a commit id in a shelf, and the
objects behind it live in the slot. Such a slot is pinned with that reason
rather than shelved. Pins in all: a paused operation (`rebase-merge`,
`rebase-apply`, `MERGE_HEAD`, `CHERRY_PICK_HEAD`, `REVERT_HEAD`,
`BISECT_LOG`, `sequencer/`), a process inside, uncapturable content, and a
shelf that could not be written. The reason is kept in the slot's
`CheckoutRecord` (an additive field), so it can be shown without
re-deriving it.

### Release is the only path that resets a slot, and it asks the machine

`checkout::release` is the one function that shelves and resets. Under the
repository's write lock, immediately before resetting, it reads
`Presence::observe_with(held)`. If anybody is inside, it leaves the slot
and the task alone: the task is not ended. The client's `held` and
`echoed` still decide who is *asked about*; the process table decides
whether anything is *done*. An agent another client launched, a placement
whose tab has not opened yet (it is in the directory running `setup`) and a
delivered agent still in its tab are all a process inside.

`checkout::reconcile` never shelves or resets. It marks a recorded slot
holding work for an ended holder as due for release, and the occupancy pass
releases it. `release_abandoned_tasks` therefore visits ended tasks that
still name a checkout, not only live ones, and releases at most one slot
per acquisition of the task lock, so the first pass after an upgrade
(about 20 slots in UZE's own repository) never holds placements behind
it.

Order inside the release: shelf ref written, then the worktree is checked
again to still match the shelf tree (closing the window between `add -A`
and the reset), then `switch --detach --discard-changes HEAD` and
`clean -fd` (no `-x`, so ignored output stays; no `-ff`, but nested
repositories never get here because they pin). Detaching at `HEAD`, not at
the branch tip, works whatever happened to the branch. Last, the task
record is written: shelved or closed, and `isolation.checkout = None`.
A crash between the reset and the record leaves a running task naming a
clean, detached slot with a shelf. The next release reads that shelf, not
the clean tree, so the task is recorded shelved, never closed.

### One holder per slot

A non-pinned release clears the task's checkout in the same critical
section. `take` also clears every other task naming a slot it reuses.
`slot_owner` becomes the single task naming the slot. For stores written by
earlier builds that name one slot from several tasks, reconciliation
decides once from Git: the task whose branch the slot has checked out holds
it. When that does not decide it (detached `HEAD`, or several live
candidates), the slot is pinned, never freed. The early return in
`resume_task` ("a task that still has its checkout") also checks that the
slot has the task's branch checked out.

### One definition of "holds work"

`holds_work(task) = branch holds commits the target lacks OR shelf holds
changes the target lacks`. It is used by release, settlement
(`landing::settle_delivered` does not settle a task whose shelf holds
work), branch pruning (`prune_integrated_branches` keeps any branch whose
task has a shelf), `end_without_checkout`, and the unjoined-children check
before delivery.

### Comparing and applying shelves with plumbing

- **In target:** `diff-tree -r -z --no-renames <parent> <shelf>` lists every
  path the shelf changes, deletions included. For each, the blob id in the
  shelf (or its absence) must equal the target's, read with `ls-tree -z`.
  There is no pathspec anywhere, so a path such as `[x].md` is never read
  as a pattern.
- **Restore when the branch tip is the shelf's first parent:**
  `restore --source=<shelf> --worktree -- :/`. The index stays at `HEAD`,
  so everything comes back unstaged.
- **Restore onto a moved branch:**
  `diff-tree -p --binary --full-index --no-renames <parent> <shelf>` piped
  to `apply` (no `--index`, no `--3way`). It is all or nothing; on failure
  the files are named and the shelf kept.
- **Idempotent:** a restore is skipped when the worktree already matches
  the shelf.
- **Order:** the ref is deleted, with compare-and-swap, only after the task
  record that says the work is back has been written.

`carry_changes` has the same porcelain-`diff` weakness (`diff.noprefix`,
binary files) and moves to the same plumbing.

### Slot states: `Occupied`, `Free`, `Pinned { reason }`

`SlotState::Parked` is removed. The cap counts `Occupied` and `Pinned`, and
`CapReached` names closing an agent.

### The task store moves to shape 5

`WorkState::Parked` is renamed `Shelved`, spelled `shelved` on disk, with no
serde alias. The store's shape goes from 4 to 5 with one rung that rewrites
`parked` to `shelved`, which is the one sanctioned way a record carries an
older build's data. The rung is deletable once no machine sits below shape
5. An older build installed beside this one refuses the newer store, as
uze-document refuses any newer shape. That is accepted: UZE is in beta and
keeps no path for what it removed. The task record gains one additive
field, `last_checkout`. What the person reads is the word "unfinished";
"shelf" stays the name of the mechanism.

### Resume

Resume prefers the slot the task last held when it is free, which is where
a harness that keys its conversation by directory left it. Elsewhere, the
existing continuity rule applies: a conversation the harness cannot find
from the new directory starts anew, with the note it already gives. After
`checkout::resume`, the shelf is restored. A missing branch is recreated at
the shelf's first parent. A shelved agent's children are placed again under
their recorded topics, with the `split_work` placement, and their shelves
restored. When a restore fails during placement, the slot is given back
with an un-take that never touches refs, not with `discard`, which deletes
the shelf.

### Pool by idle age only

`WorktreePolicy::spare` and `Pool::spare` are removed. Release and
placement stamp the slot directory's modification time. `trim_free_slots`
removes a free slot whose directory has not been stamped for `idle_days`,
and still refreshes the index of the slots it keeps (the warm-up
fast-agent-placement added). `collect` trims directories first and prunes
branches second, so a trimmed slot's integrated branch goes in the same
pass.

### Settling work merged after its agent ended

A task that ended holding commits and is later found integrated, with no
shelf holding work, is recorded `Integrated`, including when another agent
took its slot first. `Closed` stays for tasks that ended holding nothing.

### Persistence

`refs/uze/shelf/*` is a record (nothing else knows it), and the third thing
UZE keeps inside the repository's Git directory. It is named in the
worktree module with the other two, and in AGENTS.md "What UZE persists".
Its format is the commit and its trailers, which grow only by additive keys.

### Diagrams

`docs/architecture/agent-lifecycle.mmd` and `checkout-ownership.mmd` draw
`Parked` and the spare rule. Both are redrawn for shelving and the idle
rule, and checked with `cargo test -p uze-extensions` and
`uze agent artifacts check`.

## Candidate ADRs

- **Agent work is kept in Git refs, never in a checkout directory.** It
  fixes what a slot is (capacity only) and introduces a UZE-owned ref
  namespace that resume, preserved work and collection build on.

## Risks / Trade-offs

- [An untracked, non-ignored tree is huge, such as a dependency directory the project forgot to ignore] → It is written as objects once and becomes unreachable when the shelf is collected or discarded; `gc` reclaims it. No size guard in this change.
- [Ignored files in a slot that sits idle are deleted with the directory] → Already true of free slots today. Former parked slots now age out too. A non-reproducible ignored file belongs in `link:`, which the docs say.
- [`refs/uze/*` is published by `git push --mirror` or a `refs/*` push refspec, and pruned by a `+refs/*:refs/*` fetch] → Documented; shelves can hold non-ignored secrets.
- [`Presence` sees only a process's working directory] → An editor elsewhere writing into a slot is invisible. `reuse` keeps `--discard-changes`. Documented.
- [A process the operator left in a slot pins it indefinitely] → The pin and its reason are shown in the checkouts view.
- [A shelf exists only on this machine] → Same as an uncommitted tree today.

## Migration Plan

No separate step. The task store is read through the shape 4 to 5 rung, and
the first occupancy pass after the upgrade releases each former parked slot,
one per lock acquisition, through the shelving release. Pinned ones stay
until their pin clears. An `agents.yaml` still declaring `spare` is reported
as declaring an unknown key; removing the line is the whole fix. Rolling back
to an older build means letting it set the newer store aside (its floor);
shelves stay as refs it ignores, and `git for-each-ref refs/uze/shelf/`
lists them.
