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
  `uze status` and a proposal toast.
- The manage modal shows some of this, but it is the machine's surface,
  opened over the client, not the space's.

A space needs a place of its own: one screen, scoped to the selected
space, that answers "what is the state of this project" and lets the
operator act where an action already exists.

## What Changes

- **A space overview.** A dedicated screen drawn where the pane is, like
  the code, architect and spec surfaces, scoped to the selected space.
  - Selecting the space's row shows it instead of focusing the space's
    shell.
  - While it is up, the tab strip shows the space's own shells, and an
    "Open shell" action brings one to the front, or creates one for a
    space that only has agents.
- **Readiness.**
  - The project's state as `uze status` computes it: `AGENTS.md`, the
    manifest against the lock, each harness's context delivery including
    agent context, gates this machine cannot run, and commands awaiting
    approval.
  - Approval stays the dialog that already exists, opened from here.
- **Memory.**
  - The project's and the machine's facts, with each one's kind, scope
    and provenance (UZE, or the harness that keeps it).
  - Opening a fact's body.
  - Proposed machine facts, accepted or discarded with the dialog that
    already exists.
- **Plugins.** The plugins the project declares: installed or not,
  contributing here or not, and whether a newer commit exists.
- **Workspace policy.** What `agents.yaml`'s `workspace:` section
  declares, read-only, including where its artifacts live.
- **Activity.** Counts of the space's agents by state: running, ready to
  deliver, preserved. The sidebar keeps listing them one by one.
- **Phase 1 is read-only plus actions that already exist.** Editing
  `agents.yaml`, installing or removing plugins and forgetting facts from
  this screen are a later change. Each writes the operator's own files
  and deserves its own review.
- **Naming.** The existing Overview read model calls the `AGENTS.md`
  context state "Memory". It becomes "Context", so "memory" means one
  thing on every screen.

## Capabilities

### New Capabilities

- `space-overview`: the space-scoped screen, covering:
  - when it shows and what selecting a space does;
  - its sections and the read model behind them;
  - the actions phase 1 offers;
  - how it stays current without blocking a frame.

### Modified Capabilities

- `overview`: the project half's `Memory` state is renamed `Context`.

## Impact

- **Depends on** `portable-agent-memory` (PR #268) for the memory read
  model.
- **Code:**
  - `uze-application`: `uze status`'s project composition moves here from
    `src/cli/status.rs`, so the CLI and the overview render one
    computation, plus a `SpaceOverview` read model built on it.
  - `src/ui/orchestrator`: the surface's model state, its read
    (`reads.rs` `spawn_*` and an `absorb_*`), its rendering from the
    `widget/` vocabulary, keys and hits.
  - The Overview read model's rename, and the views that draw it.
- **No change to `uze-terminal`:** tabs stay PTYs, and the space keeps the
  shell its server opens.
- **No new dependency, and no persisted format.**
