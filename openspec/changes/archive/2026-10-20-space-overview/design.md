## Context

See proposal.md (Why). The layout was settled in the Claude Design project
"UZE TUI" (`Space Overview v8.dc.html`); this design binds it to the code.
What the code holds today (2026-10-17):

- **Selecting a space.**
  - A click on a space's row is `WorkspaceHit::SelectSpace`, handled in
    `session/mouse.rs`. It closes any open extension and calls
    `land_on_space` (`session/keys.rs`), which focuses the space's own
    shell; for a space with only agent tabs the click then calls
    `open_space_shell`.
  - The keyboard walk, `step_space`, goes through `land_on_space`.
  - The server opens a shell for every new space (`uze-terminal`
    `add_space`).
- **Every tab is a PTY.** What is drawn in place of a pane is the
  extension surfaces (`code`, `architect`, `spec`): client state drawn by
  `render_extension` into the pane's rect, chosen in `render.rs` by
  `open_extension`, with `LeadingSlot::Surface` handing the strip's leading
  slot to the surface.
- **Read models that exist:**
  - `StatusReport` (`health().status`): instructions, drift, the lock,
    harnesses' context, issues, held-back hooks, agent-context shortfalls
    and memory. `ProjectStatus` wraps it with `steps_not_spelled_here` and
    `commands_awaiting_approval`, composed in `src/main.rs`. The TUI does
    not consume it.
    - Agent context is `Vec<String>`, each line prefixed with a display
      name; the plugins contributing here are a count.
  - `CommandsAwaitingApproval`: read per project, answered through
    `approval.rs`'s dialog from `remembered.commands_awaiting`.
  - `plugins().list()`: per installed plugin, its commit and `Freshness`,
    read from the store index and mirrored heads without Git.
  - `memory().show`: one fact and its body.
  - `project_artifacts(cwd)`: the declared artifact places, or why they
    were refused.
- **Writing `agents.yaml`.** `ManifestDocument` (`uze-core`
  `project/manifest/edit.rs`) edits the YAML syntax tree in place, keeping
  comments, and refuses anchors, aliases, merge keys, tabs and document
  separators before an edit. Only `delivery` has a writer today
  (`declaration::set_completion`). Validation of `workspace:` reads the
  file from disk and stops at the first problem; the artifact check lives
  in `uze-application`.
- **The manage modal's header** (`management.rs` `fitted_tabs`,
  `tab_line`) and **drawer** (`view.rs` `drawer`, `drawer_width`) are the
  patterns this screen uses. Both name `TuiModel` and the manage `Hit`.

## Goals / Non-Goals

**Goals:**
- One screen per space, answering "what is the state of this project" from
  read models that already exist, with no second computation of anything
  `uze status` computes.
- The `workspace:` section discoverable and editable without reading the
  docs, validated by the parser that reads it, and saved as each change
  is made, with nothing to confirm and nothing the parser refuses written.
- No change to the terminal runtime: tabs stay PTYs and spaces keep their
  shell.

**Non-Goals:**
- Installing or removing plugins, or forgetting a fact, from this screen.
  The Plugins section names `uze install` when something is missing.
- Editing `actions`. It is free text for agents; the section shows which
  are declared.
- Proposed machine facts. They belong to the machine; the toast answers
  them.
- Moving the manage modal's project half out of it. Once this screen
  ships, the modal's Overview shows the machine only; that is a follow-up.
- A preference to land on the shell instead. Measure first whether anyone
  wants it.
- Stopping the server from opening a shell for a new space. That changes
  `uze-terminal`'s protocol, and the shell costs one idle process.

## Decisions

### D1. A host screen drawn like the extension surfaces, not an extension

- **Where it lives.**
  - `WorkspaceModel.space_overview: Option<SpaceOverviewView>`, keyed by
    the `SpaceId` it is for, holding the latest answer and the screen's
    own state (section, row, drawer, pending edits).
  - It is drawn in the pane's rect from `render.rs`'s no-extension arm,
    with `LeadingSlot::AgentTabs`, so the strip keeps the space's tabs.
- **Why not a `uze-extensions` extension.** It reads domain read models,
  writes `agents.yaml` and opens host dialogs, and an extension reaches
  nothing it was not handed (ADR-041). It belongs to the host, like the
  approval dialog.
- **Module.** `orchestrator/space_overview.rs`, a directory once it
  outgrows one file (one module per section). The management view's
  `view/overview.rs` already uses "overview" for the machine's screen, so
  the space's carries its scope in its name.

### D2. The layout: header, section bar, section and drawer, footer

- **Header.** The space's name and root; activity on the right
  ("● 2 running · 1 ready · 3 preserved"), shortened to numbers when
  narrow.
