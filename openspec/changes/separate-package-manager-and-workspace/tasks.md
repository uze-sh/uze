## 1. Extract `uze-workspace`, no behavior change

- [x] 1.1 Parse `agents.yaml`'s `worktrees` section outside `ProjectManifest` so `uze-core`'s manifest no longer names `WorktreePolicy` (the parsing half of 2.1); `load` keeps its `check-ignore` until 2.2, and `set_completion` keeps re-validating through it
- [x] 1.2 Keep `SessionId` in `uze-core` as a shared type outside `conversation`, and point `IntegrationPort` at it
- [x] 1.3 Keep the task and `AgentPlacement` variants in `uze-core`'s shared error: they carry only text, so they create no dependency on the workspace, and a separate error type would re-type every `Result` in `uze-workspace` and `uze-application` for no boundary gained (design D1)
- [x] 1.4 Create `crates/uze-workspace` (workspace member, inherited version, no external dependency beyond what the moved code already uses) and move `worktree`, `checkout`, `task`, `landing`, `conversation` (with `Claim`), `delivery/continuity`, `prompt_history`, `client_layout`, `notifications` and `extensions` into it; make public the `uze-core` helpers they call
- [x] 1.4a Extend the layering rule that gives the agent identity variable one owner (`tests/architecture/layering.rs`) to `crates/uze-workspace`, and repoint intra-doc links that named moved items (`persistence.rs`'s link to `task::locked`)
- [x] 1.5 Rename `project/workspace.rs` (the `agents.yaml`/`marketplace.json` anchor) to `anchor` and update every caller
- [x] 1.6 Update `uze-application`, `src/`, `uze-testkit` and `tests/` imports; `uze-core` keeps no re-export of what moved
- [x] 1.7 Rename the linked-marketplace "checkout" in `project_environment.rs` to `linked_source`, and move `agent_context` and the package half of `overview::summary` off the `Workspace` facade onto `Context` and `Project`, leaving prompt history on `Workspace`
- [x] 1.8 Group `IntegrationPort`'s workspace-only methods under one documented section, no signature change
- [x] 1.9 Repoint every box link and every test reference in `docs/architecture/` (`checkout-ownership.mmd`, `agent-lifecycle.mmd`, `subagent-checkouts.mmd`, `invariants.md`) at the files' new paths in `crates/uze-workspace`, so `make artifacts` stays green after the move (the content changes are group 8)
- [x] 1.10 Update AGENTS.md (Workspace layout, Architecture diagram and rules) for `uze-workspace` and its direction
- [x] 1.11 Check that `cargo tree -p uze-core` does not list `uze-workspace`; run `cargo test --workspace --no-fail-fast` and `cargo clippy --all-targets -- -D warnings`; nothing behaves differently

## 2. agents.yaml sections by owner

- [x] 2.1 Finish splitting `manifest` parsing: the manifest layer keeps the file, the scaffold and the known top-level keys (a misspelled key is still refused); the package manager parses `marketplaces`, the workspace parses `worktrees` and `artifacts`
- [x] 2.2 Move the `worktrees.link` Git check out of `manifest::load` into the workspace's policy read, reported through the workspace
- [x] 2.3 Write `SCAFFOLD`'s `worktrees:` block fully commented, defaults shown; keep `artifacts:` commented as `architect-surface` requires
- [x] 2.4 Confirm the policy popup's `set_completion` writes a live `worktrees:` value into a scaffold whose block is commented
- [x] 2.5 Tests: a first install declares no policy; a non-ignored `worktrees.link` and a non-Git directory do not fail `install`/`status`; choosing a completion makes the policy live (`module-boundary` scenarios)

## 3. AGENTS.md regions by owner, and the workspace keeps its own in step

- [x] 3.1 Add the package manager's `project:plugin-authoring` region carrying the authoring verbs, reconciled for every project with an `agents.yaml`, and remove the authoring bullet from the policy region's naming clause
- [x] 3.2 Open the policy region with the statement that it applies to agents the workspace launched and can be ignored otherwise, before any instruction
- [x] 3.3 Move the policy region's converge, supersede and stale logic and `WorktreePolicyStatus` from `context.rs`, `read_models.rs` and `project_environment.rs` into the workspace half of `uze-application`; drop `worktrees` from `ContextReport`; package-manager reconciliation (`install`, `update`, `agent context reconcile`) and `status` drift neither write, remove nor count the region
- [x] 3.4a Add a per-project guard on `AGENTS.md` to `uze-core`'s shared foundation, taken by package-manager reconciliation and by the workspace sync (never `MutationLock`)
- [x] 3.4 Add the client's region sync as a `spawn_*`/`absorb_*` pair: when a space's root is first learned (`unread_named_directories`), when the popup changes the policy, and when `agents.yaml`'s digest changes on the `TASK_REFRESH` clock, the digest read inside the spawned thread; writes take the guard and write nothing when the rendered region is unchanged
- [x] 3.5 Sync the region inside `spawn_agent_placement` when the placement's root is the primary checkout (`worktree::primary_checkout(root) == root`), before the agent starts; never write any slot
- [x] 3.5a After `checkout::carry_changes` on `Isolate`, restore the slot's `AGENTS.md` from `HEAD` when the primary's differs from `HEAD` only inside managed regions; every other change, `agents.yaml` included, is carried as before
- [x] 3.6 Report a hand-edited region once per client session and region identity as a toast, never overwriting or removing it; remove a region an earlier policy left when the declaration is gone (`converge` with an empty desired set, no longer skipped when there is no policy)
- [x] 3.7 Change the popup's notice to name `agents.yaml` and `AGENTS.md` as the files it changes, without asking for a reconciliation (no client surface calls `set_completion` today, so there is no notice to change; the service it would call now keeps `AGENTS.md` in step itself, and the requirement stands for the popup when it returns)
- [x] 3.8 Tests for the `worktree-policy` and `plugin-authoring` scenarios: an edit to `agents.yaml` reaches the file (an L3 test of the application's sync; the refresh tick's call is covered by the journey in 9.7 rather than a `TestBackend` test, since the tick spawns a thread the backend does not run); isolating with a synced region leaves the slot's `AGENTS.md` as committed; two owners converging on one file keep both regions; an agent in place reads the current region; no slot's `AGENTS.md` is ever written by the sync, placed or running; an isolated agent placed after the operator committed the change reads the current region; a stale region leaves `uze status` clean; an agent started by hand reads the region's opening statement
- [x] 3.9 Reconcile this repository's own `AGENTS.md` regions so they match the new rendering

## 4. The shim belongs to the workspace

- [x] 4.1 Stop `uze setup` from writing the shell rc: remove the `ensure_path_line` path from `runtime_shim.rs` and its report lines, keep creating the shims
- [x] 4.2 Make `uze setup` remove a verified `# >>> uze shims path >>>` block and report it, leaving a file whose markers do not verify untouched and reported; add the test that fails once the workspace version reaches 1.0.0, when the removal and the test are deleted
- [x] 4.3 Compose every pane's `Launch` env with the shims directory prepended to `PATH` in the client; confirm menu launches keep using the shim's absolute path (`services/tasks.rs`)
- [x] 4.4 Add to the pane status in the versioned client protocol whether the foreground name came from the shim's stamp, decided by the server from the environment it already reads (Linux and macOS); bump the protocol and keep an older client's decoding rule
- [x] 4.5 In the client, show one toast per pane when a harness the registry knows is in the foreground without the stamp, saying that session resume and project resources are lost for it
- [x] 4.6 Remove the `PATH` walk (`runtime_shim_is_active` in `doctor.rs`) and the `ShimShadowed` state (`read_models.rs` `RuntimeProjection`), and update `agent_context.rs` and `src/ui/agent_support.rs`; `uze doctor` stops reporting a shadowed shim
- [x] 4.7 Report project resources that reach a harness only through `runtime_contribution` as adapted, "inside the workspace", in `uze inspect`, `uze status`, `uze doctor` and the client's agent support
- [x] 4.8 Tests: `uze setup` leaves shell files unchanged and removes an earlier block byte-exactly; a typed harness in a pane reaches the shim regardless of the outer `PATH`; continuity still holds with an untouched operator `PATH`; the pane status carries the stamp flag; `cli-performance`'s scenario that no command walks `PATH` for the shim
- [x] 4.9 Journeys: invert `01-first-run/01`'s check that the shell rc contains the shims into "the shell rc is unchanged", and add a pane-level check that a typed harness resolves to the shim (the pane-level check lands with the journey in 9.7); the Lab already prepends the shims itself and needs no change here

## 5. Surfaces that name the wrong module

- [x] 5.1 Open `uze:worktree`'s SKILL.md with its condition (an agent the workspace launched) and reconcile its naming guidance with the policy region's text
- [x] 5.2 Reframe `uze:architect` around the diagram files and `uze agent artifacts check`, with the workspace's surface as optional
- [x] 5.3 Fix `init` SKILL.md's reference to the nonexistent `uze list`
- [x] 5.4 Reword the hidden `uze agent` help: only `work` needs an agent the workspace launched
- [x] 5.5 Label `uze config notification` and `uze config extension` as workspace settings in the CLI help and `reference/cli.mdx`

## 6. Documentation

- [x] 6.1 Rewrite `workspace/terminal.mdx`'s shim section for the workspace-scoped shim and the bypass notice; fix `reference/glossary.mdx` and `reference/harnesses.mdx`
- [x] 6.2 Drop the rc-block step from `installation.mdx`'s removal steps and explain that `uze setup` takes an earlier block back
- [x] 6.3 Say in the package manager's docs which project resources reach Claude Code and Codex only inside the workspace
- [x] 6.4 Move the Skills page to Reference, and have each module page name its own skill (AGENTS.md → `uze:init`, Authoring → `uze:author`, Agents → `uze:worktree`, Extensions → `uze:architect`), with a redirect from `/docs/plugins/skills`
- [x] 6.5 Say in `workspace/agents.mdx` and `reference/project-files.mdx` that a project has no isolation policy until one is chosen, and how to opt out by commenting the key

## 7. Hold the boundary

- [x] 7.1 Add the architecture rule for `uze-application`: its package-manager files (`lifecycle/`, `project_environment`, `context`, `marketplace*`, `authoring`, `freshness`, the package read models) never name `uze_workspace`, with any exception listed with its reason
- [x] 7.2 Add the invariants to `docs/architecture/invariants.md`: `uze-core` does not depend on `uze-workspace` (the manifest), and the `uze-application` rule (the test)
- [x] 7.3 Run the full gate: `cargo test --workspace --no-fail-fast`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `cargo deny check`, `make artifacts`, `openspec validate --all --strict`

## 8. Architecture artifacts

- [x] 8.1 `crate-layering.mmd`: add `uze-workspace` between `uze-application` and `uze-core`, with the edges it has (`uze-core`, `uze-git`) and none from `uze-core` to it; keep `uze-terminal` apart
- [x] 8.2 `containers.mmd`: add the workspace domain container; narrow `Core` to the shared foundation and the package manager; redraw the shim as the workspace's (launched by the TUI and the terminal server through the pane's `PATH`, no longer "Runs, from PATH" from the developer's shell)
- [x] 8.3 `core-components.mmd`: narrow `project` to what the package manager declares and writes; move `delivery`'s continuity out; add the `anchor` rename
- [x] 8.4 New `workspace-components.mmd` for `uze-workspace` (worktree, checkout, task, landing, conversation, continuity, the TUI state modules), each box linked to its file
- [x] 8.5 `system-context.mmd`: show the two modules' relations apart (the developer installs plugins through the package manager and runs agents through the workspace; the agent's `uze agent work` relation belongs to the workspace)
- [x] 8.6 `agent-lifecycle.mmd` and `subagent-checkouts.mmd`: a launch goes through the shim by absolute path from the workspace; the workspace region is reconciled at launch
- [x] 8.7 `install-pipeline.mmd`, `install-sequence.mmd` and `attachment-lifecycle.mmd`: confirm they carry no workspace step (policy region, `worktrees.link` check) after groups 2 and 3, and remove any
- [x] 8.8 `invariants.md`: add the module-boundary invariants (tasks 7.2), update the shim invariants for its workspace scope and "no shell file is edited", and update every test path that moved
- [x] 8.9 Run `cargo test -p uze-extensions` and `make artifacts`; every diagram draws and every box link opens a file

