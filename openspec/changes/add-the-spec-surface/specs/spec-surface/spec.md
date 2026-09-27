## Purpose

Lists and renders the spec-driven-development artifacts a checkout carries
(proposals, designs, tasks and specs), by unit of intent and whichever
tool wrote them, so the intent behind a checkout's changes can be read
beside the changes themselves without leaving the terminal.

## ADDED Requirements

### Requirement: A dialect is detected by the marker its own tool defines
The surface SHALL find a checkout's spec-driven-development layout by
looking, at the checkout root, for the marker each known dialect's tool
defines, and SHALL NOT require anything to be declared in `agents.yaml`.
The only dialect this change ships SHALL be OpenSpec, detected by an
`openspec/` directory. A checkout where no marker is found SHALL be told
which layouts the surface reads, and SHALL NOT be shown an error.

#### Scenario: An OpenSpec checkout
- **WHEN** the surface is opened on a checkout whose root holds `openspec/`
- **THEN** its changes and specs SHALL be listed with no `agents.yaml` edit

#### Scenario: A checkout with no known layout
- **WHEN** the surface is opened on a checkout with no known marker
- **THEN** the surface SHALL say no spec layout was found
- **AND THEN** it SHALL name the layouts it reads, and SHALL NOT present
  this as an error

#### Scenario: The checkout is an agent's worktree
- **WHEN** the surface is opened from a tab whose checkout is an agent's
  worktree
- **THEN** the artifacts SHALL be read from that worktree, as its branch
  has them, and not from the operator's checkout

### Requirement: The surface speaks in roles, never in tools
Every file the surface lists SHALL be given one role, read from the
dialect's catalog entry: **why** (the motivation), **how** (the design),
**steps** (the work, as checkboxes), **contract** (a requirement that
outlives the change), or **other**. The roles, and not a tool's file
names, SHALL be what the surface orders and marks by. A file in a unit
that the catalog gives no role SHALL be listed as **other** rather than
hidden. For OpenSpec: `proposal.md` is why, `design.md` is how,
`tasks.md` is steps, and every `spec.md` under a change's `specs/` is a
contract, named by its capability path.

#### Scenario: A change's artifacts in role order
- **WHEN** a change holds `tasks.md`, `design.md`, `proposal.md` and
  `specs/spec-surface/spec.md`
- **THEN** they SHALL be listed as proposal, design, tasks, and the
  `spec-surface` contract, in that order

#### Scenario: A file the catalog does not name
- **WHEN** a change holds `notes.md` beside its proposal
- **THEN** `notes.md` SHALL be listed, after the named roles, as other

#### Scenario: A schema that writes different files
- **WHEN** a change was written by an OpenSpec schema that produces none
  of the named files
- **THEN** each of its Markdown files SHALL still be listed as other, and
  the change SHALL NOT be dropped

### Requirement: Units are offered as three subjects
The surface SHALL offer three subjects: the **changes** in flight, the
**specs** that outlive them, and the **archive**. Changes SHALL be listed
one group per unit, holding its artifacts. Specs SHALL be listed by
capability path. The archive SHALL be listed newest first. Changes SHALL be
the subject the surface opens on.

#### Scenario: Opening the surface
- **WHEN** the surface opens on an OpenSpec checkout
- **THEN** it SHALL show the changes subject, one group per directory
  under `openspec/changes/` other than `archive`

#### Scenario: Switching to the archive
- **WHEN** the archive subject is chosen
- **THEN** the archived changes SHALL be listed with the most recently
  archived first

#### Scenario: A project with no changes in flight
- **WHEN** `openspec/changes/` holds only `archive/`
- **THEN** the changes subject SHALL say there is nothing in flight and
  point at the archive and the specs

### Requirement: A unit's steps carry their progress
For every change in flight with a steps artifact, the surface SHALL count
its Markdown checkboxes, checked and unchecked, and show `checked/total` beside the
unit. A unit whose every checkbox is checked SHALL be marked complete. A
steps artifact with no checkbox SHALL show no count rather than `0/0`.
Archived units SHALL show no count: their steps are read only when opened.

#### Scenario: A change part-way through
- **WHEN** a change's `tasks.md` holds seven `- [x]` and five `- [ ]`
- **THEN** the change SHALL show `7/12`

#### Scenario: A finished change not yet archived
- **WHEN** every checkbox in a change's `tasks.md` is checked
- **THEN** the change SHALL be marked complete