- **Section bar.** One tab per section, in a fixed order: `1 Agent`,
  `2 Settings`, `3 Plugins`, `4 Context`, `5 Memory`, `6 Worktrees`: the
  agent in front, then what the project declares, then what follows from
  it. Worktrees came last (D10) so the numbers the first five are reached
  by did not move. The overview
  opens on Agent when an agent is in front, on Settings otherwise.
  - A tab carries a glyph only when its section warns, fails or is
    loading. Its state is the worst among the section's parts.
  - Without room, inactive tabs shrink to number and glyph.
  - It is the manage header's tabs, moved to `src/ui/widget/` as a
    generic `tabs` (labels, an optional mark, the active index, back the
    rect of each) since this is their second file. Manage keeps its look.
- **Cause line.** The section's first line names the cause of its state
  ("drift · 2 missing", "1 command never runs here · 1 waiting for
  approval", "1 unreadable"). It is never a neutral count when the tab
  warns.
- **Section body.** Tables with a header row and state in words, coloured
  only where a value diverges.
- **Drawer.** The selected row in full, on the right when the pane leaves
  room for both; otherwise Enter opens it over the section and Esc closes
  it. It is the manage drawer, moved to `src/ui/widget/` with the width
  and the drag state passed in rather than read from `TuiModel`.
- **Footer.** The hints for the focus, and on the
  right how many things need attention ("! 3 to check", or "✓ ready"). The
  count is a button: it lists them in a dialog, worst first, drawn by the
  `alerts` widget the management's Health dialog is drawn by too. What a
  harness makes of the project's context is said there and in the Agent
  drawer, so there is no Context section.
- **Built from the vocabulary.** Every part comes from `src/ui/widget/`;
  nothing is assembled at the call site. New widgets the Workspace
  section needs (D6) are added there.

### D3. Opened by the space button, put away by any other choice

- **Opening.** A `space` button at the strip's right end, a group of its
  own apart from spec, arch and code, drawn in every context of the space,
  toggles the overview over the tab in front (`WorkspaceHit::OpenOverview`,
  `Action::OpenSpaceOverview`). It stands where the agent drawer's mark
  stood. Selecting a space's row keeps landing on its shells.
- **Lifetime.** The overview records the tab it stands in front of. Any
  other selection the server reports puts it away, whatever caused it; so
  do a tab or agent click (the one behind it included), opening a surface,
  the button again and Esc.
- **The Agent section.** The agent drawer's state (`AgentSupportDropdown`:
  support, scope, selection) moves into the overview, built when it opens
  and read again then. Its prompts are drawn with the drawer's own
  `record_lines`, its facts with `capability_lines`; the scope switch is a
  pair of `Chip`s. Enter copies a prompt; `tab` switches whose prompts;
  shift+x asks to clear the space's history. The floating drawer, its
  scope and its text selection are removed.
- **A space that is not a project.** When the root is gone, or no project
  lies in or above it, the screen says which. No section bar is drawn.

### D4. One read model, composed in the application

- **Status moves into the application first.** `ProjectStatus` (the
  `StatusReport` plus `steps_not_spelled_here` plus
  `commands_awaiting_approval`) is composed in `src/main.rs` today. It
  moves into `uze-application` (`health().project_status(root)`), so the
  CLI and the overview render one computation.
- **Status carries what the screen needs per thing, not as sentences.**
  - Agent context and the "context unread" reason move onto each harness
    of the report, keyed by integration id. The CLI renders the same
    lines from them.
  - The plugins contributing here are named, not counted.
  - `uze status`'s text output stays as it is; `--json` changes shape,
    which no persisted format or other surface reads.
- **`SpaceOverview`** lives in `services/` (the workspace half) and
  composes:
  - the moved project status;
  - plugins: the lock's declared plugins with installed or not and
    `differs_from_lock`, contributing or not from the status, and commit
    and freshness from `plugins().list()` (no Git);
  - memory: `StatusReport.memory`, the project's entries listed and the
    machine's counted;
  - the workspace declaration view (D6);
  - preserved work for this project.
  - "Root gone" and "not a project" are answers of their own, never
    errors.
- **Activity counts are the client's.** Running and ready come from the
  task views the client already holds; only preserved work is read.
- **Approvals come from what the toast uses.** The Settings section marks
  lines awaiting approval from `remembered.commands_awaiting`, never a
  second read that could disagree with the toast.
- **Agreement is tested.** A test compares the overview's Context and
  Plugins states with `uze status` for one fixture project.

### D5. Read off the frame, once at a time, and not on every tick

- **When it is read.** `spawn_space_overview(home, space, root)` in
  `reads.rs` runs when the overview opens, when the operator asks for a
  refresh (`r`), after each written workspace change, and when the space's
  `agents.yaml`, `agents.lock` or `AGENTS.md` changes, seen through the
  stamps the client already polls (`CHECKOUT_WATCH`).
