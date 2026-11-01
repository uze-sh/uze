## Purpose

Gives a person one deterministic command that puts UZE's own machine state
back into working order after it was deleted, corrupted or left half-upgraded,
without depending on an agent, a harness or the workspace.

## ADDED Requirements

### Requirement: Doctor repairs on request
`uze doctor --repair` SHALL plan a repair for every finding it can repair,
execute that plan under the mutation lock, re-inspect, and report each finding
as repaired, needing the person (with the reason and the command), needing the
network, or unrepairable. Running it again on a repaired machine SHALL change
nothing.

#### Scenario: A deleted vendor store
- **WHEN** Claude Code's vendor store was deleted and `uze doctor --repair` runs
- **THEN** the store is rebuilt from the package sources, Claude Code loads every UZE-delivered plugin again, and the report lists it as repaired

#### Scenario: Nothing to do
- **WHEN** `uze doctor --repair` runs on a healthy machine
- **THEN** it changes nothing and reports the machine healthy

#### Scenario: Run twice
- **WHEN** `uze doctor --repair` runs twice after damage
- **THEN** the second run reports nothing repaired

### Requirement: Every owned thing has a health check
Every path UZE owns, every receipt kind and every vendor registration UZE
makes SHALL have a health check, and each check SHALL carry a repair or a
stated reason it has none. A path or kind added without one SHALL fail the
build.

#### Scenario: A new owned path
- **WHEN** a path is added to the map of what UZE owns without a health check
- **THEN** the coverage test fails naming it

### Requirement: Repair follows the tier
The repair of a finding SHALL follow the tier of what is damaged:
- generated and remembered things are produced again;
- a vendor store is rebuilt from the sources and re-registered;
- package bytes are acquired again at the revision their receipt pins;
- a record is carried by the ladder or set aside by its floor, never invented.

#### Scenario: Deleted package sources
- **WHEN** `sources/` was deleted and the network is reachable
- **THEN** every installed package is acquired again at the revision its receipt pins, and no version changes

#### Scenario: Offline
- **WHEN** `sources/` was deleted and the network is unreachable
- **THEN** each package is reported as needing the network, every repair that needs no bytes still runs, and the vendor stores keep working

#### Scenario: An unreadable record
- **WHEN** `attachments.json` cannot be read
- **THEN** it is set aside, never deleted, and what the vendor stores and registrations still show is reported for the person to re-adopt

### Requirement: Repair never overrides the operator
`--repair` SHALL NOT overwrite an artifact that drifted or conflicts, change
a pinned revision, grant or widen trust, delete a record, or change a
project, a checkout, a branch or a ref. Each such finding SHALL be reported
with the command that resolves it.

#### Scenario: An edited delivered skill
- **WHEN** a delivered skill directory was edited by hand
- **THEN** repair leaves it and reports it with the command that replaces it

#### Scenario: A package whose authorization is gone
- **WHEN** the record of a package's authorization was set aside
- **THEN** repair does not re-authorize it and reports the command that does

### Requirement: Doctor starts on a damaged home
`uze doctor`, with or without `--repair`, SHALL first run a recovery of the
home that needs no constructed application. That recovery finishes an
interrupted layout move, sets aside a record from a shape it cannot read, and
clears a lock whose holder is no longer alive. `--repair` SHALL run no
bootstrap.

#### Scenario: Corrupt state blocks every other command
- **WHEN** a record in `state/` is corrupt and every other `uze` command fails to start
- **THEN** `uze doctor` starts, reports the record and the repair, and `uze doctor --repair` leaves a machine on which other commands start again

#### Scenario: A stale lock
- **WHEN** the mutation lock's holder process is gone
- **THEN** doctor clears the lock and reports it, and a lock whose holder is alive is left

### Requirement: Findings beyond automatic maintenance name the repair
Where `uze doctor` or the TUI reports a finding that automatic maintenance
does not repair but `--repair` does, the report SHALL name
`uze doctor --repair` as the action.

#### Scenario: The TUI sees a missing vendor store
- **WHEN** the TUI starts and a vendor store is missing
- **THEN** the problem it shows names `uze doctor --repair`
