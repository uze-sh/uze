## 1. Sync off the placement path

- [x] 1.1 Remove `landing::sync_target` from `place_in_slot`; branch from the local target.
- [x] 1.2 Add `Workspace::sync_target` (isolating projects only) returning `TargetSyncReport`; re-ask slot states after a fast-forward so the next placement finds the integration answers remembered.
- [x] 1.3 `landing::sync_target` leaves a target checked out in the primary checkout alone and reports it.
- [x] 1.4 TUI: `spawn_target_sync` on `TARGET_SYNC` (3 min, and on open), one per primary; toast once per fall behind.
- [x] 1.5 Tests: placement does not fetch and a sync brings the next agent up to date; a non-isolating project is never synced; a checked-out target is left for the pull; the toast is said once until caught up.
- [x] 1.6 Fetch into `refs/uze/sync/<target>` without the repository lock (`uze_git::fetch_private`), taking the lock only to move the tracking ref and fast-forward: under the lock, the evaluation's and every placement's `worktree prune` waited out the network every three minutes.
- [x] 1.7 Reword the invariant and its tests in `docs/architecture/invariants.md`.

## 2. Slot choice and reuse

- [x] 2.1 `slot_state`: integration answer before `holds_uncommitted_work`.
- [x] 2.2 `reuse`: `switch --discard-changes`, drop `reset --hard`.
- [x] 2.3 Test: a free slot holding a region edit is reused onto a target that changed the file.

## 3. Lock scopes

- [x] 3.1 Evaluation returns a copy of the records and builds the views once, after the lock.
- [x] 3.2 Collection runs under the agents document's lock, sets slots aside into `.worktrees/.trash/`, and empties the trash after releasing it.
- [x] 3.3 `BranchTips`: one `for-each-ref` per collection and per slot listing; `is_integrated_among` asks the remembered answer by commit; branches Git no longer has are skipped.
- [x] 3.4 Refresh the index of each spare a collection keeps.
- [x] 3.5 Tests: a collected slot leaves the registry at once and its bytes after; a slot written in after it was read free is not collected.

## 4. Evidence

- [x] 4.1 Debug spans on `checkout.reconcile`, `checkout.slots`, `checkout.take`, `checkout.materialize`, `checkout.collect`, `checkout.empty_trash`, `landing.sync_target`.
- [ ] 4.2 Confirm against the journal on this repository after the operator validates by hand.
