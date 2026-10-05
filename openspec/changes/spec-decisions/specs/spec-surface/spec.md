## RENAMED Requirements

- FROM: `### Requirement: Units are offered as three subjects`
- TO: `### Requirement: Units are offered as four subjects`

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

### Requirement: Units are offered as four subjects
The surface SHALL offer up to four subjects: the **changes** in flight,
the **specs** that outlive them, the **archive**, and the **decisions**
the project keeps. A subject none of the detected tools keeps, or, for
decisions, that no declared place holds, SHALL NOT be offered; one a tool keeps but that
holds nothing yet SHALL be offered and say so. Changes SHALL be listed one
group per unit, holding its artifacts. Specs SHALL be listed by capability
path. The archive SHALL be listed newest first. Changes SHALL be the
subject the surface opens on.

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

#### Scenario: Switching to the decisions
- **WHEN** the decisions subject is chosen
- **THEN** every decision in the declared places SHALL be listed, in the
  order their file names give, each with its status

## ADDED Requirements

### Requirement: A decision is recognised by signals that hold in any language
The surface SHALL look for decisions in every place `workspace.artifacts`
declares, as deep as each goes, and SHALL NOT require a tool's marker, a
fixed directory, or any word of a heading or a label. A Markdown file
SHALL be a decision when its name is a record's: a number of at least
three digits, a hyphen and a slug (`0042-use-yaml.md`). Its title SHALL be
the front matter's `title`, else its first `# ` heading, else its slug.
Its status SHALL be the front matter's `status`, else the first
`Label: value` line before its first section whose value is not a date,
else the single short line its first section holds, with the punctuation
that closes a sentence removed. A record with none of these SHALL be
listed with no status rather than dropped. Every other file SHALL be
ignored, without a report. A project that declares no place SHALL be
offered no decisions subject.

#### Scenario: A Nygard record written by adr-tools
- **WHEN** a declared place holds `0001-keep-one-lock.md` with a title, a
  `Date:` line and a `## Status` section holding `Accepted`
- **THEN** it SHALL be listed as `0001 Keep one lock`, with status
  `Accepted`, and the date SHALL NOT be taken for the status

#### Scenario: A record in another language
- **WHEN** a declared place holds `0001-manter-um-lock.md` whose first
  section is `## Situação` holding `Aceita.`
- **THEN** it SHALL be listed with status `Aceita`

#### Scenario: A MADR record
- **WHEN** a declared place holds a numbered file with `status: accepted`
  in its front matter
- **THEN** it SHALL be listed with status `accepted`

#### Scenario: A guide is not a decision
- **WHEN** a declared place holds `guide.md` and `README.md`
- **THEN** neither SHALL be listed, and nothing SHALL be reported about
  them

#### Scenario: Nothing declared
- **WHEN** the project's `agents.yaml` has no `workspace.artifacts`
- **THEN** no decisions subject SHALL be offered, even where a `docs/adr/`
  exists

### Requirement: Which record replaced which is said only by structured fields
A decision SHALL be listed as superseded only when a record says so in
its front matter: its own `superseded-by`, or another record's
`supersedes` naming it, by file or by number; each side SHALL name the
other. A link from a record's header to another record SHALL be shown as
a reference on both, with no claim about which way it points, because
telling "replaces" from "mentions" would mean reading the sentence around
it. A reference the surface cannot resolve to a record SHALL be shown as
written rather than dropped.

#### Scenario: A superseded decision
- **WHEN** `0002-split-the-lock.md` has `supersedes: 0001-keep-one-lock.md`
  in its front matter
- **THEN** 0001 SHALL be listed as superseded, named by 0002, and 0002
  SHALL name 0001

#### Scenario: A link in prose
- **WHEN** decision 054's header links `019-boundary.md` in a sentence
  saying it replaces part of it
- **THEN** 054 SHALL show a reference to 019, 019 SHALL show it is
  referenced by 054, and 019 SHALL NOT be listed as superseded

#### Scenario: A reference to a file that is gone
- **WHEN** a record's front matter supersedes a file the places do not
  hold
- **THEN** the reference SHALL be shown as written, and the record SHALL
  still be listed
