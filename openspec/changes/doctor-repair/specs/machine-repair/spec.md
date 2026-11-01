## Purpose

Gives a person one deterministic command that puts UZE's own machine state
back into working order after part of it was deleted or damaged, without
depending on an agent, a harness or the workspace.

## ADDED Requirements

### Requirement: Doctor repairs on request
`uze doctor --repair` SHALL plan a repair for every finding in its lane from
live inspection, execute the plan under the mutation lock, observe again,
and report each finding as repaired, needing the person (with the reason
and the command), needing the network, or unrepairable. It SHALL run no
bootstrap, and running it again on a repaired machine SHALL change nothing.

#### Scenario: Nothing to do
- **WHEN** `uze doctor --repair` runs on a healthy machine
- **THEN** it changes nothing and reports the machine healthy

#### Scenario: Run twice
- **WHEN** `uze doctor --repair` runs twice after damage
- **THEN** the second run reports nothing repaired

### Requirement: Every owned thing has a health check
Every path UZE owns, every receipt kind and every vendor registration UZE
makes SHALL have a health check naming its repair lane: automatic,
`--repair`, or none with the reason. A path or kind added without one SHALL
fail the build.

#### Scenario: A new owned path
- **WHEN** a path is added to the map of what UZE owns without a health check
- **THEN** the coverage test fails naming it

#### Scenario: A new receipt kind
- **WHEN** a receipt kind or a vendor registration kind is added without a health check
- **THEN** the coverage test fails naming it

### Requirement: Missing package bytes are acquired again as they were
`--repair` SHALL acquire missing package bytes again from the provenance the
registry recorded, and SHALL keep them only when their digest equals the
digest recorded at ingest. A mismatch, or a source that no longer exists,
SHALL be reported as needing the person. Offline, it SHALL be reported as
needing the network.

#### Scenario: Deleted sources
- **WHEN** `sources/` was deleted and the network is reachable
- **THEN** every package's bytes are back with the digest recorded at ingest, and no version changes

#### Scenario: The source moved on
- **WHEN** a package's recorded source now yields bytes with another digest
- **THEN** repair keeps the delivered builds, adds no bytes, and reports the package with the command that updates it

#### Scenario: Offline
- **WHEN** `sources/` was deleted and the network is unreachable
- **THEN** each package is reported as needing the network, every other repair still runs, and every delivery keeps working

#### Scenario: The bundled plugin
- **WHEN** the bundled `uze` plugin's bytes are missing, including after an upgrade of the binary
- **THEN** they are restored from the snapshot inside the binary, offline, and its recorded digest becomes the snapshot's

### Requirement: A marketplace registered elsewhere is repointed
`--repair` SHALL repoint a marketplace UZE owns whose registered root is not
its vendor store, unless a session of that harness is running, in which
case it SHALL report it.

#### Scenario: A root left in the old layout
- **WHEN** Codex's UZE marketplace is still registered at a root under `runtime/`
- **THEN** repair repoints it to Codex's vendor store, and every plugin stays installed and enabled

#### Scenario: A running session
- **WHEN** a Claude Code session is running
- **THEN** repair leaves Claude's registration as it is and names the session

### Requirement: Repair never overrides the operator or destroys irreplaceable state
`--repair` SHALL NOT overwrite an artifact that drifted or conflicts, change
a revision, grant or widen trust, set aside or rewrite the receipt ledger or
an authorization, or change a project, a checkout, a branch or a ref. Each
such finding SHALL be reported with the command that resolves it.

#### Scenario: An edited build
- **WHEN** a file UZE wrote into a vendor-store build was edited by hand
- **THEN** repair leaves it and reports it, naming the file

#### Scenario: An unreadable ledger
- **WHEN** `attachments.json` cannot be read
- **THEN** repair leaves it untouched, changes no delivery, and reports the file, the reason and what the person can do

### Requirement: Every owned path declares how it is collected
Every path UZE owns SHALL declare a retention: rebuilt on demand, by age,
while something reaches it, or never. One collector SHALL delete only what a
declared disposable path's owner no longer reports alive. A path added
without a retention SHALL fail the build.

#### Scenario: A projection in use is kept
- **WHEN** a runtime projection has not changed for months but a project still uses it
- **THEN** the collector keeps it

#### Scenario: A stale extension scratch
- **WHEN** an extension program's scratch is older than its declared age
- **THEN** the collector removes it, and doctor reports the space it freed

#### Scenario: A new path without a retention
- **WHEN** a path is added to the map of what UZE owns without a retention
- **THEN** the coverage test fails naming it

### Requirement: The home is compared with its schema
`--repair` SHALL remove paths an earlier release retired, and empty
directories outside the generated and remembered tiers, except declared
paths. It SHALL report, and never delete, anything it does not recognise.
It SHALL report records the ladder set aside with their size, and remove
them only with `--discard-leftovers`.

#### Scenario: A file of an earlier release
- **WHEN** `state/profiles.json`, which an earlier release wrote and this one retired, is present
- **THEN** repair removes it and reports it

#### Scenario: Something unknown
- **WHEN** `$UZE_HOME` holds a path that is neither declared nor retired
- **THEN** repair leaves it and reports it as unknown, possibly from a newer build

#### Scenario: A set-aside record
- **WHEN** the ladder set a record aside
- **THEN** repair reports its path and size and keeps it, and `--repair --discard-leftovers` removes it

### Requirement: Doctor reports every check even when one fails
`uze doctor`, with or without `--repair`, SHALL run every check even when
another check's record cannot be read, and SHALL report each failure as a
finding of its own.

#### Scenario: One corrupt record
- **WHEN** one record UZE owns is corrupt
- **THEN** `uze doctor` reports that record and every other check's findings

### Requirement: Findings beyond automatic maintenance name the repair
Where `uze doctor` or the TUI reports a finding whose lane is `--repair`, the
action it shows SHALL be `uze doctor --repair`.

#### Scenario: Missing bytes in the TUI
- **WHEN** the TUI starts on a machine whose package bytes are missing
- **THEN** the problem it shows names `uze doctor --repair`
