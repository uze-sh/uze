## Context

See proposal.md — Why. Today `project/checkout.rs` reads `git worktree
list`, keeps the entries whose parent is the isolation directory, and
decides everything else from the agent store and the checkout's name. A
checkout nobody recorded is adopted (`reconcile`). One whose owner is not
live and whose branch holds nothing the target lacks is `Free`
(`slot_state`), and gets `switch -c` / `reset --hard` / `clean -fd` on the
next acquisition. The hotfix (`14065458`) narrowed `slots()` and adoption
to generated or legacy `agent-<n>` names.

"In use" exists only as "a pane's cwd is inside", passed in by the
terminal. `WorkspaceService::release_abandoned_tasks` ends every live agent
with no pane in its slot and no echoed launch. `uze agent work name` is the
one verb in the `work` noun; the caller is identified by `UZE_AGENT` plus
its cwd (`conversation::owner_of`).

A census of the repository this change is written in (2026-09-26) found:
- 29 worktrees, from roughly five concurrent agents:
  - 14 UZE slots;
  - 13 checkouts made by hand, mostly subagents following the projected
    `.worktrees/<topic>` text;
  - 2 of Claude Code's own.
- Of the 14 slots, 4 were free and idle, and 7 were parked.
- At least 4 of the parked slots were parked only by UZE's own writes: a
  re-resolved `agents.lock`, or a reprojected region in `AGENTS.md`.
- Two of those branches had already landed by squash. They would be free
  if not for that dirt.

The capability this change revises for slots, removal, adoption, siblings
and projection is `worktree-policy`, which lives in the open change
`add-portable-worktree-policy`. Those requirements are edited there in
place (task group 1); `checkout-accounting` adds what is new.

## Goals / Non-Goals

**Goals:**
- A recorded fact that UZE made a checkout, readable with no UZE state.
- One classification of every worktree, shared by the CLI, the TUI and
  every automatic decision.
- A process working in a checkout blocks every destructive decision.
- The pool stops growing on dirt UZE itself wrote.
- Subagent checkouts reuse the slot pool, so they are warm, and their work
  returns without merge commits the parent's rebase would drop.

**Non-Goals:**
- Managing a harness's own isolation. It is named and shown, never driven.
- Delivering a child's work to the target other than through its parent.
- Nested children.
- Removing a foreign checkout on any automatic path. Cleanup of foreign
  checkouts is one explicit gesture.
- Cross-repository accounting.

## Decisions

### The record lives in the worktree's own Git administrative directory

Every linked worktree has `<git-common-dir>/worktrees/<admin>/`, found from
inside the checkout as `git rev-parse --git-dir`. The admin directory's
lifecycle:
- Git creates it with the worktree and keeps it across `git worktree move`.
- `git worktree remove` deletes it.
- `git worktree prune` deletes it once the checkout is gone (and `gc` does
  after three months).
- `add --force` onto a registered but missing path deletes the old one
  first.
- A reused admin name gets a suffix only while the old directory exists.

So a file there (`uze-checkout.json`, shaped through `uze_document`) lives
as long as the checkout it describes.

The one way to attach a stale record is to copy a checkout and run `git
worktree repair`: both `.git` files then name one admin directory. The
record therefore carries the checkout's path and is ignored where the path
differs. In the primary checkout `--git-dir` is the common directory, so
the primary is never recorded, adopted or removed.

The record says only *that UZE made the checkout*, plus a subagent's parent
and split commit. Who holds it now stays `AgentStore::slot_owner`. Two
answers to "who owns this slot" would disagree the first time an older
build reuses a slot without rewriting the record.

Alternatives considered:
- *UZE's `state/`*: lost with the state, which is what made adoption by
  inference necessary in the first place.
- *A file in the working tree*: an agent can commit or delete it.
- *A ref under `refs/uze/`*: outlives the worktree.
- *Per-worktree Git config*: turning `extensions.worktreeConfig` on changes
  how every Git client reads the repository.

