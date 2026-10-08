## Why

Creating an agent in a project that isolates its agents took 2.3–4.5s on
this repository, with spikes of 9–14s, while the workspace header said
"preparing". Measured against the journal and a reproduction on a clone of
this repository with sixteen slots:

- every placement fetched the target from the remote first, over SSH,
  holding the repository write lock: 1.5–2.1s on every agent;
- choosing a slot ran `git status` in every working tree before asking the
  remembered integration answer that would have parked it anyway;
- the evaluation pass held the agents document's lock while it built the
  views, which read Git twice for each of the 86 recorded agents, closed
  ones included, so a placement arriving during a pass waited up to its
  whole length;
- collecting a spare slot deleted its directory — gigabytes of build output
  — under the repository write lock, and read the agents document without
  its lock, so it could collect a slot a placement had just taken.

## What Changes

- **The target is synced on the workspace's clock, never by a placement.**
  Every three minutes and when the workspace opens, each visible project
  that isolates its agents fetches its target and fast-forwards it. A
  placement branches from the local tip as the last sync left it. A
  delivery keeps fetching for itself. **BREAKING** for the invariant "An
  agent is placed on the target as the remote has it": it now reads "as
  the last sync left it".
- **The sync never moves files under the operator.** A target the operator
  has checked out in the primary checkout is left for their pull, which the
  checkout's caption already offers; the sync only moves a target nobody
  stands on. A target left behind is said once, as a toast, until it
  catches up.
- **Choosing a slot asks the remembered question first.** Commits that park
  a slot are read before its working tree, so a pool of parked slots costs
  no `git status`.
- **A reused slot discards what never parked it.** `switch
  --discard-changes` replaces `switch` + `reset --hard`, so a free slot
  holding only UZE's own region edits is handed on even when the target
  changed the same file.
- **The evaluation builds its views outside the lock, once.** The pass
  copies the records it wrote and builds the views from the copy after
  releasing the lock, instead of twice inside it.
- **Many integration questions resolve their names once.** A collection
  and a slot listing read every branch tip with one `for-each-ref` and ask
  the remembered integration answer by commit, instead of a `rev-parse` per
  branch; a branch a record names but Git no longer has is skipped.
- **Collection sets a slot aside under the lock and deletes it after.** The
  slot is renamed into `.worktrees/.trash/` and pruned from Git's registry
  under the agents document's lock and then the repository lock, re-checked
  for uncommitted work first; its bytes are deleted with no lock held. A
  rename the platform refuses falls back to `worktree remove` in place. The
  spares it keeps have their index refreshed, so the next placement does not
  hash every file of a tree written in the same second as its index.

## Capabilities

### Modified Capabilities

- `worktree-policy`: "An agent is placed on the target as the remote has
  it" becomes "An agent is placed on the target as the last sync left it".

## Impact

- `uze-workspace`: `landing::sync_target` leaves a checked-out target
  alone; `checkout::slot_state` order; `checkout::reuse`; `collect` sets
  slots aside, `empty_trash` deletes them.
- `uze-application`: `Workspace::sync_target`, `TargetSyncReport`;
  placement no longer syncs; evaluation and collection lock scopes.
- TUI: a `tui.target_sync` background pass on its own clock and a toast.
- Measured on a clone of this repository (remote over SSH, ten to sixteen
  slots, a growing agents document), p50 / max, before → after:
  placement alone 2.31s / 2.46s → 0.087s / 0.21s; against a looping
  evaluation 12.3s / 21.6s → 0.087s / 0.36s; while a collection removes two
  slots with 320 MB of build output each 5.33s / 5.43s → 0.58s / 0.85s; the
  evaluation pass itself 3.15s → 1.66s.
