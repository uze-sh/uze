## Context

See proposal.md for why. What the code holds today:

- `uze-terminal` owns the space: `Space { id, label, root, tabs }`, where
  `root` is mandatory and is the identity. `Session::open_space` matches a
  `SpaceSeat { root }` by canonical root; `SpaceSeed` persists the root and
  not the id, so `Session::restore` mints fresh `SpaceId`s. A server with
  nothing persisted seeds `Session::new(seat)` from `terminal serve --root`
  (`src/main.rs:414`, `runtime/server.rs:111`), and `CreateTab { cwd: None }`
  falls back to the space's root (`runtime/server.rs:462`). The persisted
  workspace is shape 2 on `uze-document`'s ladder, and the client protocol
  is `PROTOCOL_VERSION = 21`.
- `CreateTab` names no space: the server opens the tab in the client's
  selected space (`runtime/server.rs:457`).
- The domain does not know spaces. A placement resolves its project from
  the `from` directory it is handed (`PlacementRequest::New { from }`), and
  a task records its project, never a space.
- The client reads `space.root` as "the project of this space" in:
  - `space_cwd` (where a new agent is placed from, the sidebar caption,
    `space_directories`) and `space_root_of_pane` (`agents.rs:7`), which
    keys **prompt recording** (`activity.rs:46`);
  - `land_agent_in_its_own_space`, which creates a space rooted at the
    agent's project;
  - the overview (`space_overview.rs:1239`, `put_away_overviews` keyed by
    `SpaceId` and root) and `AgentSupportDropdown::space_root`
    (`orchestrator.rs:1282`);
  - the management modal's project (`session/manage.rs:57`) and the root
    picker's starting point (`manage.rs:24`);
  - `collapsed_space_roots`; `recent.rs` (`projects_to_read`,
    `space_open_on`); the occupancy sweep's `look_in` (`tabs.rs:746`);
  - the client's attach seat (`ui.rs:119`, `orchestrator.rs:464`), whose
    `seating_at` sends `Seating::At(home)` for a launch from `$HOME`;
    `CreateSpace` from the sidebar (`session/mouse.rs:86`); `home_seat` as
    `CloseSpace`'s replacement (`tabs.rs:452`); the nested launch
    (`main.rs:729`).
- Already reading the selected tab rather than the root: the git badge,
  the changes overlay and the code, spec and architect surfaces.
- `prompt_history` is stored per project (`project_id_for`), and each
  entry already carries the agent's identity (`PromptEntry::agent`).

## Goals / Non-Goals

**Goals:**
- Any agent of a space may run in any directory, and what the workspace
  shows about a project is read from the agent in front.
- `uze-terminal` keeps knowing nothing of agents or projects: everything
  it decides is a path comparison and whether a tab was launched with a
  program.
- Delivered in three phases, each shippable on its own and leaving no
  code behind (D9).

**Non-Goals:**
- Grouping in the domain. A task does not learn its space.
- A space-level policy (one worktree policy, target or gate for the
  group). Policy stays per project.
- Moving agents between spaces by drag or menu.
- Rewording requirements whose behaviour does not change and that say
  "the space's root" for "the directory the agent was started in":
  `worktree-policy`, `agent-session-continuity`, and `agent-isolation`'s
  root-agent, isolation and carried-changes requirements. The two
  `agent-isolation` requirements whose behaviour does change (column
  grouping, where an agent starts) are modified here.

## Decisions

### D1. The directory step comes before the harness picker, in the space's place

Where an agent runs is the question that decides the rest (its project,
its worktree policy), so it is asked first. Prefilling with the agent in
front's project (D5) keeps the common case one Enter. The harnesses the
picker offers are the machine's, whatever the directory.

Alternative: harness first, directory after. Rejected: the directory is
the step that changes from one agent to the next in a mixed space, and
the harness is usually skipped (one set up) or the same.

The step reuses `root_picker`'s narrowing listing, drawn in the popover
the harness picker already is: anchored beside the space's `new`, floating
over the pane, as the design-system skill asks of a popover. Once a
directory is chosen the same popover shows the harnesses, at the same
place and size (up to ten rows, scrolling inside), so nothing moves
between the steps and nothing in the sidebar moves at all. The root
picker's drawing over the whole column goes with the new-space mode it
served.

Alternative: draw the steps in place of the space's rows in the sidebar.
Rejected by the operator: it moves the sidebar and breaks the popover
rule the rest of the workspace follows.

### D2. A space has no directory, and its id is what persists

`Space.root` is removed. A new space's shell opens in the home directory;
the launch passes its project as a creation parameter, not as state.
`CreateTab { cwd: None }` opens in the directory of the agent in front,
else home. `SpaceSeed` gains the space's `id`, and the persisted workspace
gains `next_space_id`, so `restore` keeps ids and never hands a closed
space's id to a new one. The server's first space, with nothing persisted,
is created from a label and a directory instead of a seat.

Alternative: a settable `default_dir`. Rejected: every use is answered by
the tabs, and a stored default is a second record that can disagree.

Alternative: `next_space_id` = max + 1 on restore. Rejected: it reuses the
id of a closed space that held the maximum, and anything keyed by it (D6)
would follow the wrong space.

### D3. "Working in a project" means an agent tab stands in it

One definition, used by recent's landing, the caption and the occupancy
sweep: a space works in a project when a tab of it launched with
a program (`Launch::Program`, which the runtime already records) stands in
the project's root or below it. Slots live under `<project>/.worktrees/`,
so an isolated agent matches its project with no domain knowledge in the
runtime. A shell never counts, wherever it `cd`s.