The file is written with plain `fs` under `uze_git`'s repository write
lock; only the `rev-parse` goes through `uze-git`. By tier it is a
*record*, the second path UZE writes outside `$UZE_HOME` after
`.git/info/exclude`. The path is named in `uze-core`'s worktree module,
and AGENTS.md "What UZE persists" gains a row for repository-side files.
`every_path_uze_owns_is_named_in_the_map` only scans `$UZE_HOME`
compositions and is left alone.

### Recording on sight replaces adoption by inference

Every accounting pass records a checkout under the isolation directory
when either:
- the task store names it as the checkout of an agent UZE *launched*: one
  with a harness, unlike `Agent::in_the_root("")`, which marks an earlier
  build's adoption; or
- it is named `agent-<n>`, as the builds before slots made them.

This is a standing rule, not a one-time rung. An older build on the same
machine keeps making slots for agents it launches, and they are recorded
the next time this build looks. No marker file is needed. A generated-shape
name alone never records anything: a 6-letter topic such as `parser` has
that shape.

Checkouts the store names only through inference are listed for adoption.
`CheckoutId::is_uze_made` is deleted.

### Derived dirt is recognised, not written around

"Holds work", for park/free decisions only, gains one exception evaluated
in `is_dirty`'s caller:
- **`agents.lock`:** a change counts unless every plugin present in both
  `HEAD`'s lock and the working lock keeps the same revision and digest.
  That is all `install` does: add or drop entries. `update` exists to move
  a pin and nothing else, so a moved pin is deliberate work.
- **`AGENTS.md`:** a change counts only if the file still differs from
  `HEAD`'s after both sides drop every region `text_region` recognises as
  UZE's.

Everything else is work. Reuse already runs `reset --hard`, so these
changes go with the rest of the tree. Rebase, join and delivery keep
requiring a Git-clean tree.

Stopping every UZE write inside slots would leave the lock and the region
stale for the agent working there, and the agent itself is told to run
`uze install`. Recognising the output is the one rule that holds whoever
wrote it.

### "In use" is a probe of process working directories

`machine::process_cwd` answers, once per decision, which directories this
user's processes are working in:
- **Linux:** `readlink /proc/<pid>/cwd` over the pids owned by the real
  uid. EACCES (non-dumpable daemons) and ENOENT (exited mid-scan) are
  skipped.
- **macOS:** `proc_listallpids` + `proc_pidinfo(PROC_PIDVNODEPATHINFO)`
  via `libc`, with the same skips.
- **Failure:** only a failure to enumerate at all makes every checkout in
  use.
- **Exclusions:** the calling `uze` process and the Git children it spawns
  for the decision.

The terminal server `chdir`s to `/` at start. It is otherwise started from
whatever shell ran the first `uze`, often inside a slot, and would hold
that slot for its whole life. Excluding it by pid is not possible without
excluding every pane under it.

The pane list (`occupied`) keeps its other meanings:
`release_abandoned_tasks` (an agent is in front of its slot) and
evaluation's `parked_with_agent`. Only `slots`, `take`, `collect`,
`remove_idle_slots` and the operator's remove switch to the probe.
Collection runs on every occupancy pass, so the probe is measured there.

Accepted trade-off: a language server, a build daemon or an editor left in
a finished slot keeps it in use, and the pool creates a directory instead.
That is the price of never resetting a directory somebody is in. The view
shows which slots are held that way.

### A harness's isolation is declared by its integration

`IntegrationPort` gains `own_worktree_dirs(&self) -> Vec<&'static Path>`,
relative paths defaulting to none; the Claude integration answers
`.claude/worktrees`. The application collects them from the registry and
hands core the list as data. Core classifies a worktree as a harness's
when it sits under that relative path inside *any* checkout of the
project, primary or slot. The trait stays one trait. A conformance check
in the Claude vertical proves the directory is where the harness actually
puts its worktrees.

### A subagent is an agent record with a parent

`Agent` gains `parent: Option<AgentId>`, additive with `#[serde(default)]`
and no shape bump. A bump would make every older build fail on the whole
task store. The child's topic is its label, and its split commit is its
`Isolation::base_commit`.

A child is excluded from:
- `release_abandoned_tasks`;
- reconcile's revival;
- follow-the-target rebases;
- naming from a commit subject;
- readiness evaluation;
- delivery.

It ends by `join` or with its parent, in the same sweep.

