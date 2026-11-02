## MODIFIED Requirements

### Requirement: A document from an older version is carried across, not set aside

From the first stable release, when this build knows the shape a document
was written in, it SHALL carry that document across to the shape it writes
now, keep it, and say nothing. While UZE is in beta no shape is carried
across: a document from an earlier shape SHALL meet the rule for its class,
as one this build cannot carry across does.

#### Scenario: Upgrading with work in flight

- **WHEN** the operator upgrades a stable UZE while agents, spaces and
  checkouts exist, and every document's version is one this build knows
  the shape of
- **THEN** the next run SHALL show the same spaces, agents and work
- **AND THEN** nothing SHALL be reported: nothing was lost

#### Scenario: A field the newer shape no longer carries

- **WHEN** a stable build reads a document whose older shape carries a
  field the current shape dropped
- **THEN** the rest of the document SHALL be carried across and kept
- **AND THEN** only that field SHALL be lost

#### Scenario: Several versions behind

- **WHEN** a stable build reads a document more than one version older
  than the current one
- **THEN** it SHALL be carried across every intervening shape in order
- **AND THEN** the result SHALL be what it would have been had the
  operator upgraded one release at a time

#### Scenario: A beta build meets an earlier shape

- **WHEN** a beta build reads a document whose version is older than the
  one it writes
- **THEN** the rule for the document's class applies: a rebuildable one is
  set aside and recorded again, an irreplaceable one refuses
- **AND THEN** no step carries it across

#### Scenario: A shape this build cannot carry across

- **WHEN** a document's version is older than this build's and no step
  from it exists
- **THEN** the rule for the document's class applies, as it does for bytes
  that are not the document at all

### Requirement: A version change is written once, as a step between shapes

From the first stable release, what a version change means SHALL be written
in one place, as a step from one shape to the next, and a step SHALL be
removable once no machine can be below it. While UZE is in beta a shape
change SHALL replace the shape, with no step and no earlier shape kept.

#### Scenario: A document changes shape

- **WHEN** a change gives a document UZE owns a new shape in a stable
  release
- **THEN** what the change means SHALL be written as a step from the
  previous shape to the new one, beside the steps before it
- **AND THEN** every reader of the document SHALL reach it through those
  steps rather than through knowledge kept in its own code

#### Scenario: A document changes shape during beta

- **WHEN** a change gives a document UZE owns a new shape while UZE is in
  beta
- **THEN** the new shape replaces the old one and no step is written
- **AND THEN** a document in the old shape meets the rule for its class

#### Scenario: No machine can be below a step

- **WHEN** no machine can still hold a document in the shape a step
  starts from
- **THEN** that step MAY be removed without changing what any build does
  with a document it can still meet

### Requirement: A release proves its own upgrade against the previous one

From the first stable release, the suite SHALL include a scenario per class
of state in which the previously released UZE creates the state and this
build is then run against it, checked by what it leaves on the machine
rather than by what it reports. During beta only the refusal of a newer
build's document SHALL be proven this way.

#### Scenario: The previous release's workspace is opened by this build

- **WHEN** the last stable release has created a workspace, agents and
  their checkouts on a machine
- **AND WHEN** this build is opened on the same machine
- **THEN** the agents' branches and commits SHALL still be there
- **AND THEN** the operator SHALL be able to create an agent without any
  intervening step

#### Scenario: An older beta binary meets a newer one's record

- **WHEN** an older beta binary runs on a machine where a newer build
  wrote a record
- **THEN** that record SHALL be left exactly as it is

#### Scenario: The previous release predates the recovery being proven

- **WHEN** the released binary is older than the mechanism a scenario
  needs — it records no claimant, or writes a document of a schema this
  build also writes
- **THEN** the scenario SHALL prove what this build *reports* in that
  state, which is what the operator meets
- **AND THEN** it SHALL NOT be written as though the recovery ran, and
  SHALL name the release from which the recovery itself becomes provable

#### Scenario: A new state document has no upgrade scenario

- **WHEN** a change after the first stable release introduces a document
  UZE persists for itself
- **THEN** the change SHALL be accompanied by the scenario that proves
  what this build does with the previous version of it

## ADDED Requirements

### Requirement: No name UZE owns stands for two things

A name UZE owns SHALL say what it holds within the directory it sits in.
No file name SHALL be used in two tiers, and no directory SHALL hold a file
and a directory of one stem, so the answer to "can I delete this" never
depends on which of two same-named things is meant. A lock guarding one
document SHALL share that document's stem.

#### Scenario: One file name in two tiers

- **WHEN** a change adds a file whose name is already used by a file in
  another tier
- **THEN** the build fails, naming both paths

#### Scenario: A file beside a directory of its stem

- **WHEN** a change adds a file and a directory of one stem in the same
  directory, as `attachments.json` once sat beside `attachments/`
- **THEN** the build fails, naming both paths

#### Scenario: A record and its guard

- **WHEN** the operator lists a project's records
- **THEN** each lock that guards one record sits beside it under the
  record's own name

### Requirement: What a running agent reported survives maintenance

What an agent reported about itself, and the lock its reports are appended
under, SHALL NOT be removed by maintenance while the agent is still
recorded, and no lock UZE holds SHALL sit where maintenance sweeps.

#### Scenario: Doctor while an agent is reporting

- **WHEN** `uze doctor` runs while a recorded agent is appending a report
- **THEN** the agent's reports are all still there afterwards
- **AND THEN** the append still excludes any other writer

#### Scenario: An agent that is no longer recorded

- **WHEN** maintenance runs after an agent's record is gone
- **THEN** that agent's reports are removed
