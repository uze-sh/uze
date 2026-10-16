## MODIFIED Requirements

### Requirement: A managed agent's conversation is recorded against its task
The system SHALL record, for every agent it launches, in the space's root or
in a checkout of its own, the identifier of the conversation its harness
started and the harness it belongs to. The record SHALL live in UZE's own
state beside the agent's record, never inside any checkout, keyed by the
agent's identity so it survives the checkout being reset, moved or removed
and the agent being isolated. The record SHALL NOT hold any harness
credential or conversation content; a switch's briefing is the only place
UZE writes conversation text.

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

## ADDED Requirements

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
