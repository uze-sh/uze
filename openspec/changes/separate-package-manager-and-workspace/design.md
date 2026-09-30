## Context

See proposal.md for why. The facts that shape the approach:

- `uze-core` is organized into five concerns (`package`, `capability`,
  `delivery`, `project`, `machine`), re-exported flat. `project/` holds both
  modules' state: `manifest`, `project_lock`, `context`, `project_context`,
  `text_region`, `project_root` and `workspace` (the `agents.yaml` anchor)
  belong to the package manager; `worktree`, `checkout`, `task`, `landing`
  and `conversation` belong to the workspace. `delivery/continuity.rs` is
  workspace state filed under package delivery.
- `manifest::load` parses one `ProjectManifest` (`deny_unknown_fields`) and,
  when `worktrees:` is present, runs `git check-ignore` over its `link`
  entries for every caller.
- `SCAFFOLD` in `manifest.rs` writes `worktrees: completion: handoff` live;
  `ensure_exists` (on `uze install`) and `declare_plugin` both write it.
- Context reconciliation (`uze-application`'s `context.rs`) owns three kinds
  of region in `AGENTS.md`: the bridge, package instruction contributions,
  and the workspace policy region; `project_environment.rs`'s drift counts
  all of them. The plugin-authoring text lives inside the policy region's
  naming clause.
- `uze setup` creates `~/.uze/shims/<harness>` and writes a marked block
  into the shell rc (`machine/shell_path.rs`,
  `application/runtime_shim.rs`). The shim adds the integration's
  `runtime_contribution` on every launch and session continuity when the
  launch carries an agent identity. Menu launches already call the shim by
  absolute path (`services/tasks.rs`); what the rc block adds is a harness
  typed into a pane, or started by another process, reaching the shim.
- The shim already stamps `UZE_SHIM_NAME`/`UZE_SHIM_PID` on the process it
  execs, and `uze-terminal` already reads the foreground process's
  environment (`/proc` on Linux, `KERN_PROCARGS2` on macOS) to name the
  pane's foreground. The client sees only the name, so "claude through the
  shim" and "bare claude" look the same to it.
- `uze doctor`, `status` and the client decide whether context reaches a
  harness partly by walking the operator's `PATH` for the shims directory
  (`doctor.rs` `runtime_shim_is_active`, `read_models.rs`
  `RuntimeProjection::of` giving `ShimShadowed`, `agent_context.rs`).
- Isolated checkouts already treat a region UZE re-projected into their
  `AGENTS.md` as derived, not as work (`checkout` accounting,
  `a_reprojected_region_leaves_the_slot_free`).
- `uze-terminal` depends on nothing in the workspace but `uze-document`; the
  pane environment is composed by the client (`Launch::env`) and applied by
  the server.

## Goals / Non-Goals

**Goals:**
- A package-only user never meets the workspace: no policy, no region, no
  failure, no shell edit, no instruction that refuses.
- The workspace is self-sufficient: it does not rely on the person's shell
  to reach its shims.
- The dependency direction is checked by the build.

**Non-Goals:**
- Splitting `IntegrationPort` into per-module traits (already rejected in
  AGENTS.md; this change only groups and documents).
- Splitting `uze-application`, `uze-integrations` or `uze-terminal`. The one
  new crate is `uze-workspace`; `uze-terminal` only gains a protocol field
  (D8).
- Delivering the workspace's instructions at runtime (a fixed line in
  `AGENTS.md` pointing at a command, or injection at launch) instead of a
  projected region. Considered and deferred: it trades a region that is
  always in context for one an agent has to fetch, and the subagents the
  region exists for are the ones least likely to fetch it.
- Moving the `.agents/` projection for Claude Code into a native, persistent
  form. It stays a launch-time contribution, now scoped to the workspace.
- Changing `agents.lock`, which carries nothing from the workspace.

## Decisions

