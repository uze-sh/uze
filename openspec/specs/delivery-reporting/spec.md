# delivery-reporting Specification

## Purpose
TBD - created by archiving change honest-delivery-reports. Update Purpose after archive.
## Requirements
### Requirement: The install report names every delivery

After installing a package, the system SHALL report, for each detected
harness, the delivery route taken (the package's own envelope, a generated
envelope, or capability by capability) and the reason for that route, every
artifact recorded for that harness without collapsing them per harness, each
capability that was not delivered because a name it needs is held by
something UZE does not own, and each capability delivered short of native
with its route (adapted, degraded or unsupported) and what it lost. A harness
the delivery failed on SHALL be named with its error. The human and JSON
outputs SHALL carry the same facts. An update SHALL report, per harness,
each capability not delivered or delivered short of native.

#### Scenario: Generated envelope

- **WHEN** a plugin that ships no manifest Claude Code reads is installed and
  Claude Code is detected
- **THEN** the report names Claude Code, the generated envelope and why it
  was chosen, and every artifact recorded for it

#### Scenario: A field the target cannot read

- **WHEN** an agent carries a frontmatter field the target harness does not
  accept
- **THEN** the report marks that agent degraded for that harness and names
  the field that was not carried

### Requirement: The effective view needs no session

The system SHALL answer, for an installed package and optionally one
harness, the route each harness would take and every capability of the
package with its route, without attaching anything, writing the
attachment ledger or opening a session in the harness; checking a recorded
registry entry may ask the harness's own listing command, through the same
inspection cache `uze doctor` uses. Narrowed to one harness, it SHALL also name each
capability as a session there sees it. Its route and its capabilities short
of native SHALL agree with what the install reported.

#### Scenario: Inspecting before publishing

- **WHEN** an author runs `uze inspect flow --harness claude`
- **THEN** the output lists every capability of `flow` with its route for
  Claude Code and the name a Claude Code session sees it under
- **AND** nothing is recorded in the attachment ledger

### Requirement: A failed install leaves nothing installed

When installing a package attaches it to no harness, the system SHALL remove
the package from the Store, if this install created it, and every artifact
and receipt the attempt created, and report the failure. When the package
attaches to some harnesses and fails on others, the system SHALL keep the
package and record it as partially delivered with each failing harness and
its error; `uze status -m` and `uze inspect` SHALL show
that state until a later install or update delivers it fully, and `uze
doctor` SHALL name each such harness and its error and not judge that
harness's delivery.

#### Scenario: The only harness refuses

- **WHEN** a package's install fails on the only detected harness
- **THEN** `uze status -m` does not list the package
- **AND** `uze doctor` reports no receipt for it

#### Scenario: One of two harnesses refuses

- **WHEN** a package attaches to one harness and fails on another
- **THEN** `uze status -m` lists the package as partially delivered, naming
  the failing harness and the error

#### Scenario: A failed reinstall keeps what was there

- **WHEN** a package is already installed and a reinstall of it attaches to
  no harness
- **THEN** the package stays installed

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
the system SHALL compare the capabilities the delivery plan expects with the
receipts that record them and their inspection, and with what the
integration knows the harness would not load, and SHALL report each
capability with no recorded delivery as missing, each delivered under a
name other than the expected one as renamed, and each artifact the harness
would not load as unreadable, with the harness and the capability named.
The expected name of a capability already delivered is the one its receipt
records, so renamed is reported for an entry the harness itself holds by
name, such as a hook entry in its configuration.

#### Scenario: A delivered agent file is gone

- **WHEN** the file a UZE-delivered agent was written to is deleted
- **THEN** `uze doctor` reports that agent as missing on that harness

#### Scenario: A hook entry under another name

- **WHEN** the hook entry UZE wrote into a harness's configuration is found
  under a name other than the one the plan expects
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

