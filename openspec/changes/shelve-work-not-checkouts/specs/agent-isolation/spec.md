## MODIFIED Requirements

### Requirement: An agent ends when no live pane carries it
The system SHALL record an agent as ended when no live pane carries its
identity, and SHALL keep the record. An isolated agent's slot SHALL be
released by the same sweep that releases a slot today, its uncommitted
work shelved first (`work-shelf`); an agent that is not isolated SHALL
leave nothing to release.

#### Scenario: The last pane of an agent closes
- **WHEN** the last pane carrying an agent's identity is closed
- **THEN** the agent is recorded as ended and its row leaves the column

#### Scenario: An isolated agent that ended holding work
- **WHEN** an isolated agent ends with commits its target does not have, or with uncommitted changes
- **THEN** its work stays reachable, on its branch and in its shelf, and is listed as shelved
- **AND** its slot is free for the next agent

#### Scenario: Work merged after its agent ended
- **WHEN** an agent ended holding commits and those commits later reach the target by an ordinary merge
- **THEN** its task is recorded as integrated, not as closed