## 9. Tests by usage profile

Each behavior group above lands with its own tests; this group adds the two
profiles a person actually uses, end to end, so the boundary is proven the
way it is lived and not only module by module.

- [x] 9.1 Add two world builders to `uze-testkit`'s `scenario`: *package only* (a project whose `agents.yaml` came from a first install, harness stand-ins on `PATH`, no shims on `PATH`, nothing launched by the workspace) and *package and workspace* (the same project, a chosen policy, agents launched through the workspace's launch path); both seed the shell startup files so any edit is detectable
- [x] 9.2 L3 `tests/acceptance/package_only.rs`: `setup`, `install`, `update`, `remove`, `status` with a harness started by hand; the plugin reaches the harness natively; `agents.yaml` has no live `worktrees:`; `AGENTS.md` carries the bridge, plugin and authoring regions and no workspace region; no `.worktrees/`, no task, checkout or conversation record; no shell file changed; the harness command resolves to the harness's own binary; `inspect` reports `.agents/` resources as reaching Claude Code and Codex inside the workspace; a non-ignored `worktrees.link` and a non-Git directory fail nothing
- [x] 9.3 L3 `tests/acceptance/package_and_workspace.rs`: the same project after choosing a policy; the workspace region appears and addresses launched agents; a stale workspace region leaves `install` and `status` clean and is reported by the workspace; an agent launched by the workspace goes through the shim by absolute path and keeps its conversation across a relaunch with an untouched operator `PATH`; `uze agent work name` works for the launched agent and refuses a hand-started one with the documented message; the authoring verbs work for both
- [x] 9.4 L3: moving between profiles; a package-only project that starts using the workspace keeps `agents.lock` and its plugins byte-identical; a project whose policy is commented out again loses the workspace region the next time the workspace reconciles, and the package manager never removes it on its own
- [x] 9.5 A guard over the whole package-only lifecycle that no path outside `$UZE_HOME` and the project changes (shell files, harness configs beyond receipts), so a future shell edit fails a test by name
- [x] 9.6 Journey `02-packages/…-a-project-that-only-uses-the-package-manager.yml` (tag `gate`): real CLI, a harness started from a plain shell; checks read the shell files, `AGENTS.md`, the project tree and `command -v` of the harness, never UZE's report
- [x] 9.7 Journey `04-workspace/…-the-workspace-on-a-project-with-plugins.yml` (tag `gate`): open the workspace on a project that already has plugins, launch an agent from the menu, type the harness in a pane; checks: the pane's harness resolves to the shim, the workspace region is in `AGENTS.md`, `agents.lock` unchanged
- [x] 9.8 Lab: a contract check that a plugin reaches the model with the harness started with no UZE shim on `PATH` (the Lab prepends the shims today, so this check runs without the prepend), for every harness; the project `.agents/` resource checks run through the workspace's launch path, and a harness that cannot deliver one declares it through `bindings.unsupported`
- [x] 9.9 Rename `tests/workspace/` (agents.lock consumer, marketplace, root resolution: the package manager's anchor, not the workspace) to `tests/project/`, and add the two usage profiles to `tests/README.md` as a row of its coverage map
- [x] 9.10 Update, not delete, the existing tests that assumed a live scaffold policy, the policy region in package reconciliation, or an rc edit: each keeps its claim under the profile it belongs to

