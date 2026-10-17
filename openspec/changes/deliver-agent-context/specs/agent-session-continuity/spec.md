## MODIFIED Requirements

### Requirement: A managed agent's conversation is recorded against its task
The system SHALL record, for every agent it launches, in the space's root
or in a checkout of its own, the conversation its harness started and the
harness it belongs to. The record SHALL live in UZE's state beside the
agent's, never in a checkout, keyed by the agent's identity. It SHALL hold
no harness credential and no conversation text but the agent's current
briefing, which is removed with the agent's conversations.

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
- **THEN** its continuity record names its conversations and holds the current briefing's text, and nothing else of any conversation

#### Scenario: The briefing goes with the conversations
- **WHEN** an agent's conversations are forgotten
- **THEN** its briefing is gone with them

## ADDED Requirements

### Requirement: A briefing is part of the agent's context until its work moves on
The system SHALL keep an agent's briefing in the agent's continuity record.
It SHALL include the briefing in the agent's context on every launch on the
harness it was written for. It SHALL stop when the agent's branch gains a
commit made after the switch, or when a later switch replaces the briefing.
A launch on any other harness, and a launch carrying the operator's own
arguments, SHALL neither give the briefing nor change it.

#### Scenario: A launch on another harness carries none
- **WHEN** an agent with a briefing written for one harness is launched on a different harness
- **THEN** that launch's context carries no briefing, and the briefing is kept

#### Scenario: A restore still carries it
- **WHEN** the harness a briefing was written for is launched for that agent twice before the agent commits
- **THEN** both launches' contexts carry the briefing

#### Scenario: The agent's first commit retires it
- **WHEN** the agent's branch gains a commit after the switch
- **THEN** later launches carry no briefing

#### Scenario: The next switch replaces it
- **WHEN** an agent with a briefing is switched again
- **THEN** its context carries only the briefing written for the new switch

## REMOVED Requirements

### Requirement: A pending briefing is given by the launch it was meant for
**Reason**: A briefing is no longer a first turn spent once. It is given
again on every launch on its harness until the agent commits, and one
channel (OpenCode) keeps no copy of it in the conversation. So it must
outlive the first launch, and it becomes a record rather than a generated
file that one first turn consumes.
**Migration**: Covered by "A briefing is part of the agent's context until
its work moves on". The give-once first turn remains only for a harness
whose context delivery is declared degraded.