- **Not on the evaluation clock.** Status re-runs vendor CLIs whenever
  `inspection_cache` holds no `Matched` verdict, which is exactly the
  drifted project the overview exists to show.
- **One at a time.** A pending flag per space allows one read in flight.
  An open during a `step_space` walk is coalesced: the read is made for
  the space the walk stops on.
- **Dropping stale answers.** The answer carries the space id, and one
  for a space no longer shown is dropped.
- **Loading.** A section without an answer yet is drawn as loading, and
  its tab carries the spinner.

### D6. Editing `workspace:`

- **The view.** `Workspace::declaration_view(root)` returns each key of
  `workspace:` in a fixed order with its declared value or its default,
  marked as a default; `actions` by name only; and whether the file can be
  edited (the `ManifestDocument` refusal reason when it cannot).

  | key | control | default |
  |---|---|---|
  | `worktree` | choice `manual`/`always` | `manual` |
  | `target` | text | the primary checkout's branch at launch |
  | `delivery` | choice none/`merge`/`push` | none |
  | `branch` | choice `conventional`/`gitflow`/`flat`/`agent`/`types` | `conventional` |
  | `link` | list of paths | empty |
  | `setup`, `gate` | list of commands, each a line or a posix/windows pair | empty |
  | `slots` | number ≥ 1 | no limit |
  | `idle_days` | number | 3 |
  | `artifacts` | list of directories | undeclared |

  Choosing `types` for `branch` turns the key into a list of the
  project's own types.
- **The edit, in the client.** Pending edits live on the screen. Enter
  edits the key in place (a choice advances; text and numbers open a
  `Field`; a list opens its items), `a` adds an item, `d` removes one or
  unsets a key, alt+↑↓ reorders. A key edited and not yet in the file is
  marked "edited".
- **Each change is saved as it is made.** Choosing an option, a stepper's
  − or +, committing a list item, removing or moving one, unsetting a
  key and finishing an action's text with its save chord each write the
  file; nothing is written while a field or text area is still being
  typed, and there is no Save and no diff to confirm. Pending edits are
  only those not yet written, or refused. One save is out at a time, on
  the read thread; edits made meanwhile are sent once it answers. A
  written answer drops exactly the edits it carried, unless changed
  since, re-reads the overview and raises a toast naming what changed
  ("delivery: merge", or "3 settings"), "agents.yaml created" when it
  started the file.
