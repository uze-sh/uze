## MODIFIED Requirements

### Requirement: A dialect is detected by the marker its own tool defines
The surface SHALL find a checkout's spec-driven-development layout by
looking, at the checkout root, for the marker each known dialect's tool
defines, and SHALL NOT require anything to be declared in `agents.yaml`.
The dialects read SHALL be OpenSpec (an `openspec/` directory), Spec Kit
(`.specify/`), Superpowers (`docs/superpowers/`) and GSD (`.planning/`),
each a catalog entry of data read through the same code. A checkout
holding more than one marker SHALL be read for every one of them, and a
unit SHALL name its tool beside it only while more than one tool shares
the list. A checkout where no marker is found and no decision is found
SHALL be told which layouts the surface reads, and SHALL NOT be shown an
error. Decisions are not a tool's, and are found without a marker.

#### Scenario: An OpenSpec checkout
- **WHEN** the surface is opened on a checkout whose root holds `openspec/`
- **THEN** its changes and specs SHALL be listed with no `agents.yaml` edit

#### Scenario: A checkout with no known layout
- **WHEN** the surface is opened on a checkout with no known marker
- **THEN** the surface SHALL say no spec layout was found
- **AND THEN** it SHALL name the layouts it reads, and SHALL NOT present
  this as an error

#### Scenario: A checkout carrying two tools
- **WHEN** the checkout root holds both `openspec/` and `.specify/`
- **THEN** the units of both SHALL be listed, each naming its tool

#### Scenario: The checkout is an agent's worktree
- **WHEN** the surface is opened from a tab whose checkout is an agent's
  worktree
- **THEN** the artifacts SHALL be read from that worktree, as its branch
  has them, and not from the operator's checkout

#### Scenario: Decisions without a tool
- **WHEN** a checkout carries no known marker and its declared places hold ADRs
- **THEN** the decisions subject SHALL be offered, and no other

### Requirement: The surface speaks in roles, never in tools
Every file the surface lists SHALL be given one role, read from the
dialect's catalog entry: **why** (the motivation), **how** (the design),
**steps** (the work, as checkboxes), **contract** (a requirement that
outlives the change), **decision** (a decision record the unit carries),
or **other**. A Markdown file in a unit SHALL be a decision when it
carries a record's numbered name (`NNN-slug.md`), whatever the dialect
calls it, and SHALL be listed after the design. The roles, and not a tool's file
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

#### Scenario: A change carrying its decision
- **WHEN** a change holds `proposal.md`, `design.md` and `adr/017-lock.md`, a decision record
- **THEN** the record SHALL be listed after the design, as a decision, named by its title

### Requirement: Units are offered as three subjects
The surface SHALL offer at most three subjects: the **changes**, the
**specs** that outlive them, and the **decisions** the project keeps. What
a tool puts away SHALL NOT be a subject of its own: archived changes SHALL
be the last band of the changes, **archived**, newest first and folded
until the viewer is put in it. A subject none of the detected tools keeps,
or, for decisions, that no declared place holds, SHALL NOT be offered; one
a tool keeps but that holds nothing yet SHALL be offered and say so.
Changes SHALL be listed one group per unit, holding its artifacts. Specs
SHALL be listed by capability path. Changes SHALL be the subject the
surface opens on.

#### Scenario: Opening the surface
- **WHEN** the surface opens on an OpenSpec checkout
- **THEN** it SHALL show the changes subject, one group per directory
  under `openspec/changes/` other than `archive`, and after them the
  archived band, folded

#### Scenario: Switching to the archive
- **WHEN** the archived band is opened
- **THEN** the archived changes SHALL be listed with the most recently
  archived first

#### Scenario: A project with no changes in flight
- **WHEN** `openspec/changes/` holds only `archive/`
- **THEN** the changes subject SHALL open on the archived band

#### Scenario: Never more than three
- **WHEN** a checkout carries every shipped tool's marker and its declared
  places hold decisions
- **THEN** exactly changes, specs and decisions SHALL be offered

#### Scenario: Switching to the decisions
- **WHEN** the decisions subject is chosen
- **THEN** every decision in the declared places SHALL be listed, in the
  order their file names give, each with its status

## ADDED Requirements