**D1. The workspace's domain is its own crate, `uze-workspace`.** It
depends on `uze-core` and `uze-git`; `uze-core` does not depend on it, so the
package manager cannot name the workspace and `rustc` is what says so. The
reverse coupling today is three points, each resolved before the move:
`ProjectManifest` holding `WorktreePolicy` (D3), `IntegrationPort` naming
`conversation::SessionId` (the identifier stays in `uze-core` as a shared
type, so `uze-integrations` does not depend on the new crate), and the task
variants of `uze-core`'s error enum, which on inspection carry only text
and so create no dependency: they stay in the shared error, which the
workspace's `Result` keeps using. Alternative considered: a `workspace` concern inside `uze-core`
held by a name-matching test. Rejected: a test that looks for names needs a
`sanctioned` list and misses a type re-exported under another name, and the
reason the presentation seam is a test (the binary crate shared with
`src/shim.rs`) does not apply to `uze-core`.

`uze-core` remains the shared foundation plus the package manager's domain;
splitting those apart would not serve this rule. `uze-application` stays one
crate because it orchestrates both modules; there the rule is a test, and a
precise one: its package-manager files never name `uze_workspace`.
`uze-terminal` stays unrelated (it only gains a protocol field, D8): it owns
pseudoterminals and knows nothing of agents or Git, and `uze-workspace`
knows nothing of terminals.

**D2. What moves, and what is renamed.** `worktree`, `checkout`, `task`,
`landing`, `conversation` (except `SessionId`, including `Claim`, which the
shim uses) and `delivery/continuity` move into `uze-workspace`, while the
task and `AgentPlacement` variants of `uze-core`'s error stay (text only), with the helpers they call from `uze-core`
(`persistence`, `digest`, `subprocess`, `record`, `text_region`,
`project_lock`, `project_context`, `process_cwd`) made public where they are
not. The TUI-state modules at `uze-core`'s root (`prompt_history`,
`client_layout`, `notifications`, `extensions`) are workspace state too and
move with them. `project/workspace.rs` (the directory holding `agents.yaml`
or `marketplace.json`) is renamed `anchor`, because "workspace" now names a
module. In `uze-application`, the "checkout" that means a linked
marketplace working copy (`project_environment.rs`) is renamed
`linked_source`, and `agent_context` and the package half of
`overview::summary` move from the `Workspace` facade to `Context` and
`Project`; prompt history, which the summary also carries, stays with the
workspace. The move changes no behavior: `load` keeps its `check-ignore`
until D3 moves it, error text and exit codes stay byte-identical, and the
layering rule that gives the agent identity variable one owner is extended
to the new crate.

**D3. `agents.yaml` is one file with sections by owner.** The manifest
layer keeps the file, the scaffold and the list of known top-level keys, so
a misspelled key is still refused. Each section is parsed by its owner:
the package manager reads `marketplaces`, the workspace reads `worktrees`
and `artifacts`. `load` no longer validates `worktrees.link`; the
workspace's policy read (`services/tasks.rs` `policy()`) does, and reports
through the workspace. Alternative: two files. Rejected: the file is
authored by a person and one file is what they know.

**D4. The scaffold declares only what the package manager needs.** The
`worktrees:` block is written fully commented, with its defaults. The
workspace's policy popup already writes a live value
(`set_completion`), which is the only way a policy appears besides an edit.
Projects that already carry the live `completion: handoff` keep it: the file
is theirs now, and no migration rewrites an authored file.

**D5. Regions have owners, and the workspace keeps its own in step.**
Package-manager reconciliation (`install`, `update`, `agent context
reconcile`, `status` drift) handles the bridge, package instruction
contributions, and a new `project:plugin-authoring` region carrying the
authoring verbs, written for every project with an `agents.yaml`; it never
writes, removes or counts the policy region. The policy region's converge,
supersede and stale logic, and `WorktreePolicyStatus`, move to the
workspace half of `uze-application`, and `ContextReport` loses its
`worktrees` field.

