## Context

See proposal.md (Why). What the code holds today (2026-10-17):

- **Selecting a space.**
  - A click on a space's row is `WorkspaceHit::SelectSpace`, handled in
    `session/mouse.rs`. It closes any open extension and calls
    `land_on_space` (`session/keys.rs`), which focuses the space's own
    shell, or creates one with `CreateTab` when the space has only agent
    tabs.
  - The keyboard walk, `step_space`, goes through the same function.
  - The server opens a shell for every new space (`uze-terminal`
    `add_space`).
- **Every tab is a PTY.** `uze_terminal::Tab` owns a `Pane`, and there is
  no tab without a process. What is drawn in place of a pane is the
  extension surfaces (`code`, `architect`, `spec`). They are client state
  (`WorkspaceModel` fields plus `extension_tab`), drawn by
  `render_extension` into the pane's rect while the sidebar and the tab
  strip stay live, and fed by `spawn_*`/`absorb_*` pairs in
  `orchestrator/reads.rs` and `answers.rs`.
- **Read models that exist:**
  - `StatusReport` (`uze status`): instructions, drift, harnesses' context,
    issues, held-back hooks, agent-context shortfalls, and memory since
    `portable-agent-memory`. The TUI does not consume it today.
  - `CommandsAwaitingApproval`: read per project on open, answered through
    `approval.rs`'s dialog.
  - Preserved work: swept for recent.
  - Memory proposals: answered through `proposal.rs`'s dialog.
  - The worktree policy: `repository_context`.
- **The manage modal** (`model.manage`) is opened over the client against
  the selected space's root. Its Overview route still shows a project half
  (context state, plugins) beside the machine's.
- **The Overview read model** (`application/overview.rs`) names the
  `AGENTS.md` context state `MemoryState`, in a field `memory`.

## Goals / Non-Goals

**Goals:**
- One screen per space, answering "what is the state of this project" from
  read models that already exist, with no second computation of anything
  `uze status` computes.
- No change to the terminal runtime: tabs stay PTYs and spaces keep their
  shell.

**Non-Goals:**
- Writing from this screen: editing `agents.yaml`, installing or removing
  plugins, forgetting a fact. That is a later change, with a diff shown
  before any write to the operator's manifest.
- Moving the manage modal's project half out of it. Once this screen
  ships, the modal's Overview shows the machine only; that is a follow-up,
  so the two never show the same thing for long.
- A preference to land on the shell instead. Measure first whether anyone
  wants it.
- Stopping the server from opening a shell for a new space. That changes
  `uze-terminal`'s protocol, and the shell costs one idle process.

## Decisions

### D1. A host screen drawn like the extension surfaces, not an extension

- **Where it lives.**
  - `WorkspaceModel.space_overview: Option<SpaceOverviewView>`, keyed by
    the `SpaceId` it is for, holding the latest answer.
  - It is drawn in the pane's rect, but not through `open_extension`/
    `render_extension`. That path hands the strip's leading slot to the
    surface (`LeadingSlot::Surface`) and hides the tabs, and the overview
    needs them visible (D2).
- **Why not a `uze-extensions` extension.** It reads domain read models
  and opens host dialogs (approval, proposal), and an extension reaches
  nothing it was not handed (ADR-041). It belongs to the host, like the
  approval dialog.
- **Built from the vocabulary.** Every part comes from `src/ui/widget/`:
  - a section is a `Surface::card()`;
  - the activity counts are `stat`;
  - lists are `row`s;
  - states are `Status`;
  - controls are `Button`s.

  No chrome is assembled at the call site.
- **Module name.** `orchestrator/space_overview.rs`. The management view's
  `view/overview.rs` already uses "overview" for the machine's screen, so
  the space's carries its scope in its name.

### D2. Keyed by space, put away only by an explicit choice

- **Opening.** `land_on_space` and `step_space` open the overview for that
  space instead of focusing or creating a shell. It opens at once, before
  the server confirms `SelectSpace`.
- **Lifetime.**
  - The overview belongs to a `SpaceId`, never to the tab in front.
    `close_extension_left_behind`, which runs on every
    `Snapshot`/`SessionUpdated`, does not touch it: tying it to the tab in
    front would close it on the very broadcast that confirms the space,
    and on any tab change elsewhere in the space.
  - It is put away by: selecting a tab (the `SelectTab` click already
    closes surfaces), selecting an agent row, `+`, selecting another space
    (whose overview opens), and Esc. Esc returns to the space's selected
    tab; `Space::selected_tab` always exists.
- **The tab strip while it is shown.**
  - It draws the space's own context: its shells and the `+`. This is
    always the space's own tabs, even when an agent's tab was in front
    before.
  - An `overview` button is lit in the existing trailing group, beside
    spec, arch and code (`GroupButton { lit }`), and reopens the overview
    when it is put away. It is not a leading mark: a leading mark would
    break the flush-left first-tab rule and take the slot extensions use.
- **The way back to the shells.** Selecting the row was how an operator
  got from an agent's tabs back to the space's shells. The overview
  carries that as a first-class action, "Open shell". It focuses
  `space_own_tab`, or creates one with `open_space_shell` when the space
  has only agents, exactly what `land_on_space` did.