### Requirement: A decision is read the way the published ADR templates write it
The surface SHALL look for decisions in every place `workspace.artifacts`
declares, as deep as each goes, and SHALL NOT require a tool's marker or a
fixed directory. A Markdown file SHALL be a decision when its name is a
record's, as every ADR tool names one: a number of at least three digits,
optionally after `adr-`, a hyphen or underscore, and a slug; a dated
document named `YYYY-MM-DD-slug.md` SHALL NOT be one. Its title
SHALL be the front matter's `title`, else its first `# ` heading without
adr-tools' `N. ` numbering, else its slug. Its status SHALL be the front
matter's `status`, else the header's `Status:` field, else the first line
of a `## Status` section, reduced to the state before its first link or
clause. A record with none of these SHALL be listed with no status rather
than dropped, and no other field SHALL be read as its status. Every other
file SHALL be ignored, without a report. A project that declares no place
SHALL be offered no decisions subject.

#### Scenario: An adr-tools record
- **WHEN** a declared place holds `0001-keep-one-lock.md` with
  `# 1. Keep one lock`, a `Date:` line and a `## Status` section holding
  `Accepted`
- **THEN** it SHALL be listed as `0001 Keep one lock`, with status
  `Accepted`

#### Scenario: A MADR record
- **WHEN** a declared place holds a numbered file with `status: accepted`
  in its front matter
- **THEN** it SHALL be listed with status `accepted`

#### Scenario: A log4brains record
- **WHEN** a declared place holds `20240105-use-markdown.md` whose header
  holds `- Status: accepted <!-- optional -->`
- **THEN** it SHALL be listed with status `accepted`

#### Scenario: A record in a shape of its own
- **WHEN** a declared place holds `0001-manter-um-lock.md` writing its
  state as `Situação: Aceita`
- **THEN** it SHALL be listed by its title with no status

#### Scenario: A guide is not a decision
- **WHEN** a declared place holds `guide.md`, `README.md` and
  `adr-template.md`
- **THEN** none SHALL be listed, and nothing SHALL be reported about them

#### Scenario: Nothing declared
- **WHEN** the project's `agents.yaml` has no `workspace.artifacts`
- **THEN** no decisions subject SHALL be offered, even where a `docs/adr/`
  exists

### Requirement: Supersession is read from the status, as the templates write it
A decision SHALL be listed as superseded when its status opens with
`superseded` or adr-tools' `superceded`, and the record its status names —
by a link to a record file, or as `ADR-NNNN` — SHALL be shown as
superseding it; each side SHALL name the other. The same SHALL hold for the
formats that keep it as a field: adrkit's front matter `supersedes` and
`supersededBy`, and the `Supersedes:` header field of OpenSpec's
`spec-driven-with-adr` schema. Any other link from a status to a record,
and adrkit's `relatesTo`, SHALL be shown once on each of the two records as
related, with no direction claimed, and a pair already related by
supersession SHALL NOT be shown again as related. Links outside the status SHALL NOT relate records. A
reference the surface cannot resolve to a record SHALL be shown as written
rather than dropped.

#### Scenario: adr-tools' supersede
- **WHEN** `0001-keep-one-lock.md`'s status reads
  `Superceded by [2. Split the lock](0002-split-the-lock.md)` and
  `0002-split-the-lock.md`'s reads `Accepted` then
  `Supercedes [1. Keep one lock](0001-keep-one-lock.md)`
- **THEN** 0001 SHALL be listed as superseded by 0002, 0002 SHALL name 0001
  as what it supersedes, and neither SHALL show them as merely related

#### Scenario: OpenSpec's ADR schema, prior record untouched
- **WHEN** `0002-use-postgres.md` holds `- Supersedes: ADR-0001` and
  `0001-use-sqlite.md` still reads `accepted`
- **THEN** 0001 SHALL be listed as superseded by 0002

#### Scenario: adrkit's relations
- **WHEN** a record's front matter holds `supersedes: ["0005"]` and
  `relatesTo: ["0002"]`
- **THEN** 0005 SHALL be listed as superseded by it, and it and 0002 SHALL
  each show the other as related

#### Scenario: A dated plan beside the records
- **WHEN** a declared place holds `plans/2026-01-02-catalog.md`
- **THEN** it SHALL NOT be listed as a decision

#### Scenario: MADR's supersede without a link
- **WHEN** a record's front matter holds `status: "superseded by ADR-0009"`
  and `0009-pick-another.md` is in the places
- **THEN** the record SHALL be listed as superseded by 0009

#### Scenario: A link in prose
- **WHEN** a record's header holds its own `Supersedes in part: [019](…)`
  line, outside its status
- **THEN** no relation SHALL be shown between the two records

#### Scenario: A reference to a record that is gone
- **WHEN** a record's status supersedes a file the places do not hold
- **THEN** the reference SHALL be shown as written, and the record SHALL
  still be listed