Two-builds cost:
- **An older build drops `parent` on its next save.** `Agent` has no
  catch-all field. The record carries the parent and split commit, so
  accounting restores `parent` when the store's holder of a recorded child
  checkout has none and its `base_commit` equals the record's split
  commit. An older build's reuse changes `base_commit`, so it is not
  mistaken for the child.
- **While the older build runs, it may still release and reuse a child.**
  The child is protected then only by a process working inside it. This is
  accepted, rather than locking older builds out of the whole store.
- **Reuse rewrites the record for the new holder.** That covers `create`
  and `take` alike.

### The verbs

- **`split`:** runs the same placement an agent gets: acquire, then
  `materialize` (links and the project's `setup`), then record. It prints
  the path alone on stdout. It is `JustifiedSlow`, because `setup` may take
  minutes.
- **`join`:** if the caller's `HEAD` is already an ancestor of the child's
  tip, only `merge --ff-only`. Otherwise, in the child's checkout,
  `rebase --onto <caller HEAD> <split commit> <child branch>`, then
  `merge --ff-only <child branch>` in the caller's checkout, both under the
  write lock.
  - After a completed replay, the child's split commit is the `HEAD` it
    was replayed onto.
  - It refuses while a rebase is paused in the child (a detached `HEAD`
    mid-replay is not "left its branch"), and while the probe finds a
    process other than the caller in the child's checkout.
  - This leaves no merge commit. Landing's plain `rebase` would drop one,
    and with it any conflict resolution inside it.
  - It never reintroduces the caller's pre-rebase commits.
  - A conflict pauses the rebase in the child's checkout. The paths are
    printed, the exit code is non-zero, and a second `join` after
    `rebase --continue` completes.
  - It is `JustifiedSlow`.
- **`list`:** one tab-separated line per child (topic, path, branch,
  `clean`/`dirty`, commits ahead of the caller). It is `Budgeted`, with its
  `BUDGETED_COMMAND_TESTS` entry and a timing test.

The caller is resolved as `work name` resolves it. A caller whose cwd lies
in one of its children is refused with that reason rather than "not an
agent".

### The checkouts view

An application read model, `CheckoutsView`, built from the classification.
The TUI reads it through a `spawn_checkouts`/`absorb_checkouts` pair and
places it in the space's menu. It offers:
- **Adopt:** only for checkouts under the isolation directory. It writes
  the record, and says a clean checkout becomes free at once.
- **Remove:** inspects first and keeps the branch.
- **Clean up:** applies Remove to every checkout of the operator's class
  that is clean, unused and in the target, and lists what it kept and why.
  A harness's own isolation is left to its harness.
- **Join:** for a parked child, into its parked parent.

The existing operator `discard` of a parked task keeps its force-remove.
It is already an explicit decision on a named task.

## Candidate ADRs

- Record that UZE made a checkout in Git's per-worktree administrative
  directory. It is a second path UZE writes outside `$UZE_HOME`, and the
  rule that decides what UZE may destroy.

## Risks / Trade-offs

- [Scanning every process on each occupancy pass] → one scan per pass,
  skipping per-process errors. A few hundred `readlink`s cost a few
  milliseconds; measured in `command_performance`.
- [A process started a moment after the probe] → acquisition runs under
  the repository write lock; the window is narrower than today's pane
  check.
- [The operator deletes an admin directory by hand] → the checkout becomes
  foreign and is listed, never destroyed.
- [An older build releases a child during the two-builds window] → stated
  above. Accepted, and it closes with the older build.
- [A slot held by a daemon forever] → shown in the view. The pool grows by
  one directory rather than resetting under somebody.
- [Derived-dirt rule applied to a lock the agent edited on purpose] → only
  a lock that moves no pin is derived. A moved pin is `update`'s output and
  counts as work.

## Migration Plan

1. Ship the record, recording on sight, classification and the derived
   dirt rule, with the hotfix's name rule still guarding older paths.
2. Switch every automatic decision to the record and the probe; delete the
   name rule.
3. Ship the verbs, the parent/child lifecycle and the projected text.
   Reconciling a project's context replaces the region.
4. Rollback: an older build ignores the record and the `parent` field (see
   the two-builds cost).
