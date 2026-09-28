## ADDED Requirements

### Requirement: Delivery is derived from a measured dialect table
The system SHALL keep, per harness and per artifact kind, a version-stamped
record of what the harness discovers, which fields it honours or drops,
whether it follows a link and which placeholders it expands, each fact naming
the conformance check that proves it. Delivery SHALL derive those choices from
the record rather than from constants in each integration.

#### Scenario: A harness that drops a field is told before install
- **WHEN** an agent declares `model: haiku` and OpenCode's record says it
  drops an agent carrying that field
- **THEN** the install delivers the agent without the field and reports it
  Degraded for OpenCode, naming the field

#### Scenario: A fact that stops holding fails the nightly
- **WHEN** a new harness version stops following a linked skill root
- **THEN** the conformance check that proves the fact fails
