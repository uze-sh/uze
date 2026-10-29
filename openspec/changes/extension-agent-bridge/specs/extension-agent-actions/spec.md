## Purpose

What an extension may do to an agent: prompt it, restart its conversation
or focus it, always through the host, shown first, refused rather than
queued, and limited.

## ADDED Requirements

### Requirement: A prompt is shown before it is typed, and typed only into a ready agent
An `agent.prompt` action SHALL show the exact text, the target agent and
the extension before anything is written, and SHALL be written only when the
agent is ready for input, its harness is the pane's foreground process and
no approval is pending; otherwise it SHALL be refused with the reason and
never queued.

#### Scenario: A busy agent is not typed into
- **WHEN** the operator presses `gsd-band`'s "Run phase 12" while the agent's turn is running
- **THEN** nothing is written, and the notice says the agent is busy

### Requirement: Unattended prompts are fixed, consented to per agent, and rate-limited
A prompt sent by an event or a timer SHALL need `agent:prompt:auto`,
accepted per project and agent for 24 hours and revoked by any change of
the extension's files; its text SHALL be the manifest's fixed text or a
value of its closed set; and the host SHALL drop writes past one per turn
and one per minute per extension and agent, three per minute and twenty
per hour per agent, attributed in a notice.

#### Scenario: A loop of prompts is cut down
- **WHEN** an extension's source answers on every turn and its action prompts each time
- **THEN** at most one prompt per turn reaches the agent, and the rest are dropped with a notice naming the extension

### Requirement: The operator can pause every extension's writes at once
The host's pause key SHALL hold every write an extension asks for until it
is pressed again, and SHALL reset every counter.

#### Scenario: Paused writes are dropped, not delivered later
- **WHEN** the operator pauses, an extension prompts twice, and the operator resumes
- **THEN** neither prompt was written, and nothing is written on resuming
