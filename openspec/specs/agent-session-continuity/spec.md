# agent-session-continuity Specification

## Purpose

Keeps the conversation of an agent UZE launched attached to the task it
belongs to, so relaunching that agent — after a restart, a crash, or the
operator picking a preserved task back up — continues the work instead of
starting a stranger in the same checkout.

## Requirements

### Requirement: A managed agent's conversation is recorded against its task
The system SHALL record, for every agent it launches, in the space's root
or in a checkout of its own, the conversation its harness started and the
harness it belongs to. The record SHALL live in UZE's state beside the
agent's, never in a checkout, keyed by the agent's identity. It SHALL hold
no harness credential and no conversation content; a switch's briefing is
the only place UZE writes conversation text.

#### Scenario: A new agent's conversation is recorded
- **WHEN** an agent is launched for the first time
- **THEN** the conversation the harness started is recorded against that agent and that harness

#### Scenario: The record survives isolation
- **WHEN** an agent working in the space's root is isolated into a checkout of its own
- **THEN** the conversation recorded against it is the one it was already in, and the relaunched agent continues it

#### Scenario: The record survives the checkout
- **WHEN** an isolated agent's checkout is reset, or removed and given back later
- **THEN** the recorded conversation is still the one the agent started with

#### Scenario: A task carries one conversation per harness
- **WHEN** an agent has been worked in by two different harnesses
- **THEN** each harness's conversation is recorded separately and neither is offered to the other

#### Scenario: The record holds no conversation text
- **WHEN** an agent has been switched between harnesses
- **THEN** its continuity record names conversations and a pending briefing's location, and holds none of their text

### Requirement: A conversation is keyed by identity, and the record follows the agent
The system SHALL key a conversation by the identifier its harness resumes
by, never by a title, label or name a person can change. Where a harness
moves an agent into a different conversation while it runs — cleared,
forked, or switched by the person at the keyboard — the system SHALL keep
the record pointing at the conversation the agent is actually in, so what
resumes is where the work was left, not where it began.

#### Scenario: Renaming a conversation changes nothing
- **WHEN** a person renames, retitles or relabels a conversation inside the harness
- **THEN** the recorded conversation still resolves and the agent resumes it

#### Scenario: The agent is moved to another conversation while it runs
- **WHEN** the person clears, forks or switches the conversation inside a running agent, and the agent is later relaunched
- **THEN** it resumes the conversation it was in when it stopped, not the one it started in

### Requirement: Relaunching an agent continues its task's conversation
The system SHALL start an agent whose task already has a conversation for
that harness by resuming it, and a fresh one only when the task has none,
however it is relaunched: by the operator, or by the terminal runtime
restoring a workspace after a restart with no client present. An agent
whose checkout was collected or handed to another agent while it was away
SHALL come back at its own path, where every harness keeps the
conversation.

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

### Requirement: A reused checkout never inherits another task's conversation
The system SHALL bind a conversation to the task rather than to the
directory it ran in. An agent launched into a task SHALL never be given a
conversation belonging to a different task, including a task that
previously occupied the same checkout.

#### Scenario: A recycled slot starts clean
- **WHEN** a checkout that held a finished task is reused for a new task and an agent is launched into it
- **THEN** the agent starts a fresh conversation and never sees the previous task's

### Requirement: The operator's own invocation is never rewritten
The system SHALL leave an invocation the operator composed exactly as
typed. Continuity SHALL apply only where the operator asked for nothing
about sessions themselves: an invocation carrying the harness's own session
or resume arguments SHALL be passed through untouched, and an invocation
made outside a managed task SHALL behave exactly as it did before this
capability existed.

#### Scenario: An explicit session argument wins
- **WHEN** a harness is started inside a managed task with its own resume or session argument
- **THEN** that argument decides the conversation and UZE adds nothing

#### Scenario: Outside a managed task nothing changes
- **WHEN** a harness is started in a directory that belongs to no managed task
- **THEN** it starts exactly as it would without UZE, with no conversation carried over

### Requirement: Continuity is declared by the harness, never assumed
Each harness integration SHALL declare how a conversation may be carried
over for it: named by UZE at launch, named by the harness and read back
afterwards, or not available. The system SHALL launch an agent for a harness
that declares none as a fresh conversation, SHALL state on that agent that
its conversation is not carried over, and SHALL NOT present a conversation
as continued when it was not.

