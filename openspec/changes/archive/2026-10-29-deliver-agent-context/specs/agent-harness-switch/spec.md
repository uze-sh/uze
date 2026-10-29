## ADDED Requirements

### Requirement: The incoming harness is briefed through the agent's context
The system SHALL give the briefing to the incoming harness as a section of
the agent's context, and SHALL start the incoming harness with no turn the
operator did not type, so the operator's next turn is answered by a model
that already holds the briefing. Where the incoming harness's delivery is
degraded, the system SHALL fall back to one short first turn pointing at the
briefing, given once.

#### Scenario: Switching shows no prompt
- **WHEN** an agent is switched to a harness whose context delivery is not degraded
- **THEN** the incoming pane shows no first turn and no briefing path, and the model's first request carries the briefing

#### Scenario: A harness whose delivery is degraded
- **WHEN** an agent is switched to a harness whose context delivery is degraded
- **THEN** that harness starts with one short first turn pointing at the briefing, on the first launch after the switch only

#### Scenario: A restore keeps the briefing without replaying a turn
- **WHEN** the terminal runtime restores a workspace holding an agent switched earlier that has not committed since
- **THEN** the agent resumes with no turn added, and its context still carries the briefing

## MODIFIED Requirements

### Requirement: Every switch writes a briefing
The system SHALL write, on every switch, a briefing describing the agent's
work for the incoming harness. It SHALL keep the briefing in the agent's
record, owner-readable only and outside every checkout, and SHALL NOT pass
its content as a process argument. The briefing SHALL fit its share of the
agent context's bound. It SHALL be written from the agent's Git facts
alone when the outgoing conversation cannot be read.

#### Scenario: The briefing never enters a checkout
- **WHEN** an agent is switched
- **THEN** a briefing exists outside every checkout, readable only by its owner, and no file was added to the agent's checkout

#### Scenario: An unreadable transcript still yields a briefing
- **WHEN** the outgoing harness's conversation cannot be read
- **THEN** the briefing holds the agent's branch, base, commits since the base and changed files, and the switch proceeds

#### Scenario: A long conversation stays within bounds
- **WHEN** the outgoing conversation is far larger than the briefing's bound
- **THEN** the briefing stays within the bound, keeping the goal, the latest summary and the most recent turns

## REMOVED Requirements

### Requirement: The incoming harness is pointed at the briefing once
**Reason**: The briefing now reaches the model through the agent's context
channel, which shows nothing in the pane and holds across resume. This
change removes the visible first turn naming a runtime path.
**Migration**: Covered by "The incoming harness is briefed through the
agent's context", which keeps the pointed-at first turn only as the
degraded fallback.
