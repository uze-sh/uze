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

### Requirement: The record holds the briefing and no other conversation text
An agent's continuity record SHALL hold conversation identifiers and, while
one applies, the agent's briefing. It SHALL hold no other text of any
conversation. A briefing SHALL be removed with the agent's conversations.

#### Scenario: What the record holds
- **WHEN** an agent has been switched between harnesses
- **THEN** its continuity record names its conversations and holds the briefing's text, and nothing else of the conversations

#### Scenario: Removed with the agent
- **WHEN** an agent's conversations are forgotten
- **THEN** its briefing is gone with them

## REMOVED Requirements

### Requirement: A pending briefing is given by the launch it was meant for
**Reason**: A briefing is no longer a first turn spent once. It is part of
the agent's context, which a resumed conversation needs again because the
channel does not write it into the harness's transcript. It is therefore a
record, no longer a generated file one first turn consumes.
**Migration**: Covered by "A briefing is part of the agent's context until
its work moves on". The give-once first turn remains only for a harness
whose context delivery is declared degraded.
