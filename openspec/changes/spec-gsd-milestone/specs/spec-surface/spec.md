## MODIFIED Requirements

### Requirement: Units are offered as three subjects
The surface SHALL offer at most three subjects of units: the **changes**,
the **specs** that outlive them, and the **decisions** the project keeps.
A subject no detected tool keeps, or no declared place holds, SHALL NOT be
offered; one kept but empty SHALL be offered and say so. Changes SHALL be
listed one group per unit, specs by capability path. Changes SHALL be the
subject the surface opens on, unless a GSD milestone is open and the agent
has no unit of its own.

#### Scenario: Opening the surface
- **WHEN** the surface opens on an OpenSpec checkout
- **THEN** it SHALL show the changes subject, one group per directory
  under `openspec/changes/` other than `archive`, and after them the
  archived band, folded

#### Scenario: Never more than three
- **WHEN** a checkout carries every shipped tool's marker, its declared
  places hold decisions, and no GSD milestone is open
- **THEN** exactly changes, specs and decisions SHALL be offered

#### Scenario: Switching to the decisions
- **WHEN** the decisions subject is chosen
- **THEN** every decision in the declared places SHALL be listed, in the
  order their file names give, each with its status

## ADDED Requirements

### Requirement: An open GSD milestone is a subject of its own
When GSD is detected and `.planning/STATE.md` names a milestone whose
status is not complete, completed, shipped or archived, the surface SHALL offer a
**milestone** subject before the others. It SHALL list an overview, then
one row per roadmap phase with its plans done out of all, the current
phase marked. A phase the roadmap gives a `### Phase` heading and no
checklist line SHALL be listed with the others, in number order. The surface SHALL open on it when the agent has no unit of
its own. A finished milestone SHALL NOT be offered.

#### Scenario: An open milestone
- **WHEN** the surface opens on a GSD checkout whose state has a milestone
  in progress and the agent works on no phase
- **THEN** the milestone subject SHALL be shown, on its overview

#### Scenario: A finished milestone
- **WHEN** the state's milestone status is complete or completed
- **THEN** the milestone subject SHALL NOT be offered

#### Scenario: A phase added under Phase Details alone
- **WHEN** the roadmap's checklist lists phases 3 and 4 and its Phase
  Details also hold `### Phase 3.1` and `### Phase 5`
- **THEN** the rows SHALL be phases 3, 3.1, 4 and 5, in that order

### Requirement: A phase is shown by its waves and its next step
The overview SHALL render the milestone's progress, where it stopped, a
table of its phases (number, name, plans done out of all, and whether each
is done, current, in progress or not started), the current phase's plans,
its blockers and pending todos, and end with the step GSD would run next.
A phase SHALL open on where it stands and its next step, then render its
goal, its success criteria, and its plans as a table, one row each with
its wave, its first sentence and its mark: a plan with a summary done, the
first open wave current, a plan that is not autonomous marked as stopping
for the operator. The step SHALL be chosen from what the phase holds, and
be the milestone audit once every phase is done.

#### Scenario: A phase half executed
- **WHEN** a phase has two waves and only the first wave's plans have
  summaries
- **THEN** the first wave's rows SHALL be shown done, the second's current,
  and the next step SHALL be to execute that phase

#### Scenario: A plan described at length
- **WHEN** a plan's objective or summary runs to a paragraph
- **THEN** its row SHALL show the first sentence, cut to one line

#### Scenario: Every phase done
- **WHEN** every roadmap phase is checked off
- **THEN** the next step SHALL be the milestone audit

### Requirement: A step is typed into the agent and never sent
Activating a phase SHALL offer its discuss, plan, execute and verify steps,
the next one highlighted; activating the overview SHALL offer the next
step and progress. Picking one SHALL close the surface and type the skill
and its phase into the focused agent's prompt, spelled the way that
agent's harness calls a skill, without control characters and without
submitting it. The surface SHALL NOT write any file.

#### Scenario: Planning a phase
- **WHEN** plan is picked on phase 3 with an agent that calls skills with `/`
- **THEN** `/gsd-plan-phase 3` SHALL be in its prompt, unsent, and the
  surface SHALL be closed

#### Scenario: A harness with another sigil
- **WHEN** the same step is picked with an agent that calls skills with `$`
- **THEN** `$gsd-plan-phase 3` SHALL be typed instead
