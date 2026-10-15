# delivery-reporting Specification

## Purpose
Say truthfully how each installed package reached each harness: the route it
took, what fell short of native and why, what failed, and whether what was
delivered still matches what was planned.
## Requirements
### Requirement: The install report names every delivery

After installing a package, the system SHALL report, for each detected
harness, the delivery route taken (the package's own envelope, a generated
envelope, or capability by capability) and the reason for that route, and
every artifact recorded for that harness without collapsing them per harness.
A harness the delivery failed on SHALL be named with its error. The human and
JSON outputs SHALL carry the same facts.

#### Scenario: Generated envelope

- **WHEN** a plugin that ships no manifest Claude Code reads is installed and
  Claude Code is detected
- **THEN** the report names Claude Code, the generated envelope and why it
  was chosen, and every artifact recorded for it

### Requirement: The report names every capability short of native

After installing a package, the system SHALL report, per harness, each
capability that was not delivered because a name it needs is held by
something UZE does not own, and each capability delivered short of native
with its route (adapted, degraded or unsupported) and what it lost. An update
SHALL report, per harness, each capability not delivered or delivered short
of native.

#### Scenario: A name held by something else

- **WHEN** a skill's name is already held on a detected harness by something
  UZE does not own
- **THEN** the report names that skill as not delivered on that harness and
  says why

#### Scenario: A field the target cannot read

- **WHEN** an agent carries a frontmatter field the target harness does not
  accept
- **THEN** the report marks that agent degraded for that harness and names
  the field that was not carried

### Requirement: The effective view needs no session

The system SHALL answer, for an installed package and optionally one
harness, the route each harness would take and every capability of the
package with its route, without attaching anything, writing the
attachment ledger or opening a session in the harness. Narrowed to one
harness, it SHALL also name each capability as a session there sees it. Its
route and its capabilities short of native SHALL agree with what the install
reported.

#### Scenario: Inspecting before publishing

- **WHEN** an author runs `uze inspect flow --harness claude`
- **THEN** the output lists every capability of `flow` with its route for
  Claude Code and the name a Claude Code session sees it under
- **AND** nothing is recorded in the attachment ledger

#### Scenario: Checking a recorded registry entry

- **WHEN** the effective view checks a registry entry recorded for a harness
- **THEN** it may ask the harness's own listing command, through the same
  inspection cache `uze doctor` uses
- **AND** no session is opened in the harness

### Requirement: A failed install leaves nothing installed

When installing a package attaches it to no harness, the system SHALL remove
the package from the Store, if this install created it, and every artifact
and receipt the attempt created, and report the failure.

#### Scenario: The only harness refuses

- **WHEN** a package's install fails on the only detected harness
- **THEN** `uze status -m` does not list the package
- **AND** `uze doctor` reports no receipt for it

#### Scenario: A failed reinstall keeps what was there

- **WHEN** a package is already installed and a reinstall of it attaches to
  no harness
- **THEN** the package stays installed

### Requirement: A partial install is recorded with each failing harness

When a package attaches to some harnesses and fails on others, the system
SHALL keep the package and record it as partially delivered with each failing
harness and its error; `uze status -m` and `uze inspect` SHALL show that
state until a later install or update delivers it fully, and `uze doctor`
SHALL name each such harness and its error and not judge that harness's
delivery.

#### Scenario: One of two harnesses refuses

- **WHEN** a package attaches to one harness and fails on another
- **THEN** `uze status -m` lists the package as partially delivered, naming
  the failing harness and the error
- **AND** `uze doctor` names that harness and its error without judging its
  delivery

### Requirement: Update reports what changed in the bytes

The system SHALL decide whether an update changed a package by comparing the
package's content before and after, for every source kind, and SHALL NOT
report a package as current when its Store content changed.

#### Scenario: Linked marketplace edit

- **WHEN** a skill is edited in the working tree of a linked marketplace and
  `uze update <plugin> -m` runs
- **THEN** the report says the package was updated

### Requirement: Doctor checks delivery against intent

For each installed package and each detected harness it was delivered to,
the system SHALL compare the capabilities the delivery plan expects with
their receipts and inspection and with what the integration knows the
harness would not load, and SHALL report, naming harness and
capability, each capability with no recorded delivery as missing, each
delivered under a name other than the expected one as renamed, and each
artifact the harness would not load as unreadable.

#### Scenario: The expected name is the recorded one

- **WHEN** a capability was already delivered under a name its receipt
  records
- **THEN** `uze doctor` takes that recorded name as the expected one when
  judging it renamed

#### Scenario: A delivered agent file is gone

- **WHEN** the file a UZE-delivered agent was written to is deleted
- **THEN** `uze doctor` reports that agent as missing on that harness

#### Scenario: A hook entry under another name

- **WHEN** the hook entry UZE wrote into a harness's configuration, an entry
  the harness itself holds by name, is found under a name other than the one
  its receipt records
- **THEN** `uze doctor` reports that hook as renamed on that harness

#### Scenario: Empty plugin cache

- **WHEN** Claude Code's records point at a cached copy of a UZE-delivered
  plugin that holds none of its skills or agents
- **THEN** `uze doctor` reports each of the package's skills and agents as
  unreadable on Claude Code, naming the cached copy

#### Scenario: An agent the harness drops

- **WHEN** an OpenCode agent file carries a `model` that is not
  `provider/model`, a `tools` that is not a map, or no `mode: subagent`
- **THEN** `uze doctor` reports that agent as unreadable on OpenCode, naming
  the field