### Requirement: Changes are ordered by where they stand
Changes in flight SHALL be listed in three bands, in this order: the
changes this checkout is working on, then the changes still in progress,
then the changes that are complete and not yet archived. Within a band,
changes SHALL be ordered by name. The last band SHALL be headed as ready
to archive, so a finished change that was never archived is told apart
from one still being worked on.

#### Scenario: A mix of states
- **WHEN** change `b` is this checkout's, `a` is at `3/10`, and `c` is at
  `8/8`
- **THEN** they SHALL be listed `b`, `a`, `c`, with `c` under the ready to
  archive heading

#### Scenario: A change with no steps
- **WHEN** a change has no steps artifact, or one with no checkbox
- **THEN** it SHALL be listed with the changes in progress

### Requirement: The change this checkout is working on is marked
A unit SHALL be marked as this checkout's own when a file under it differs
from `HEAD` in the working tree, or was changed by a commit on this
checkout's branch since the base the host reports for it. Units marked
this way SHALL be listed before the rest, and the first of them SHALL be
selected when the surface opens with no place to return to. When the host
reports no base, only the working tree SHALL be considered.

#### Scenario: An agent drafting a proposal
- **WHEN** the checkout holds an uncommitted `openspec/changes/x/proposal.md`
- **THEN** change `x` SHALL be marked as this checkout's, listed first and
  selected

#### Scenario: Committed work on an agent's branch
- **WHEN** the checkout's branch committed changes to
  `openspec/changes/x/tasks.md` since its base, and the working tree is
  clean
- **THEN** change `x` SHALL be marked as this checkout's

#### Scenario: The operator's checkout on the target branch
- **WHEN** the host reports no base and the working tree touches no
  change
- **THEN** no change SHALL be marked, and the first change SHALL be
  selected

### Requirement: Artifacts are read, and edited elsewhere
An artifact SHALL be shown rendered as Markdown, with its source one mode
away. Activating an artifact SHALL open it in the code surface at its path,
where it can be edited. The spec surface itself SHALL NOT write, rename or
delete any file.

#### Scenario: Reading a design
- **WHEN** a change's `design.md` is selected
- **THEN** its headings, lists, emphasis and code blocks SHALL be shown
  rendered

#### Scenario: Editing a task list
- **WHEN** a `tasks.md` is activated
- **THEN** the code surface SHALL open with that file selected

### Requirement: The surface is reachable the way the other lenses are
The surface SHALL open by a `toggle-spec` action bound in the workspace,
the code surface and the architect surface, and by a button in the tab
strip's group beside `architect` and `code`; the same action SHALL close
it. It SHALL stand where the pane is, as the architect and code surfaces
do, and SHALL NOT be drawn as a dialog over it; opening it SHALL close
whichever of the other two was standing there. While it is open its scope SHALL seal the keyboard from the pane
underneath. Every key it answers to SHALL be a named, rebindable action
listed in the action index, and its footer SHALL advertise keys read from
the keymap. Where the viewer was SHALL be remembered per checkout, by the
unit's and the artifact's names, and restored on reopening when both still
exist.

#### Scenario: Opening from the tab strip
- **WHEN** the spec button in the tab strip is clicked
- **THEN** the surface SHALL open over the workspace, and the button SHALL
  be the one lit

#### Scenario: Coming back to a checkout
- **WHEN** the surface is closed on change `x`'s design and reopened on
  the same checkout
- **THEN** change `x`'s design SHALL be selected

#### Scenario: The remembered change was archived
- **WHEN** the remembered change no longer exists under `changes/`
- **THEN** the surface SHALL open as if nothing were remembered

### Requirement: Reading the artifacts never blocks the workspace
Detecting the dialect, walking its directories, reading its files and
asking Git which of them the checkout touched SHALL happen off the thread
that draws. While it is outstanding the surface SHALL be open and say it
is reading. An answer SHALL carry the checkout it was asked for, and one
that arrives after the surface was closed or reopened for another
checkout SHALL be dropped.

#### Scenario: A slow filesystem
- **WHEN** the surface is opened and the reads take seconds
- **THEN** the workspace SHALL keep drawing and taking input meanwhile
- **AND THEN** the surface SHALL say it is reading

#### Scenario: The surface was closed before the answer
- **WHEN** the surface is closed while the read is outstanding
- **THEN** the late answer SHALL be dropped and nothing SHALL reopen