While it runs, the workspace keeps the region in step itself, through a
`spawn_*`/`absorb_*` pair like every other repository touch in the client:
when a space opens on a project (the point where the client first learns a
root, `unread_named_directories`); when the declaration changes, from the
popup or from an edit to `agents.yaml` that the client notices by the
file's digest on the existing 20-second `TASK_REFRESH` clock, inside the
spawned thread (no file-watching dependency); and before an agent starts
in the primary checkout, inside the existing `spawn_agent_placement`
thread, only when the placement's root is the primary checkout (an agent
"in place" in a pane standing in a slot is not). It keeps the region only
in an `AGENTS.md` the project has and never creates the file: a tracked
file appearing in the operator's checkout because a screen opened is not
the workspace's call, and the journeys that hold the operator's checkout
untouched caught exactly that; `uze install` creates it, as it always has.
It writes only the
primary checkout: in a slot, `AGENTS.md` is part of the agent's branch, and
a region written there becomes a change the agent can commit and deliver,
colliding with the same change still uncommitted in the primary checkout.
An isolated agent reads the region its branch was cut with. Slots are cut
from the local target branch after a fast-forward-only sync, so the
operator brings the region forward by committing the primary checkout's
`agents.yaml` and `AGENTS.md` on that branch (the declared target, else the
primary's branch); with `pr` completion, delivery rebases onto the remote's
target, so the commit is pushed too. The popup already presents this as a
versioned edit. `Isolate` copies the primary's `git diff HEAD` into the new
slot (`checkout::carry_changes`); when `AGENTS.md` differs from `HEAD` only
inside managed regions (`text_region::same_outside_managed_regions`, which
slot accounting already uses), the slot's `AGENTS.md` is restored from
`HEAD` after the copy, so the synced region never rides into a branch.
Alternatives: writing the slot and having delivery drop region-only
changes (touches landing, the most sensitive path in the workspace), or
telling the agent not to commit the file (relies on the model). Both owners of `AGENTS.md` take a per-project guard on the file, a new lock
in `uze-core`'s shared foundation (the package manager may name it; the
machine-wide `MutationLock` is not reused, being non-reentrant and held
across whole installs), because `text_region::converge` is a whole-file
read-modify-write and two owners interleaving would drop a region until the
next run. Writes render deterministically, so a sync with nothing new
writes nothing. A region edited by hand is reported once per client session
and region identity, and is never overwritten or removed, since removal is
structural and a drifted region fails it. Two clients on one project are
benign: identical bytes, atomic writes. Removing the region when the
declaration is gone is `converge` with an empty desired set, which today is
skipped when there is no policy. When the
workspace is not running nothing syncs, and the region catches up the next
time it opens. The popup names both files it changes and no longer asks for
a reconciliation.

The policy region's text opens by saying it applies to agents the
workspace launched. Alternative for authoring: drop the region and rely on
the `uze:author` skill. Rejected because `plugin-authoring` requires the
verbs to be documented in `AGENTS.md`, and a skill is only offered, never
guaranteed to be read.

**D6. The shim belongs to the workspace.** `uze setup` still creates the
shims under `~/.uze/shims` (generated tier) but never writes a shell file.
The client composes every pane's `Launch` env with the shims directory
prepended to `PATH`, which is what keeps a harness typed into a pane, or
started by the agent itself, going through the shim; menu launches already
use the shim's absolute path. The shim itself does not change: outside the
workspace nothing calls it. `uze setup` removes the marked block an earlier
build wrote, through the same whole-line marker check `shell_path` already
uses, and leaves any file whose markers do not verify untouched and
reported. That removal exists only to take back what UZE wrote. Its exit is
a test that fails once the workspace version reaches 1.0.0, when the
removal is deleted with it.

**D7. What only the workspace delivers is reported as such, without
looking at `PATH`.** The `PATH` walk (`runtime_shim_is_active`) and the
`ShimShadowed` state it produces are removed: after D6 they would read as
shadowed on every machine. A project resource that reaches a harness only
through its `runtime_contribution` is reported by `inspect`, `status`,
`doctor` and the client's agent support with the existing adapted route and
the detail "inside the workspace", derived from the integration alone.

**D8. The server says whether a pane's harness came through the shim.**
The server already names a pane's foreground from the shim's stamp when it
is present. It adds one field to the pane status in the versioned client
protocol: whether that name came from the stamp. The client shows one toast
per pane when a harness the registry knows is in the foreground without it.
No new marker and no second reader of process environments: the server
reads them on Linux and macOS alike.

