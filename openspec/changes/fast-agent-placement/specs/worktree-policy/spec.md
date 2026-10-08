## RENAMED Requirements

- FROM: `### Requirement: An agent is placed on the target as the remote has it`
- TO: `### Requirement: An agent is placed on the target as the last sync left it`

## MODIFIED Requirements

### Requirement: An agent is placed on the target as the last sync left it
The system SHALL bring the local target of a project that isolates its
agents in line with the remote's on the workspace's clock, by fast-forward
and by nothing else, and SHALL NOT ask the remote while placing an agent:
a new agent's branch SHALL be cut from the local target as the last sync
left it. A target carrying commits the remote lacks, one Git refuses to
move, and one checked out in the primary checkout SHALL be left exactly as
they stand, and the system SHALL say once that new agents start behind
until the target catches up. A repository with no remote, or whose target
the remote does not have, SHALL be placed from its local target with
nothing to report. A delivery SHALL fetch the target for itself.

#### Scenario: A placement does not wait on the remote
- **WHEN** the remote's target has moved since the last sync and an agent is created
- **THEN** the agent's branch is cut from the local target without contacting the remote

#### Scenario: A new agent starts from what the remote has
- **WHEN** a sync ran after the remote's target moved and the target is not checked out in the primary checkout
- **THEN** the local target is fast-forwarded onto the remote's tip and the next agent's branch is cut from there

#### Scenario: The operator's checkout is never moved by the clock
- **WHEN** the operator has the target checked out in the primary checkout and the remote has moved
- **THEN** the sync leaves the target and the working tree as they stand and says once that new agents start behind

#### Scenario: The operator's own commits are never rewritten to sync
- **WHEN** the local target carries commits the remote does not and the remote has moved
- **THEN** the local target is left where it stands and the system says how far behind new agents start

#### Scenario: A repository with no remote is placed from its own target
- **WHEN** an agent is created in a repository with no remote
- **THEN** the agent is placed from the local target and nothing is reported

