## 0. Prerequisite

- [ ] 0.1 Archive `account-for-every-checkout` (its code is merged in #128) so `checkout-accounting` exists in `openspec/specs/` before this change's delta is applied; its two open tasks (3.4 conformance, 10.2 journey) are carried as follow-ups named in its archive note.

## 1. Regression suite first

- [x] 1.1 Turn the probe into an asserting suite, `crates/uze-application/tests/slot_lifecycle.rs` (its own test target): one test per scenario in this change's specs, driven through the calls the workspace client makes, checking directories under `.worktrees/`, `git worktree list`, branches, `refs/uze/shelf/*`, the stash, the process table and the task record, never UZE's own report except where the spec makes the answer the behavior (the cap's refusal, a conflict naming its files, the unknown `spare` key).
- [x] 1.2 Add the clock-step test deterministically (`slots::the_clock_going_back_does_not_move_a_slot`), a real process inside a checkout (`stay_inside`, this test binary running an ignored helper), and records left by an earlier build (`edit_store`).
- [x] 1.3 Run it against today's code: 52 tests, 21 pass (behavior that holds and must keep holding), 30 fail for the reason the spec predicts, 1 ignored helper. Tests land on the branch with the group that turns them green; nothing red is committed.
- [x] 1.4 The lock-file scenarios of "Content UZE derives never parks a checkout" (a pin moved, entries added) stay at L0 in `checkout/accounting_tests.rs`; group 3 makes them assert what is shelved.

## 2. One holder per slot

- [x] 2.1 `take`/`reuse` clear `isolation.checkout` on every other task naming the reused slot, ending each by `end_without_checkout`, under the caller's task lock; `AgentStore::slot_owner` becomes the single task naming the slot, with no `created_at_unix` ordering.
- [x] 2.2 `reconcile` resolves slots several records name from Git (the task whose branch is checked out holds it; undecided → pinned), and the early return in `resume_task` checks the slot has the task's branch.

## 3. The shelf

- [x] 3.1 A `shelf` module in `crates/uze-workspace/src/checkout/`: snapshot from a copy of the slot's index with `add -A`, derived content reset, unbranched `HEAD` kept, earlier shelf as second parent, fixed identity, no signing, no hooks, compare-and-swap `update-ref`; `refs/uze/shelf/` named in the worktree module and reserved against `fetch_private`.
- [x] 3.2 `restore`: fast path with `restore --source --worktree :/`, moved branch with `diff-tree -p --binary --full-index --no-renames | apply`, idempotent, branch recreated at the shelf's parent when missing; ref removed with compare-and-swap only after the caller recorded the restore.
- [x] 3.3 `list` from the same `for-each-ref` as `BranchTips`, with the trailers; `is_in_target` by blob id per path from `diff-tree -r -z --no-renames`, no pathspecs.
- [x] 3.4 What cannot be shelved (a submodule with changes or commits of its own, a nested repository) and the extended paused-operation check (`CHERRY_PICK_HEAD`, `REVERT_HEAD`, `BISECT_LOG`, `sequencer/`) as pin reasons, with the reason an additive field of `CheckoutRecord`.
- [x] 3.5 L0 tests in `checkout/shelf_tests.rs`: tracked change, deletion, new file, ignored file left out, derived-only change gives nothing, a moved lock pin is shelved, branch and stash untouched, an earlier shelf kept as second parent, restore unstaged, restore onto a moved branch, conflict keeps the shelf, a nested repository pins.

## 4. Release, slot states and the cap

- [x] 4.1 Replace `SlotState::Parked` with `Pinned { reason }`; `slot_state` answers holder, pin or free.
- [x] 4.2 Rewrite `checkout::release` as the only shelving release: process table read under the repository lock right before the reset, pinned leaves everything, otherwise shelve, re-check the tree against the shelf, `switch --detach --discard-changes HEAD`, `clean -fd`, stamp the directory, and record shelved or closed with `checkout = None`.
- [x] 4.3 `release_abandoned_tasks` also visits ended tasks still naming a checkout, releases one slot per task-lock acquisition, and treats a pane on an ended task as occupied; `reconcile` only marks slots due for release.
- [x] 4.4 Route subagent children through the same release; an agent with a child holding work is shelved; the unjoined-children check before delivery reads shelves too.
- [x] 4.5 Count only `Occupied` and `Pinned` toward `slots:`, and make `CapReached` name closing an agent.

## 5. Work state and the task store

- [x] 5.1 Rename `WorkState::Parked` to `Shelved`, spelled `shelved` with no alias, and move the task store to shape 5 with one rung rewriting `parked`, plus a ladder test reading a shape 4 `parked` task that still names a checkout; rename `WorkStateView::Parked` and every match in `uze-application` and `src/ui/`.
- [x] 5.2 One `holds_work` used by settlement, branch pruning, `end_without_checkout` and release: a task with a shelf holding work is never settled integrated and its branch never pruned; a task that ended holding commits and is later found integrated is `Integrated`, also after another agent took its slot.
- [x] 5.3 Adopt shelves with no task in `reconcile` as shelved tasks under the id the shelf names; collect shelves the target carries; `discard` deletes the shelf with the branch.

## 6. Resume

- [x] 6.1 `resume_task` prefers the slot the task last held when free, restores the shelf after `checkout::resume`, recreates a missing branch, refuses on conflict naming the files and gives the slot back with an un-take.
- [x] 6.2 A shelved agent's children are placed again under their recorded topics and their shelves restored; the operator's Join on a shelved child becomes resume-then-join (`uze-keys` `JoinCheckout`, the Work view, its tests).

## 7. Pool by idle age

- [x] 7.1 Remove `spare` from `WorktreePolicy`, `Pool` and `project-files.mdx`/`agents.mdx` in the same step (`declaration.rs` checks the docs against the struct); `trim_free_slots` keeps a free slot until it has gone unstamped for `idle_days`, keeping the index warm-up.
- [x] 7.2 In `collect`, trim directories before pruning branches.
- [x] 7.3 Move `carry_changes` to the plumbing diff (`diff-index -p --binary --full-index --no-renames HEAD`, the working tree against `HEAD`).

## 8. Everything else that says "parked" or "spare"

- [x] 8.1 Existing tests that assert the old behavior, rewritten in the step that changes it: `tests/acceptance/engine.rs`, `checkout/tests.rs`, `checkout/accounting_tests.rs`, `src/ui/orchestrator/tests/work.rs`, and the `invariants.md` citations that name them in the same step.
- [x] 8.2 The client: unfinished work shown with label and branch (no Git read for the list); one aggregated toast, title and detail, when a pass shelves work; a pinned checkout with its reason in the checkouts view.
- [x] 8.3 Docs: `agents.mdx` ("Work nobody finished"), `project-files.mdx`, `docs/observability.md` and the `occupancy.rs` log line, the `uze:worktree` skill (`plugins/uze/skills/worktree/SKILL.md`), AGENTS.md "What UZE persists" (`refs/uze/shelf/`), and the mirror-push note.
- [x] 8.4 Redraw `docs/architecture/agent-lifecycle.mmd` and `checkout-ownership.mmd`; `cargo test -p uze-extensions` and `uze agent artifacts check`.
- [x] 8.5 `journeys/suites/06-recovery/07-work-the-closing-did-not-take.yml` checks a shelf instead of a parked directory.

## 9. Gates and validation

- [x] 9.1 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace --no-fail-fast`, `openspec validate --all --strict`, and all of `slot_lifecycle` green.
- [ ] 9.2 Validate in a sandbox (`make playground-linux`): a space used by up to four agents across sessions, some ending with uncommitted work, some with commits, one resumed in a different checkout, one in its own; the number of directories never exceeds the peak.
- [ ] 9.3 After the operator's own validation by hand, a journey in `04-workspace` proving a dirty agent's close leaves its changes in `refs/uze/shelf/*` and the next agent reuses the directory, checked against Git and the filesystem.
