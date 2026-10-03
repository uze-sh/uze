## ADDED Requirements

### Requirement: Delivery is derived from a measured dialect table
The system SHALL keep, per harness, a table of measured facts about what the
harness discovers, follows, expands and accepts, each fact stating the
harness version it was measured on and the conformance check that proves it.
A test SHALL fail a fact that names no measured version, or whose check is
not a function the conformance Lab defines, and the table SHALL be rendered
into the published harness matrix. The fields a harness accepts on an agent
SHALL be declared as data in that harness's vertical (its agent dialect), and
that one declaration SHALL render the delivered file, answer `plugin check`
and state the loss at install, so the three never disagree.

#### Scenario: A harness that drops a field is told before install
- **WHEN** an agent declares `model: haiku` at the root and OpenCode's
  agent dialect does not carry it
- **THEN** the install delivers the agent without the field and reports it
  Degraded for OpenCode, naming the field

#### Scenario: A fact names a check the Lab defines
- **WHEN** a harness states a fact whose `proven_by` names a function the
  conformance tree does not define, or the fact names no measured version
- **THEN** the workspace test suite fails, naming the harness and the fact

#### Scenario: A fact proved by the contract is rechecked by the conformance run
- **WHEN** a fact names a check under `conformance/contract/` and a new
  harness version stops behaving as the fact says
- **THEN** that contract check fails in the conformance run, while a fact
  proved only by an experiment under `conformance/experiments/` is
  rechecked only when that experiment is run on demand
