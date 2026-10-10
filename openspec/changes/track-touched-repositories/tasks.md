## 1. Retire the riskiest assumptions (Lab and playground, before code)

- [x] 1.1 Launch-scoped observer injection measured on all four harnesses (`conformance/experiments/<vendor>/launch-hook.py`, 2026-10-10; results in design.md)
- [ ] 1.2 Windows: in `make playground-windows`, with a cross-built `uze.exe`, one file-tool write per harness through the generated launch file lands one JSONL line in the inbox (Claude via Git Bash/PowerShell, Codex `commandWindows`, Antigravity's shell)
- [ ] 1.3 OpenCode: a `--standalone` session is still listed by UZE's session read-back; otherwise prototype the `sessionID`-keyed inbox
- [ ] 1.4 Pre-image: add a pre-event variant to each experiment recording the file's hash before the write; confirm Codex's pre payload carries the full patch
- [ ] 1.5 trace2 classifier: collect the argv corpus the experiments print into a fixture; write the verb classifier as a test against it
- [ ] 1.6 Antigravity: detect the server-side hook gate at launch (`LAUNCH_HOOK_AUTH=apikey` and signed-out runs)
- [ ] 1.7 Turn the experiments into a contract check in `conformance/contract/` (outcome terms, no vendor name), one binding per vertical, with Unsupported declared through `bindings.unsupported`

## 2. The record and its paths

- [ ] 2.1 `UzeHome::trail_dir` and its children (`record.json`, `inbox/`, `git/`, `objects/`), added to the map `every_path_uze_owns_is_named_in_the_map` checks
- [ ] 2.2 `uze-workspace::trail`: the `Trail` record (`Shaped`, shape 1) — repositories with root, first-touched time, attribution class, evidenced paths and baselines; load/save under a per-agent lock
- [ ] 2.3 Fold: consume inbox JSONL and trace files, delete what was consumed, tolerate a torn last line, map paths to repository roots (nearest existing ancestor for a deleted file), ignore paths in no repository; unit tests for each
- [ ] 2.4 Discarding an agent removes its trail directory

## 3. Witnesses

- [ ] 3.1 Shim: when `owned_identity()` resolves, add `GIT_TRACE2_EVENT=<trail>/git` and `GIT_TRACE2_ENV_VARS=UZE_AGENT`, create the trail directories, pass `RuntimeContext.observe`
- [ ] 3.2 `GIT_TRACE2_EVENT` in `STAMPED_VARIABLES`; test that a plain pane started from inside an agent does not inherit it
- [ ] 3.3 `uze-core::delivery`: `EditObservation`, `observe_contribution`, `edit_paths` on `IntegrationPort`, Unsupported by default
- [ ] 3.4 `uze agent trail <harness> <pre|post>`: stdin → `edit_paths` → JSONL append; pre-image blobs via `GIT_OBJECT_DIRECTORY=<trail>/objects`; always exit 0; classified `Budgeted`
- [ ] 3.5 Claude: `--settings` contribution with pre/post groups calling the handler; payload reader
- [ ] 3.6 Codex: `-c hooks.*` plus `hooks.state` trust for the generated hook; `apply_patch` header reader; trail `git/` as a writable root under `workspace-write`
- [ ] 3.7 OpenCode: generated plugin directory and `OPENCODE_CONFIG_CONTENT`; scoping per the outcome of 1.3; Unsupported on a shared service
- [ ] 3.8 Antigravity: generated `--add-dir` directory holding only `.agents/hooks.json`; Unsupported when the gate is off or in `--print`
- [ ] 3.9 Report `EditObservation` per harness, with its reason, in the agent-support view

## 4. Measurement

- [ ] 4.1 `uze_git::read_with_env`
- [ ] 4.2 The trace2 verb classifier (from 1.5) and the touched/visited distinction
- [ ] 4.3 Measurement per attribution class: own (fork point), evidenced (pre-image blob via alternates, HEAD blob fallback, new file whole), git-nominated (HEAD + dirty blobs at first trace); untracked files counted
- [ ] 4.4 Tests for the spec's dirty-file, commit-elsewhere, shared-checkout, two-agents and nothing-written-into-the-repository scenarios

## 5. Application read model

- [ ] 5.1 `TouchedRepositoryView { root, display, own, attribution, summary }` and `touched_repositories(project, agent)` in `uze-application`, folding and measuring in one call
- [ ] 5.2 Keep `uze-core`/`uze-application`/`src/` vendor-neutral and `src/` off `uze_core::` (architecture suite green, no new `sanctioned` entry)

## 6. Sidebar

- [ ] 6.1 `spawn_touched_repositories` in `orchestrator/reads.rs` and `absorb_touched_repositories` keyed by agent id, paced, dropping answers for agents no longer shown; `Remembered::touched`
- [ ] 6.2 `draw_tree` child rows (home/other `Symbol`s, display name, `+N −M`, "since first git here" for git-nominated rows), the `+N repo` caption and the disclosure mark; `agent_rows` counts them
- [ ] 6.3 `WorkspaceHit::OpenTouchedRepository { tab, index }` and `ToggleTouchedFold(tab)`, pushed before the row-wide hit; drag grouping and `space_blocks` unaffected
- [ ] 6.4 Click: `open_code_at` takes the `ContentMode` and opens `Diff` at the repository root; select the agent's tab first when needed and open on the server's confirmation; report a repository that is gone
- [ ] 6.5 Fold set as an additive field in `client_layout`, unfolded by default
- [ ] 6.6 `TestBackend` tests: an agent with one sibling repository, an agent that stayed home (no level), a reverted repository (no counts), a git-nominated row, fold surviving a reattach, the click opening `Diff` without changing selection

## 7. Gate

- [ ] 7.1 `make check` (fmt, clippy `--all-targets`, workspace tests, deny, artifacts, ruff) green
- [ ] 7.2 Hand validation in a real workspace with an agent editing a sibling repository on each harness, before any journey is written
