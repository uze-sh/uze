## Why

A slot is supposed to make isolation cheap: the number of checkouts is
bounded by peak concurrency and ignored build output survives from one
agent to the next. In practice a checkout is the place work is *kept*,
so every agent that ends with anything in its directory pins that
directory for good, and the next agent pays for a new `git worktree add`
and a fresh `setup`. A probe driving the workspace's own calls through a
simulated week with at most four agents at a time left 14 checkouts; the
repository UZE is developed in has 27, 152 GB, 20 of them uncommitted
changes on `agent/<id>` branches that never reached a commit. The same
probe found a running agent losing its slot because ownership of a
reused slot is decided by wall-clock seconds, which go backwards on WSL.

## What Changes

- **Work is kept in Git, never in a directory.** When an agent ends with
  uncommitted work in its checkout, that work is recorded as a snapshot
  commit under `refs/uze/shelf/<task>`, cut from the branch tip without
  moving the branch or touching the shared stash, and the directory goes
  back to the pool clean. Commits the target lacks are already kept by
  the branch and pin nothing either. Both stay local.
- **A new work state, `Shelved`, replaces `Parked`** as what an agent that
  ended holding work becomes, shown as "unfinished". **BREAKING**: the task
  store moves to shape 5, so an older build refuses it; no alias is kept
  for `parked`. Resuming it places the branch in *any* free
  checkout and puts the snapshot back as uncommitted changes. Staged and
  unstaged are not told apart: everything comes back unstaged.
- **Only two things pin a directory**: a rebase or merge paused in it, and
  a process that is not UZE's working inside it. Both are temporary and
  each is reported with its reason.
- **The pool follows peak concurrency.** A free checkout is removed only
  after it has gone unused for `idle_days`. **BREAKING**: the `spare` key
  is removed from `workspace:` in `agents.yaml`; a manifest that still
  declares it is reported as declaring an unknown key, as any other
  unknown key is.
- **A declared cap counts only checkouts in use**, and its refusal tells
  the operator to close an agent rather than to park a task, which never
  freed one.
- **A slot has exactly one holder.** Reusing a slot takes it from the
  task that held it in the same critical section; no reading of a slot's
  holder depends on when its tasks were created.
- **A shelf is never orphaned.** One with no task recording it is adopted
  back as a `Shelved` task from what its commit says; one whose content
  the target already carries is collected; discarding a task deletes its
  branch and its shelf.
- **Existing parked checkouts are shelved and freed** on the first pass
  after the upgrade, unless one of the two pins holds them.
- Smaller corrections found by the same probe: work merged without a
  squash after its agent ended settles as `Integrated` rather than
  `Closed`, and a trimmed checkout's integrated branch is collected in
  the same pass rather than the next one.

Out of scope: pruning ended tasks from the task store (they carry the
conversation an agent resumes), preserving the staging area, and pushing
local commits anywhere as a backup.

## Capabilities

### New Capabilities
- `work-shelf`: what an agent's uncommitted work becomes when it ends,
  how it is put back, adopted when unrecorded, collected and discarded.

### Modified Capabilities
- `worktree-policy`: slots are freed rather than parked when their agent
  ends holding work; only a paused operation or a foreign process pins a
  checkout; a slot's holder is explicit; the cap counts checkouts in use.
- `checkout-accounting`: the pool keeps free checkouts by idle age alone
  (the spare count is removed); content UZE derives is never shelved.
  This capability is introduced by `account-for-every-checkout`, whose code
  is merged; that change is archived before this one.
- `agent-isolation`: an isolated agent that ended holding work frees its
  slot, and its work is listed as shelved until resumed, finished or
  discarded.

## Impact

- `crates/uze-workspace/src/checkout/` — `lifecycle.rs` (`release`,
  `discard`, `trim_free_slots`, `collect`), `pool.rs` (`Pool`, `take`,
  `reuse`, `resume`, `CapReached`), `accounting.rs` (`slot_state`),
  `reconcile.rs` (adopting shelves, the holder rule), and a new shelf
  module for snapshot, restore and listing.
- `crates/uze-workspace/src/task.rs` — `WorkState::Shelved` in place of
  `Parked`, the task store's shape ladder for the rename, `slot_owner`.
- `crates/uze-workspace/src/worktree.rs` — `WorktreePolicy::spare` removed.
- `crates/uze-application/src/application/services/tasks/` — occupancy
  (release, collection), placement (resume), evaluation (settling),
  views (`WorkStateView`), and the preserved-work list.
- `src/ui/` — the column and toasts naming shelved work and the reason a
  checkout is pinned; the checkouts panel.
- Docs: `web/content/docs/workspace/agents.mdx`,
  `web/content/docs/reference/project-files.mdx`, the projected
  "Concurrent work isolation" region if it mentions parking.
- Tests: the probe in `crates/uze-application/tests/worktree_dx_probe.rs`
  becomes the regression suite for these scenarios; a journey in
  `04-workspace` once the operator has validated by hand.