#### Scenario: A harness without a mechanism is honest about it
- **WHEN** an agent is relaunched for a harness that declares no continuity
- **THEN** it starts fresh and the agent's tab states that the conversation was not carried over

#### Scenario: A harness that names its own conversation is still recorded
- **WHEN** an agent runs for a harness that does not let UZE name the conversation
- **THEN** the identifier the harness chose is read back from that harness's own records and recorded against the task

### Requirement: A harness that takes the caller's identifier is named by UZE
A harness whose measured behaviour is to create a conversation under an
identifier the caller supplies SHALL be named by UZE at launch, in the shape
that harness accepts, and the name SHALL be recorded before the harness
starts; reading the identifier back SHALL be reserved for harnesses that
accept none. Naming at launch SHALL NOT stop the record following the agent
to another conversation it starts or switches to inside the harness.

#### Scenario: A harness that accepts a caller's identifier is named by UZE
- **WHEN** a managed agent is first launched for a harness that creates a conversation under an identifier the caller supplies
- **THEN** UZE supplies an identifier in that harness's shape and records it against the task before the harness runs
- **AND** relaunching the agent continues that same conversation

#### Scenario: The agent starts another conversation inside the harness
- **WHEN** an agent named at launch starts or switches to another conversation from inside the harness
- **THEN** the record follows it to that conversation

### Requirement: A conversation that cannot be resumed never blocks the launch
The system SHALL start the agent whatever continuity fails: a recorded
conversation the harness no longer holds, a harness version that does not
accept the resume argument, or unreadable state SHALL leave the agent
starting fresh, with the reason stated once. Continuity SHALL never be a
precondition for an agent starting.

#### Scenario: The recorded conversation is gone
- **WHEN** an agent is relaunched into a task whose recorded conversation no longer exists in the harness's records
- **THEN** the agent starts fresh and the reason is stated once

#### Scenario: Unreadable continuity state
- **WHEN** the recorded state for a task cannot be read
- **THEN** the agent still starts and nothing about it is guessed at

### Requirement: Continuity does not depend on the operator's environment
The system SHALL carry a conversation over for the agents it launches
without requiring any UZE-provided executable to be on the operator's PATH
and without requiring the operator to change how they start a harness by
hand.

#### Scenario: An untouched PATH still carries the conversation
- **WHEN** the operator's PATH contains only the harness's own executable and an agent is relaunched
- **THEN** its conversation is still carried over

### Requirement: Every harness proves continuity against its real binary
Conformance SHALL state the continuity outcome in vendor-neutral terms and
run it against every supported harness's real binary: a conversation
started in a task, its process ended, an agent launched into that task
again, and the earlier turn present in the new process, also when the
checkout was removed and placed again in between. A harness that cannot
deliver it SHALL declare it unsupported with a reason the run records.

#### Scenario: A supported harness carries a turn across a relaunch
- **WHEN** the continuity check runs against a harness that declares the capability
- **THEN** the relaunched process answers from the conversation the first process started

#### Scenario: An unsupported harness is recorded as such
- **WHEN** the continuity check runs against a harness that declares no mechanism
- **THEN** the run records it as unsupported with the reason, and the vertical does not fail for it

#### Scenario: A relaunch after the checkout was placed again
- **WHEN** the continuity check removes the task's checkout between the two processes and places it again at the agent's path
- **THEN** the relaunched process answers from the conversation the first process started

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

### Requirement: A pending briefing is given by the launch it was meant for
The system SHALL give a pending briefing's first turn only to a launch of
that agent on the harness it was written for, SHALL clear it in the same
change that records that launch, and SHALL leave it untouched for a launch
on any other harness. A launch carrying the operator's own arguments SHALL
neither give nor clear it.

#### Scenario: A launch on another harness leaves it pending
- **WHEN** an agent with a briefing pending for one harness is launched on a different harness
- **THEN** that launch carries no briefing turn and the briefing stays pending

#### Scenario: Given exactly once
- **WHEN** the harness a briefing was written for is launched for that agent twice
- **THEN** only the first launch carries the briefing turn