**D9. Workspace skills say when they apply.** `uze:worktree` and
`uze:architect` open with their condition (an agent the workspace
launched; the architect surface is optional and `uze agent artifacts
check` works without it). The `uze` plugin keeps shipping all four skills.
Alternative: a separate workspace plugin installed when the workspace first
opens. Rejected for this change: it adds a second default plugin and an
install triggered by opening a screen, for a problem the text already
solves.

**D10. The architecture artifacts follow the split.** Every diagram
under `docs/architecture/` is reviewed, because two changes reach them: the
move (box links name files that move to `uze-workspace`, and `make
artifacts` fails on a link that opens nothing) and the shim scope
(`containers.mmd` draws the shim as run from the developer's `PATH`).
`crate-layering.mmd` gains `uze-workspace`; `containers.mmd` gains the
workspace domain and redraws the shim; `core-components.mmd` narrows
`project`; a new `workspace-components.mmd` draws the new crate;
`system-context.mmd`, `agent-lifecycle.mmd` and `subagent-checkouts.mmd`
show the modules and the launch path apart; the install and attachment
diagrams are checked to carry no workspace step; `invariants.md` gains the
boundary and updates the shim and moved test paths. Link fixes land with
the move so the gate never goes red; content changes land after the
behavior they describe. AGENTS.md's Workspace layout and Architecture
sections name the new crate and its direction.

**D11. Tests are organized by the profile a person lives in.** Beside the
module-level tests, two world builders in `uze-testkit` (package only;
package and workspace) back an L3 suite per profile, a journey per profile,
and a Lab contract for the hand-started harness. Where the two profiles meet
(a project moving from one to the other) the rule is the ownership rule of
D5: the package manager leaves the workspace region alone even when the
policy is gone, and the workspace removes it the next time it reconciles.
`tests/workspace/`, which tests the package manager's anchor, is renamed so
the word keeps one meaning in the suite too.

## Candidate ADRs

- The workspace's domain is a crate of its own above `uze-core`, so the
  package manager cannot depend on it: it decides where every future module
  goes, and a crate boundary is costly to move back.
- The runtime shim is scoped to the workspace and UZE never edits shell
  startup files: it reverses a delivery route and is costly to reintroduce.

## Risks / Trade-offs

- [Project skills under `.agents/` stop reaching Claude Code started by
  hand] → reported by `inspect`/`status` (D7) and documented; plugins are
  unaffected.
- [A pane's shell rc (mise, asdf, nvm) prepends ahead of the shims] → menu
  launches use the absolute path; typed launches are detected and stated
  (D8).
- [The workspace writes a tracked file in the operator's checkout] → only
  when the rendered region differs, which is only after the declaration
  changed; the popup says so before the first write.
- [Existing projects keep a live `completion: handoff` and keep getting the
  workspace region] → the region now says it only applies to launched
  agents; commenting the key opts out, stated in the docs.
- [Removing the rc block edits a person's file] → only a block whose markers
  verify, byte-for-byte outside it, the same guarantee used to write it.
- [The crate extraction ripples through `uze-application`, `src/` and the
  tests] → the moves land as their own commits with no behavior change,
  before the behavior changes that depend on them; `uze-core` keeps no
  re-export of what moved, so a stale import fails to compile rather than
  drifting.
- [Journeys and the Lab assert that a harness command resolves to the shim
  in the person's shell] → those checks move inside a workspace pane.

## Migration Plan

1. Resolve the three reverse couplings, then extract `uze-workspace` and
   rename (D1, D2, D3 parsing, D10), with no behavior change, gated by the
   existing suites.
2. Behavior for package-only projects (D3 validation, D4, D5).
3. The shim scope (D6, D7, D8), with the rc block removal.
4. Surfaces and docs (D9, the Skills page, shim docs, `uze agent` and
   `uze config` help).
5. The `uze-application` rule last, when the code already satisfies it.

Rollback is per step; step 3 is the one that changes what a person's shell
does, and reverting it restores the rc block on the next `uze setup`.
