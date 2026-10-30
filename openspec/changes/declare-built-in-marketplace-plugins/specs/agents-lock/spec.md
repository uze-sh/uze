## ADDED Requirements

### Requirement: The built-in marketplace is pinned by the uze version
A locked built-in marketplace SHALL record the uze version that resolved
it, its only revision, and each of its plugins SHALL carry the integrity
of the bytes that version ingested.

#### Scenario: Resolving a built-in plugin records the version
- **WHEN** a built-in plugin is resolved in a project
- **THEN** the lock lists `uze` with the running uze version, and the
  plugin with the integrity of what was ingested

### Requirement: A lock another uze version wrote is resolved again
A built-in entry recorded by another uze version SHALL be resolved again
from the running binary and the lock rewritten, reported as updated,
never refused: the binary is the marketplace, and it moved. Bytes that
differ under the same version SHALL still fail as any pin does.

#### Scenario: Another version's lock is brought up to this binary
- **WHEN** a project's lock pins `uze` at a version other than the
  running one
- **THEN** its built-in plugins are installed from this binary, the lock
  records this version and the new integrity, and the install reports
  them as updated

#### Scenario: Same version, other bytes, fails
- **WHEN** the lock pins `uze` at the running version and a built-in
  plugin's bytes hash to another integrity
- **THEN** the install fails naming both hashes, and nothing is delivered
