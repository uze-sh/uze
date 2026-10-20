## Why

Selecting a space in the workspace focuses the shell its server opened,
which tells the operator nothing about the space itself. Whether the
project is ready, which commands wait for approval, what its agents
remember, which plugins it declares and how its workspace is set up all
exist as read models already, but each is seen somewhere else or not at
all:

- `uze status` is not consumed by the TUI at all.
- Commands awaiting approval only appear as a toast.
- Agent memory (`portable-agent-memory`) only reaches the operator through
  `uze status`.
- The `workspace:` section of `agents.yaml` is discovered by reading the
  docs: nothing in the product shows which keys exist, what each defaults
  to, or what a project set.

A space needs a place of its own: one screen, scoped to the selected
space, that answers "what is the state of this project", lets the operator
act where an action already exists, and lets them configure the
workspace without knowing its keys.

## What Changes

- **A space overview.** A screen drawn where the pane is, like the code,
  architect and spec surfaces, scoped to the space in front.
  - A `space` button at the strip's right end, apart from spec, arch and
    code, opens it from any context of the space: an agent's or the
    space's own. It takes the place of the agent drawer's mark.
  - Selecting a space's row keeps landing on its shells, as before.
- **An Agent section** takes the agent drawer's place: the prompts the agent
  in front, or its space, was given, and what reaches that agent. Enter on a
  prompt copies it. The drawer itself is removed.
- **Five sections**: the agent in front, then what the project declares
  and what follows from it, reached from a section bar like the
  manage modal's header. Each tab carries the worst state of
  its parts, and the section's first line names the cause.
  - **Settings.** Every key of `agents.yaml`'s `workspace:` section, set or
    defaulted, and the state of each `setup` and `gate` line on this
    machine. The section is **editable**: a key is changed in place, the
    real parser validates the result, and each change is written to
    `agents.yaml` as it is made; a change the parser refuses is not
    written and stays marked with its problem.
  - **Plugins.** The plugins `agents.yaml` declares: installed, contributing
    here, the commit installed and its freshness; and `agents.lock`'s drift
    against the declaration.
  - What reaches each harness of the project's context has no section:
    the Agent drawer says it for the agent in front, and the foot's count
    of what to check lists every harness that misses it.
  - **Memory.** The project's facts with their kind, origin and date, and
    how many machine facts also reach agents here. A fact's body opens in
    the drawer.
  - **Worktrees.** The checkouts UZE made for the space's agents and
    subagents, one card each: branch, state, owner and the tab working
    there, size on disk, commits ahead, uncommitted work, path and age.
    A free worktree can be removed from its drawer, after a question;
    nothing else is: discarding kept work stays where the work is.
- **A drawer** beside the section shows the selected row in full: a
  harness, a plugin, a fact, a workspace key.
- **Activity.** Counts of the space's agents by state beside the title.
- **Proposed machine facts stay out.** They are the machine's, not the
  space's, and the toast that announces one already stays until answered.
- **`uze status` stops listing every fact.** Its memory block becomes one
  line per scope (count by provenance) plus what needs the operator
  (proposals, unreadable files). With fifty facts the listing buried the
  rest of the report; the facts move to the overview's Memory section and
  stay in `--json`. This revises `portable-agent-memory`'s open
  requirement in place.
- **Naming.** The existing Overview read model calls the `AGENTS.md`
  context state "Memory". It becomes "Context", so "memory" means one
  thing on every screen.

## Capabilities

### New Capabilities

- `space-overview`: the space-scoped screen, covering:
  - when it shows and what selecting a space does;
  - its sections, the drawer and the read model behind them;
  - editing the `workspace:` section;
  - how it stays current without blocking a frame.

### Modified Capabilities

- `overview`: the project half's `Memory` state is renamed `Context`.

## Impact

- **Depends on** `portable-agent-memory` (PR #268) for the memory read
  model.
- **Code:**
  - `uze-application`:
    - `uze status`'s project composition moves here from `src/main.rs` and
      `src/cli/status.rs`, so the CLI and the overview render one
      computation.
    - `StatusReport` carries per-harness agent context and the plugins
      contributing here by name, rather than display-name-prefixed lines
      and a count. `uze status --json` changes shape accordingly.
    - A `SpaceOverview` read model, and a workspace declaration view and
      edit (one apply per change: written, or refused with its problems)
      built on `uze-workspace`'s parser.
  - `uze-workspace`: the `workspace:` section's validation takes candidate
    text and collects every problem, instead of reading the file and
    stopping at the first.
  - `uze-core`: a public writer for any `workspace:` key on
    `ManifestDocument`.
  - `src/ui`: the screen, its read and its edit (`reads.rs` `spawn_*` and
    `absorb_*`), keys and hits. The manage header's tabs and the drawer
    move into `src/ui/widget/`, since this is their second file.
  - The Overview read model's rename, and the views that draw it.
- **No change to `uze-terminal`:** tabs stay PTYs, and the space keeps the
  shell its server opens.
- **No new dependency, and no persisted format.** The only file the screen
  writes is the `agents.yaml` of the checkout the agent works in, through
  the existing editor.