- **Validation is the parser's.** `Workspace::apply_declaration(root,
  edits)`, on the read thread, applies the edits to the file's current
  text through `ManifestDocument::from_source`, and runs the same
  checks `declaration::declared` runs on that text, collecting every
  problem keyed by the key it concerns:
  - `uze-workspace` gains a validation that takes the candidate text and
    the root and returns every problem rather than the first (slots 0,
    misplaced links, links Git does not ignore, unknown keys);
  - the artifact check (`project_artifacts`'s inside-the-project rule)
    runs on the candidate too.
  - With any problem, nothing is written: the refused edits stay
    pending and marked, each problem is shown under its key, editing the
    value again saves again, and `x` drops the refused edits.
- **Writing.** `apply_declaration` holds `MutationLock`, reads the file as
  it is now, applies the edits and saves atomically. An edit names a key,
  so it lands on whatever the file holds, and nothing is compared with an
  earlier reading. When `setup` or `gate` changed, the answer says those
  lines wait for approval again, since approval is a digest of the lines;
  saving never approves them, and the toast says so. `uze-core` gains a public `workspace:` key writer on
  `ManifestDocument` (any value, set or removed), which `set_completion`
  then uses too.
- **No `agents.yaml` yet.** Every key shows its default, and the first
  change creates the file with only the keys set.
- **A file UZE will not edit.** The section is read-only and says why
  ("edit agents.yaml by hand: uses a YAML anchor").

### D7. Actions reuse the dialogs, opened on the thing chosen

- **Commands awaiting approval.** `review_commands()` opens the first
  project waiting. It gains `review_commands_for(project)`, which the
  Settings section's Review uses.
- **A fact's body** is read through `memory().show` on the read thread
  and shown in the drawer.
- Nothing else writes but D6's changes. `health().status` writes `cache/`
  (detection, inspection), which is remembered state.

### D8. "Memory" means agent memory everywhere

- The Overview read model's `MemoryState` becomes `ContextState`, and the
  field `memory` becomes `context`.
- The management view's label changes with it.
- It is a read model the TUI draws, with no `--json` surface and no
  persisted shape, so the rename is local.

### D9. Keys

- **Scopes of its own** in `uze-keys`: `SpaceOverview`, and
  `SpaceOverviewEditing` (sealing, text) while a `Field` is open. They are
  pushed where the code, architect and spec scopes are, before
  `Scope::Surfaces`.
- **Chords:**
  - `1`-`6` (`Action::SelectSection`) and ←→ switch section; ↑↓ move
    within it. `SelectTab`'s chords stay the strip's over the overview,
    so `alt+n` still chooses a tab and puts the overview away;
  - Enter opens the drawer or edits; Esc closes the innermost thing open
    (field, drawer) and, at the top, puts the screen away;
  - `o` opens the shell; `r` refreshes;
  - in Settings: `a`, `d`, alt+↑↓, and `x` over refused edits (D6).
- **Reopening from a pane.** `Action::OpenSpaceOverview` in
  `Scope::Workspace` opens the current space's overview.
- **One adapter.** No chord is hard-coded in the view; `src/ui/keys.rs`
  stays the one file that names a physical key.

### D10. Worktrees as cards, measured only while shown

- **What is listed.** `Workspace::checkouts(root)`, the read the sidebar's
  recent mode already makes, filtered to the checkouts UZE made for an
  agent or a subagent (and its own whose record is unreadable). A
  person's and a harness's own checkouts are theirs.
- **Last on the bar.** The checkouts are what the agents left on disk:
  what follows from everything before. Last also keeps `1`-`5` where they
  were.
- **Cards, not a table.** A worktree is read as a whole thing to act on
  elsewhere, as the manage catalogs' cards are. The catalog's grid moved
  to `widget::grid` (its second file); the card is `Surface::selectable`,
  so selection and hover read as on every card the keys land on. Columns
  follow the width (at least 32 wide, at most four), and a row is drawn
  only whole. The wheel scrolls rows of cards; the keys walk them one at
  a time, in reading order.
- **State, from what the client holds.** Ready when the last evaluation
  says so; working when a tab of this workspace or the record says an
  agent is there (the card names the tab); to join for a subagent whose
  agent ended; kept for preserved work; idle when held otherwise; free
  when no agent holds it.
- **Cost.** The measure walks every file of every checkout, in parallel per
  checkout. It runs off the frame when the section comes to front and on a
  refresh, never on open or a tick; one per overview at a time, and an
  answer to an earlier measure or to an overview put away is dropped.
- **Health.** The tab warns on uncommitted work no agent attends (a slot
  whose agent is not live, a subagent's with no topic) and on an
  unreadable record. Size alone never warns: no threshold is honest for
  every project.
- **One removal, of a free worktree only (D11).** The drawer reads the
  worktree in full; kept work is discarded or joined from recent, beside
  the work.

### D11. Removing a free worktree, asked first and judged again

- **Only free.** A slot no task holds, with nothing uncommitted, nothing
  unkept on its branch and nobody inside: what the pool would hand the next
  agent. Nothing else is offered, and no button stands greyed where it is
  not. Kept, to join and idle cards say in one muted line where to go.
- **The safety is the workspace's.** `checkout::remove_free_slot` re-reads
  the slot under the task store's lock and the repository's write lock,
  right before it moves anything, and observes the process table there:
  any process inside, attended or not, refuses it, since a daemon follows a
  moved directory without a word. A slot that is no longer UZE's, that a
  task names, that holds uncommitted work or that is not free is refused
  with the reason. It is set aside as collection does, its branch kept,
  and the trash is emptied once the locks are let go.
- **Asked first.** Its directory is often gigabytes of build output the
  client cannot give back, so `RemoveItem` (`d`) or the drawer's Remove
  opens a dialog naming the worktree and its size; arrows, Enter and Esc
  answer it, Cancel first.
- **Reusing `RemoveItem`.** It already means "remove the entry in focus" in
  this scope, and stays non-destructive honestly: it only asks, and a free
  slot holds no work, only output a build makes again. A new action would
  have needed another letter for the same gesture.
- **Off the frame.** The removal runs on a thread, one per overview at a
  time; the header says it is underway, a toast says what happened (the
  size freed, or why it was kept), and the section is read again either way.

## Risks / Trade-offs

- **[Operators used to landing on the shell.]**
  - The space button opens the overview from anywhere in the space;
    the row keeps leading to the shells.
  - The release notes say it.
- **[Status cost on a drifted project.]** A drifted project re-runs vendor
  CLIs on every status read. The overview reads it on open, on refresh,
  after a written change and on a manifest, lock or instructions change
  only (D5).
- **[A write from the TUI to the operator's file, with no confirmation.]**
  Each write changes only the keys edited, keeps comments, refuses shapes
  the editor cannot keep and anything the parser refuses, and a toast
  names what changed; the file is the operator's, under Git.
- **[`git check-ignore` per save.]** Validating links asks Git; the save
  runs on the read thread, once per committed change.
- **[Two overviews for one project while the manage modal keeps its
  project half.]** Accepted for one release; the follow-up removes the
  project half from the modal.

## Open Questions

- Whether the drawer's width is resizable here as in manage. The moved
  widget allows it; the first slice keeps the default width.
