## 1. The target's reach (uze-workspace, leaves first)

- [ ] 1.1 Tests in `checkout/tests.rs` with a remote ahead of the local target: a branch cut from the remote's tip with nothing of its own is integrated and holds no work; with one commit it is not integrated; once the remote carries it and a fetch ran, it is; diverged targets fall back to the local tip unless the branch descends from the remote's.
- [ ] 1.2 `checkout::target_reach` / `BranchTips::target_reach` (D1) with remembered ancestry per pair of commits (D6); move `REMOTE` into `checkout` and have `landing` use it.
- [ ] 1.3 `is_integrated` and `is_integrated_among` resolve a branch-named target through the reach (D2); `HEAD` and commits resolve as before.
- [ ] 1.4 `Refs::carries` takes the agent's branch and resolves the target through the reach (D3).

## 2. Landing reads the same target

- [ ] 2.1 Tests in `landing/tests.rs`: an agent cut from the remote's tip is ready with exactly its own commits; a refresh records the remote's tip as its base instead of the local one; an agent cut from a local target behind the remote counts only its own commits.
- [ ] 2.2 `effective_base` and `target_tip` (non-`pr`) read the reach (D3).
- [ ] 2.3 `TargetSync::Diverged`; `concern` speaks only for it (D5); sync tests updated: a checked-out target behind the remote has no concern, a diverged one does.

## 3. Placement

- [ ] 3.1 Tests in `crates/uze-application/tests/slot_lifecycle.rs`: New births on the remote's tip while the operator stands on a target behind it, records it as `base_commit` and is not counted as holding work; To worktree births on `HEAD`; a local target with commits the remote lacks keeps the local tip; no remote keeps the local tip.
- [ ] 3.2 `place_in_slot` cuts from `target_reach(primary, target, None)` (D4).
- [ ] 3.3 Placement unit test in `services/tasks/tests.rs` updated to the new rule.

## 4. Workspace client

- [ ] 4.1 Reword the toast in `src/ui/orchestrator/session/absorb.rs` and its test.

## 5. Documentation and gate

- [ ] 5.1 `docs/architecture/invariants.md`: rewrite "An agent is placed on the target as the last sync left it" and add "An agent's work is judged against the target it was cut from", each with the tests that prove it.
- [ ] 5.2 `docs/architecture/agent-lifecycle.mmd`: the refresh edge no longer says "the local target"; `uze agent artifacts check`.
- [ ] 5.3 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --test architecture`, `openspec validate --all --strict`, `cargo test --workspace --no-fail-fast`.
