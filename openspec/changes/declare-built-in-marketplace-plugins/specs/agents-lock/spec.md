## ADDED Requirements

### Requirement: The built-in marketplace's plugins are pinned by the binary
A locked plugin of the built-in marketplace SHALL carry the integrity of
the bytes the running binary holds for it, and the lock SHALL record the
uze version that resolved them. The built-in marketplace SHALL have no
marketplace entry: it has no repository and no commit.

#### Scenario: Resolving a built-in plugin records the version and bytes
- **WHEN** a project's `uze` plugins are resolved
- **THEN** the lock records the running uze version, and each plugin
  under `marketplace: uze` with the integrity of the binary's bytes

#### Scenario: A lock with no built-in plugin records no version
- **WHEN** a project declares no plugin of the built-in marketplace
- **THEN** its lock carries no uze version

### Requirement: Built-in plugins follow the binary, never fail on it
A built-in plugin whose bytes on the machine differ from the running
binary's SHALL be replaced by the binary's on install and the lock
rewritten, reported as updated: the binary is the only source, so a
difference is another uze build, never a reason to refuse.

#### Scenario: A lock another uze build wrote is brought to this binary
- **WHEN** a project's lock pins a built-in plugin to bytes other than
  the running binary's
- **THEN** install delivers the binary's bytes, the lock records them and
  the running version, and the plugin is reported as updated

#### Scenario: Nothing moves when the binary agrees
- **WHEN** the machine, the lock and the binary hold the same bytes
- **THEN** install reports no change for that plugin

## MODIFIED Requirements

### Requirement: Marketplace source types
A locked marketplace's source SHALL be `git` or `path`, mirroring the
acquisition mechanisms a project declares. The marketplace built into uze
SHALL have no marketplace entry; its plugins are pinned as the built-in
requirements say.

#### Scenario: Each source type parses to its own variant
- **WHEN** a lock declares a marketplace with `git:` or `path:`
- **THEN** it parses into the corresponding source, carrying that
  source's own fields

### Requirement: Resolved revision reflects what was actually acquired
A locked marketplace's or plugin's `resolved.revision` SHALL be derived
from what acquisition actually observed, not fabricated — present for a
Git commit, absent for a local path (which has no stable revision to
pin). The built-in marketplace records the uze version instead.

#### Scenario: Git source records a commit
- **WHEN** a marketplace or plugin is acquired from a Git source
- **THEN** `resolved.revision` holds that acquisition's resolved commit

#### Scenario: Built-in source records the uze version
- **WHEN** a plugin is acquired from the marketplace built into uze
- **THEN** the lock records the running uze version, not a revision

#### Scenario: Local source records no revision
- **WHEN** a marketplace or plugin is acquired from a local path
- **THEN** `resolved.revision` is absent, not a fabricated value
