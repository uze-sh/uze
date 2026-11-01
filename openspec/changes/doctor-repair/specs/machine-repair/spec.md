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
its vendor store, through the harness's own CLI, and SHALL leave every
plugin installed and enabled. It SHALL NOT wait for or look for a running
session of that harness, as the vendor-store migration does not.

#### Scenario: A root left in the old layout
- **WHEN** Codex's UZE marketplace is still registered at a root under `runtime/`
- **THEN** repair repoints it to Codex's vendor store, and every plugin stays installed and enabled

### Requirement: Repair never overrides the operator or destroys irreplaceable state
`--repair` SHALL NOT overwrite an artifact that drifted or conflicts, change
a pinned revision (restoring the commit pinned before a marketplace link is
not a change), grant or widen trust, set aside or rewrite the receipt ledger or
an authorization, or change a project, a checkout, a branch or a ref. Each
such finding SHALL be reported with the command that resolves it.

#### Scenario: An edited build
- **WHEN** a file UZE wrote into a vendor-store build was edited by hand
- **THEN** repair leaves it and reports it, naming the file

#### Scenario: An unreadable ledger
- **WHEN** `attachments.json` cannot be read
- **THEN** repair leaves it untouched, changes no delivery, and reports the file, the reason and what the person can do

### Requirement: Every disposable path has exactly one collector
Every generated or remembered path UZE may sweep SHALL be declared
disposable, and SHALL have exactly one collector. That collector keeps its
owner's rule and lock and reports what it freed to one report. A disposable
path without a collector SHALL fail the build. The terminal runtime's state
SHALL never be collected.

#### Scenario: A projection in use is kept
- **WHEN** a runtime projection has not changed for months but a project still uses it
- **THEN** the collector keeps it

#### Scenario: A long-running program's scratch
- **WHEN** an extension program has been running for hours
- **THEN** its scratch is kept while its process is alive, and removed once it is gone

#### Scenario: The terminal runtime
- **WHEN** maintenance or `--repair` runs while a workspace is open
- **THEN** nothing under the terminal runtime's state is removed

#### Scenario: A new disposable path without a collector
- **WHEN** a disposable path is added to the map of what UZE owns without a collector
- **THEN** the coverage test fails naming it

### Requirement: The home is compared with its schema
`--repair` SHALL remove retired paths still present, and undeclared empty
directories in the bytes, store and record tiers. It SHALL report, and never
delete, an unknown entry in those tiers or the authored ones. It SHALL report
set-aside records with their size, removing them only with
`--discard-leftovers`.

#### Scenario: A file of an earlier release
- **WHEN** `state/profiles.json`, which an earlier release wrote and this one retired, is present
- **THEN** it is removed by the next bootstrap, and `--repair` reports anything still present

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