- **A space that is not a project.** When the root is gone, or no project
  lies in or above it, the overview says which and offers only "Open
  shell". Its sections are not drawn as failed reads.

### D3. One read model, composed in the application; counts in the client

- **Status moves into the application first.** `uze status`'s
  `ProjectStatus` (the `StatusReport` plus `steps_not_spelled_here` plus
  `commands_awaiting_approval`) is composed in `src/cli/status.rs` today.
  It moves into `uze-application`, so the CLI and the overview render one
  computation. Without the move, the overview would be the second
  computation the spec forbids. That is the first task.
- **`SpaceOverview`** lives in `services/` (the workspace half) and
  composes:
  - the moved project status;
  - plugins: the lock's declared packages with installed or not (the
    `ProjectLockStatus`), contributing or not (the context inspection's
    `contributions`), and newer commit or not (the freshness the machine
    snapshot already reads, a JSON read and never a Git call);
  - memory: `StatusReport.memory`;
  - the worktree policy and the declared artifact places, with defaults
    marked;
  - preserved work for this project.
- **Activity counts are the client's.** Running and ready come from the
  task views the client already holds (`remembered`); only preserved work
  is read. An application read model computed on the read thread cannot
  see the client's evaluation.
- **Approvals come from what the toast uses.** The overview lists commands
  awaiting approval from `remembered.commands_awaiting`, fed by
  `commands_await`, never a second read that could disagree with the
  toast.
- **Agreement is tested.** A test compares the overview's readiness with
  `uze status` for one fixture project.

### D4. Read off the frame, once at a time, and not on every tick

- **When it is read.** `spawn_space_overview(home, space, root)` in
  `reads.rs` runs when the overview opens, when the operator asks for a
  refresh, and when the space's `agents.yaml`, `agents.lock` or
  `AGENTS.md` changes. Those are read through the stamps the client
  already polls (`CHECKOUT_WATCH`).
- **Not on the evaluation clock.** Status re-runs vendor CLIs whenever
  `inspection_cache` holds no `Matched` verdict, which is exactly the
  drifted project the overview exists to show. Re-reading it every
  `TASK_REFRESH` would pay that cost every 20 seconds.
- **One at a time.** A pending flag per space allows one read in flight.
  An open during a `step_space` walk is coalesced: the read is made for
  the space the walk stops on.
- **Dropping stale answers.** The answer carries the space id, and one
  for a space no longer shown is dropped.
- **Loading.** A section without an answer yet is drawn as loading.

### D5. Actions reuse the dialogs, opened on the thing chosen

- **Commands awaiting approval.** `review_commands()` opens the first
  project waiting. It gains a form that takes the project,
  `review_commands_for(project)`, and the overview uses that.
- **A proposed fact.** `review_proposal()` likewise gains
  `review_proposal_named(name)`. The dialog opens on the way out, as from
  the toast.
- **A fact's body** opens in place: the section shows the body, read
  through `memory().show` on the read thread, until Esc. That is a read,
  not a write.
- **Open shell** is D2's action.
- Nothing else acts in phase 1. `health().status` writes `cache/`
  (detection, inspection), which is remembered state; the spec's "writes
  nothing" covers records and authored or project files, not cache.

### D6. "Memory" means agent memory everywhere

- The Overview read model's `MemoryState` becomes `ContextState`, and the
  field `memory` becomes `context`.
- The management view's label changes with it.
- It is a read model the TUI draws, with no `--json` surface and no
  persisted shape, so the rename is local.

### D7. Keys

- **A scope of its own.** The overview is a `Scope` in `uze-keys`, so its
  chords are declared and listed like every other screen's:
  - `Tab`/`Shift-Tab` move between sections;
  - arrows move within a list;
  - `Enter` acts on the focused row (open a body, review, open shell);
  - `r` refreshes;
  - `Esc` puts it away (D2).
- **Scope order.** It is pushed where the code, architect and spec scopes
  are, before `Scope::Surfaces`, so `Tab` is the overview's rather than a
  surface opener's.
- **Reopening from a pane.** A new `Action::OpenSpaceOverview` in
  `Scope::Workspace` opens the current space's overview, since
  `step_space` only reaches other spaces.
- **One adapter.** No chord is hard-coded in the view; `src/ui/keys.rs`
  stays the one file that names a physical key.

## Risks / Trade-offs

- **[Operators used to landing on the shell.]**
  - The strip shows the space's own shells while the overview is up, and
    "Open shell" is one action, which covers the agent-only space that
    selecting the row used to cover.
  - The release notes say it.
- **[Status cost on a drifted project.]** A drifted project re-runs vendor
  CLIs on every status read. The overview reads it on open, on refresh and
  on a manifest, lock or instructions change only (D4).
- **[Two overviews for one project while the manage modal keeps its
  project half.]**
  - Accepted for one release.
  - The follow-up removes the project half from the modal.

## Open Questions

- Whether the plugins section links into the manage modal's Plugins route
  for installing. That is a choice of shortcut, not of behavior, and it
  changes no requirement here.
