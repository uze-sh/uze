## ADDED Requirements

### Requirement: A harness below the version UZE needs is reported, never worked around
Each harness integration SHALL declare the oldest harness version its
deliveries are written for. When a detected harness reports an older
version, `uze doctor` and `uze status` SHALL name the harness, its version
and the version needed, and every operation that would fail because of it
SHALL say so instead of failing opaquely. The system SHALL NOT keep an
older delivery path alive for a version below that floor. A harness whose
version cannot be read SHALL be treated as meeting the floor.

#### Scenario: A harness below its floor
- **WHEN** a detected harness reports a version older than its integration declares
- **THEN** `uze doctor` and `uze status` report it as below the version UZE needs, naming both versions

#### Scenario: An install refused because of the floor
- **WHEN** a package install reaches a harness below its floor and that harness's delivery cannot proceed
- **THEN** the refusal names the version needed, not only that the delivery was refused

#### Scenario: Unreadable version
- **WHEN** a detected harness's version cannot be read
- **THEN** nothing is reported about its floor
