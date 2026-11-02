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
