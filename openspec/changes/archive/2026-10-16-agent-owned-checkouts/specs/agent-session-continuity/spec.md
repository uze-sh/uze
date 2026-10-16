## MODIFIED Requirements

### Requirement: Relaunching an agent continues its task's conversation
The system SHALL start an agent whose task already has a conversation for
that harness by resuming that conversation, and SHALL start a fresh one
only when the task has none. This SHALL hold however the agent is
relaunched — by the operator from the interface, or by the terminal
runtime restoring a workspace after a restart, with no client present.
When the operator resumes an agent whose checkout was collected or handed
to another agent while it was away, the agent SHALL come back at its own
path, which is where every harness keeps the conversation.

#### Scenario: The workspace comes back after a restart
- **WHEN** the terminal runtime restores an agent tab after its server was restarted
- **THEN** the agent resumes the conversation its task was left in, not a new one

#### Scenario: A preserved task is picked up again
- **WHEN** the operator resumes a preserved task and chooses a harness that already worked in it
- **THEN** the agent starts in that task's conversation

#### Scenario: A task with no conversation for that harness starts one
- **WHEN** an agent is launched into a task the chosen harness has never worked in
- **THEN** a fresh conversation starts and is recorded against the task

#### Scenario: The checkout was collected while the agent was away
- **WHEN** an agent's checkout was removed as idle and the agent is resumed
- **THEN** a checkout is placed at the agent's own path again and the agent continues its conversation

#### Scenario: The slot was handed to another agent meanwhile
- **WHEN** the slot an agent left was given to a new agent, and the first agent is resumed
- **THEN** the first agent is placed at its own path, not in the slot the new agent holds, and continues its conversation

### Requirement: Every harness proves continuity against its real binary
Conformance SHALL state the continuity outcome in vendor-neutral terms and
run it against every supported harness's real binary: a conversation
started in a task, its process ended, an agent launched into that same task
again, and something from the earlier turn present in the new process —
both when the checkout stayed and when it was removed and placed again at
the agent's path in between. A
harness that cannot deliver it SHALL declare it unsupported with a reason
that the run records, never by omitting the check.

#### Scenario: A supported harness carries a turn across a relaunch
- **WHEN** the continuity check runs against a harness that declares the capability
- **THEN** the relaunched process answers from the conversation the first process started

#### Scenario: An unsupported harness is recorded as such
- **WHEN** the continuity check runs against a harness that declares no mechanism
- **THEN** the run records it as unsupported with the reason, and the vertical does not fail for it

#### Scenario: A relaunch after the checkout was placed again
- **WHEN** the continuity check removes the task's checkout between the two processes and places it again at the agent's path
- **THEN** the relaunched process answers from the conversation the first process started
