## Why

uze is two tools in one binary: a package manager that delivers plugins and
one `AGENTS.md` to every harness, and a workspace that runs agents side by
side in checkouts of their own. The documentation and the CLI now present
them as independent, but the code does not keep them apart: the first
`uze install` in a project turns on the workspace's isolation policy, a
workspace setting can fail a package install, every agent a person starts by
hand is told to run a command that only works inside the workspace, and
`uze setup` edits the person's shell startup file to put a process wrapper
ahead of every harness, which `transparent-harness-attachment` already
forbids. Someone who only wants the package manager pays for the workspace
without asking for it.

The rule this change adopts is one-way: **the workspace may depend on the
package manager; the package manager never depends on the workspace.**

## What Changes

- **BREAKING** The `agents.yaml` UZE scaffolds declares nothing about the
  workspace: its `worktrees:` block is written entirely commented out. A
  project only carries an isolation policy once somebody chooses one (the
  workspace's policy popup, or an edit).
- Reading `agents.yaml` for a package-manager command no longer checks the
  workspace's `worktrees.link` entries with Git; that check moves to the
  workspace's own read of the policy, so a workspace setting can never fail
  `install`, `update`, `remove` or `status`.
- The package manager's drift (`install`, `status`) no longer counts the
  workspace's region of `AGENTS.md`; the workspace reports its own region.
- The workspace's region of `AGENTS.md` addresses only agents the workspace
  launched, and says so before its first instruction. It no longer carries
  plugin authoring. While the workspace runs it keeps that region in step by
  itself in the primary checkout: when a space opens, when the policy
  changes (from the popup or an edit to `agents.yaml`), and before an agent
  starts there. Isolated checkouts read the region their branch was cut
  with.
- Plugin authoring (`uze agent market|plugin create|check`) is documented in
  a region the package manager owns, projected into every project UZE
  manages, whether or not it declares a workspace policy.
- **BREAKING** The runtime shim belongs to the workspace. `uze setup` stops
  editing shell startup files, and removes the block it wrote there before.
  The terminal server starts every pane with the shims directory first on
  `PATH`, and the workspace launches an agent through its shim by absolute
  path. Outside the workspace, a harness command is the harness's own binary.
- What reached a harness only through the shim (project-authored
  `.agents/skills` and `.agents/agents` for Claude Code, project agents for
  Codex) is reported by `uze inspect` and `uze status` as available inside
  the workspace, never silently missing. A pane whose shell put another
  entry ahead of the shims says so.
- The workspace's skills stop misleading agents the workspace did not launch:
  `uze:worktree` and `uze:architect` state their condition, the hidden
  `uze agent` help says only `work` needs a launched agent, and
  `uze config notification|extension` are labelled as workspace settings.
- Internal reorganization with no behavior change: the workspace's domain
  (tasks, checkouts, work naming, landing finished work, conversations and
  their continuity) moves out of `uze-core` into a new crate,
  `uze-workspace`, above it, so the package manager cannot depend on it;
  names that collide with the module names are changed, each module parses
  its own section of `agents.yaml`, package-manager read models leave the
  workspace facade, and `IntegrationPort` groups its workspace-only methods.
- In `uze-application`, which orchestrates both, an architecture test fails
  the build when a package-manager file names the workspace crate.

Already done on this branch and not part of this change: the documentation
split by module, `uze workspace`, and the flat root help.

## Capabilities

### New Capabilities
- `module-boundary`: the package manager and the workspace are separate
  modules with a one-way dependency; what a package-only project carries,
  what a workspace setting may affect, and the test that holds the rule.

### Modified Capabilities
- `worktree-policy`: the projected region is addressed only to agents the
  workspace launched, carries nothing about plugin authoring, and a project
  declares a policy only by choosing one.
- `plugin-authoring`: the authoring verbs are documented in a
  package-manager region projected into every managed project, not in the
  workspace's region.
- `transparent-harness-attachment`: `uze setup` never edits shell startup
  files, and outside the workspace a harness runs as its own binary.
- `terminal-runtime`: panes start with the shims first on `PATH`, launches
  go through the shim by absolute path, and a pane says when its shell
  reordered `PATH`.
- `cli-performance`: no command walks the operator's `PATH` to ask whether a
  shim is active.

## Impact

- New crate `crates/uze-workspace`; workspace members and `deny.toml` unaffected (no external dependency).
- `crates/uze-core`: `project/manifest.rs` (scaffold, `load`),
  `project/worktree.rs` (region text), `project/context.rs`, the `project/`
  and `delivery/` module layout, `delivery/integration.rs` (grouping),
  `machine/harness_runtime.rs`.
- `crates/uze-application`: `project_environment.rs` (drift),
  `context.rs` (regions), `runtime_shim.rs` and `setup.rs` (no shell rc),
  `services/tasks.rs` (policy read), `agent_context.rs`, `overview.rs`.
- `crates/uze-terminal` and `src/ui`: pane environment and launch path.
- `src/main.rs`: `uze agent` and `uze config` help, `inspect`/`status`/`doctor`
  reporting (the shadowed-shim check goes away).
- `plugins/uze/skills/{worktree,architect,init}`.
- `tests/architecture/layering.rs`, `tests/`, `journeys/` (checks that a
  harness command resolves to the shim move inside a pane), the Lab where it
  asserts on the shell-level shim.
- `web/content/docs` (shim, installation removal steps, Skills page moved to
  Reference), `docs/architecture/*.mmd` where the module split shows.
