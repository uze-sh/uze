## 1. Build script

- [x] 1.1 Read `CARGO_MANIFEST_DIR` at run time in `crates/uze-application/build.rs`; verify a renamed checkout of this repository builds without `cargo clean`

## 2. Conformance first

- [x] 2.1 Apply the continuity scaffolding (relaunch `between` step, per-scene markers in the four providers, `relaunch_in(..., between)` in the bindings) and a scene that removes the task's checkout between the two processes and adds it again at the same path on the same branch
- [x] 2.2 Run `lab.py --contract continuity` on all four harnesses and record that the same-path scene passes (it proves the harnesses, independent of the pool)

## 3. The record carries a move

- [x] 3.1 Add `moving_to` to `CheckoutRecord` (optional, additive); `record::read` accepts a mismatched path only when it equals `moving_to`; tests, including that a copied checkout is still not taken
- [x] 3.2 Run `git worktree repair` before every `git worktree prune` the workspace issues; test a move interrupted between Git's rename and the record rewrite

## 4. Pool and placement, one green step

- [x] 4.1 Thread the agent's own path (checkout, else last_checkout, else id) through `checkout::take`/`acquire`/`resume`, `place_new_agent`, `isolate`, `resume_task`, `resume_children` and `split_work`; `create` makes the checkout at that path; reword `CheckoutId`'s doc
- [x] 4.2 Move preconditions: Git refusals (submodule, lock), target exists, any process in the slot by the full process table; a slot set aside does not count against the cap; tests against real Git for each
- [x] 4.3 In `reuse`, intent → `git worktree move` → record rewrite → reset, under the repository lock; test the moved slot reads as the system's at its new path and keeps its ignored files
- [x] 4.4 Slot preference: a slot whose last agent will not come back, then an unowned generated name, then most recently used; test that a shelved agent's slot is taken last
- [x] 4.5 The agent's path taken by another checkout: place at a generated name and raise the toast saying the conversation could not be carried; test
- [x] 4.6 Keep release and collection free of renames; test that a freed slot keeps its agent's name and that resuming that agent moves nothing; test an agent placed by an earlier build resumes in its generated-name slot

## 5. Proof, journeys and docs

- [x] 5.1 Update journeys that read slot names (`04-workspace/01-agents-and-slots.yml`, `02-a-shell-opened-beside-an-agent.yml`, `05-delivery/01-handing-the-work-back.yml`) and add a `07-upgrade` scene where an older build meets agent-named checkouts
- [x] 5.2 Rebuild the Lab image and run the continuity contract on all four harnesses, and the full verticals
- [x] 5.3 Document in the workspace docs that `setup` runs whenever an agent takes a slot and is where path-bound caches are repaired, and that Antigravity asks to trust each agent's folder
- [x] 5.4 Update `docs/architecture/invariants.md` (the "identifier that never changes" entry and the copied-checkout entry), each tied to its test
- [x] 5.5 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace --no-fail-fast`, `openspec validate --all --strict`
