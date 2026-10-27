## Purpose

How an agent UZE launched tells the workspace what it is doing, what the
workspace makes of those reports, and what an extension may read of them.

## ADDED Requirements

### Requirement: An agent reports about itself through a verified, bounded journal
`uze agent report <event>` SHALL be accepted only from a process carrying
the launch key of an agent UZE launched, SHALL append one line to that
agent's journal, bounded in lines and bytes, and SHALL exit 0 whatever
happens, so a turn is never blocked by it.

#### Scenario: A report without the key is refused
- **WHEN** a shell with no launch key runs `uze agent report turn.finished`
- **THEN** nothing is appended and the command says it is not an agent UZE launched

#### Scenario: A flood of reports is bounded
- **WHEN** an agent reports ten thousand tool calls
- **THEN** its journal holds at most its bound, the newest kept

### Requirement: Reports become agent events an extension may subscribe to
The workspace SHALL turn each report into an `agent.*` event carrying the
agent's id, harness and checkout, a tool's name and checkout-relative path
scrubbed of credentials, never its input or output. An extension holding
`agent:observe` SHALL receive them as it receives checkout events.

#### Scenario: A turn's end refreshes a source
- **WHEN** a source declares `refresh-on "agent.turn.finished"` and the agent's turn ends
- **THEN** the source is asked again with the event in `input.event`

### Requirement: The agents source is the workspace's own account of its agents
A source declaring `agents` SHALL be answered by the workspace with every
agent of its open spaces: id, harness, state, work label, checkout, branch
and when its turn began, only for an extension holding `agent:observe`.

#### Scenario: A header row of every agent
- **WHEN** `agents-hud`'s status row reads the `agents` source with two agents open, one working
- **THEN** it draws both, the working one with its elapsed turn