Containment runs one way, tab inside project. A directory in no project
is its own project, as `space_root` already answers. A launch from the
home directory selects nothing and creates nothing
(`Seating::WhereItLeftOff`): with containment, every tab under `$HOME`
would match, and the launch would pick an arbitrary space.

The launch alone also takes a shell, as a second choice: a space with an
agent in the project first, else one with any tab there. Without it, the
space a launch opened for a project, holding only its first shell, would
be missed by the next launch there, which would open a second one.

Alternative: shells never count, the launch included. Rejected: every
`uze` started in a project before an agent ran there would open another
space.

### D4. Landing is decided by the client, never by creating a space

`land_agent_in_its_own_space` is replaced by one rule:
- placed for a tab (isolate, switch, resume replacing a tab): that tab's
  space;
- new: the space whose `new` was pressed, carried as a `SpaceId` in the
  placement's pending state, since the operator may select another space
  before the checkout is ready;
- resumed from recent: the space in front if it works in the project (D3),
  else the first space that does, else the space in front.

Until phase 3, the client sends `SelectSpace` for that space before
`CreateTab`. Phase 3 adds `space` to `CreateTab`, so the tab lands where it
was meant without moving any client's selection, and the `SelectSpace`
goes.

### D5. Project actions read the agent context, and a space's shell has none

`agent_in_front(model)` answers the agent whose context is in front (its
tab, or a shell whose `Tab.agent` names it) and the directory it was
placed in: the task record's checkout when there is one, else the
directory the tab was launched in. Never the live-probed `pane.cwd`, which
a finished agent's respawned shell can move. Everything that describes a
project reads it: `focused_cwd`, the spec, architect and code surfaces, the
overview and its prompt history, the management modal's project, and the
directory step's prefill. With a shell of the space in front it answers
nothing: the strip draws no status and no surface buttons, an open surface
closes, and the prefill and the management modal fall back to the space's
most recently started agent, then home.

Alternative: follow the tab in front, shells included. Rejected: the
strip would change repository on every `cd` and read Git in arbitrary
directories.

### D6. Folded spaces are kept by id

`client_layout.collapsed_space_roots` becomes `collapsed_spaces:
BTreeSet<u64>` (`uze-workspace` does not depend on `uze-terminal`), shape 2
of the client layout, pruned of ids no space has on every session update.
The step from shape 1 drops the folded set: every space opens unfolded
once, the one thing the upgrade loses.

### D7. The occupancy sweep reads the projects agents work in

`look_in` extends with the distinct projects of agent tabs (D3), instead
of every space's root. A shell's directory, `$HOME` included, never
reaches `reconcile_occupancy`.

### D8. The overview is the agent's, and prompts are recorded by the agent's project

The overview opens on one agent: header `Agent`, its label and directory;
its own prompts only; its project's settings, plugins, memory and
worktrees. It drops the space-wide prompt list, the "where" column and the
count of agents by state. The strip's `space` button becomes `agent`,
drawn only in an agent's context. Selecting another agent turns the
overview to it; leaving the agent's context puts it away.

Prompt recording moves to phase 1: it keys by the agent's project (D5),
not `space_root_of_pane`, which phase 3 removes. Prompts already recorded
for an agent of another project under the space's root stay in that file
and are not found; the history is a best-effort log, so they are not
moved. The Agent section filters a project's file by `PromptEntry::agent`;
entries with none (a harness started by hand, or written before identity
was recorded) are not listed. Clearing rewrites the project's file without
that agent's lines, under the lock compaction takes.

The `space-overview` change is implemented but not archived, so its spec
was rewritten in place; its implementation is phase 2's. The capability
keeps its name until it is archived.

### D9. Each phase deletes what it retires

A phase is done when the code it replaced is gone, not when the new path
works beside it. Nothing is kept for a later phase, behind a flag, or
under `#[allow(dead_code)]`. The old persisted shape is read only by the
ladder step, the one place a space root may still be named. The tasks
list the names each phase retires, so a `grep` proves it.

### D10. Facts the sidebar draws are computed off the frame

The project each agent row names and the caption's count need `slot_key`,
a filesystem read. They are computed when a session update or an
evaluation is absorbed, as `evaluation_key` already is, never while
drawing.

## Candidate ADRs

- **A space is a group of agents, not a directory**: supersedes the clause
  of ADR-053's Decision that "one root means one space", and changes the
  persisted workspace's identity from root to id.

## Risks / Trade-offs

- [A space's caption no longer says where its shell is] → the caption
  names the project its agents work in, or how many; the shell's own row
  still shows its directory.
- [A new space's first agent walks from home] → only the first: from then
  on the prefill reads the space's agents.
- [Two spaces on one project] → worktree removal already checks occupancy
  across every live pane, not per space; a test pins it.
- [An older client attached to a newer server] → the protocol bump makes
  the handshake refuse it, and the existing "end and restore" path takes
  over.
- [The overview flickering between agents of different projects] → it
  keeps the last answer drawn until the new one lands.
- [Agents of several projects hide which target each delivers to] → each
  row names its project in a multi-project space; delivery stays per agent.

## Migration Plan

1. **Phase 1, no format change.** Directory step in the space's place; D4
   landing with `SelectSpace` before `CreateTab`; prompt recording by the
   agent's project. `root` still persists, and nothing the new flow does
   reads it.
2. **Phase 2, no format change.** D5, D7, D8's overview, D10, the sidebar
   caption and multi-project rows, recent's row.
3. **Phase 3, the runtime model.** D2, D3, D6, `space` on `CreateTab`:
   shape 3 of the persisted workspace (root dropped, id and
   `next_space_id` kept), shape 2 of the client layout,
   `PROTOCOL_VERSION` 22. Rolling back past this is a downgrade across a
   shape, which `uze-document` refuses rather than misreads.
