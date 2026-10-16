## MODIFIED Requirements

### Requirement: Continuity is declared by the harness, never assumed
Each harness integration SHALL declare how a conversation may be carried
over for it: named by UZE at launch, named by the harness and read back
afterwards, or not available. A harness whose measured behaviour is to
create a conversation under an identifier the caller supplies, when none
exists, SHALL be named by UZE at launch, in the identifier shape that
harness accepts, and the name SHALL be recorded before the harness starts;
reading the identifier back SHALL be reserved for harnesses that accept
none. Naming at launch SHALL NOT stop the record following the agent to
another conversation it starts or switches to inside the harness. The
system SHALL launch an agent for a harness that declares none as a fresh
conversation, and SHALL state on that agent that its conversation is not
carried over. The system SHALL NOT present a conversation as continued
when it was not.

#### Scenario: A harness without a mechanism is honest about it
- **WHEN** an agent is relaunched for a harness that declares no continuity
- **THEN** it starts fresh and the agent's tab states that the conversation was not carried over

#### Scenario: A harness that names its own conversation is still recorded
- **WHEN** an agent runs for a harness that does not let UZE name the conversation
- **THEN** the identifier the harness chose is read back from that harness's own records and recorded against the task

#### Scenario: A harness that accepts a caller's identifier is named by UZE
- **WHEN** a managed agent is first launched for a harness that creates a conversation under an identifier the caller supplies
- **THEN** UZE supplies an identifier in that harness's shape and records it against the task before the harness runs
- **AND** relaunching the agent continues that same conversation

#### Scenario: The agent starts another conversation inside the harness
- **WHEN** an agent named at launch starts or switches to another conversation from inside the harness
- **THEN** the record follows it to that conversation

## ADDED Requirements

### Requirement: A recorded conversation is looked up by its identifier
The system SHALL decide whether a harness still holds a recorded
conversation by asking for that conversation by its identifier wherever
the harness can answer that question, never by searching a bounded listing
of the harness's conversations. A conversation SHALL NOT be treated as
gone because the harness holds more conversations than one listing
returns.

#### Scenario: More conversations than one listing returns
- **WHEN** an agent is relaunched into a task whose recorded conversation exists in the harness but falls outside the first page of the harness's own listing
- **THEN** the conversation is continued, not started fresh
