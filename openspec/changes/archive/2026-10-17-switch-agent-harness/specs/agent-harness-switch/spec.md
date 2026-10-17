## Purpose

Lets the operator continue an agent's work on a different harness, keeping
the agent, its checkout and every harness's conversation, and telling the
incoming harness what happened so it does not start blind.

## ADDED Requirements

### Requirement: An agent can be continued on another harness
The system SHALL offer, in the menu of a running agent UZE launched, one
"Continue in" entry opening the detected harnesses other than the one the
agent runs now. Taking one SHALL relaunch the same agent on that harness in
the same checkout, on the same branch and under the same label, replacing
the tab. Harnesses that are not detected SHALL NOT be offered.

#### Scenario: Continuing a Claude agent in Codex
- **WHEN** the operator chooses to continue an agent running on one harness in another detected harness
- **THEN** a tab running the chosen harness replaces the agent's tab, standing in the same checkout on the same branch with the same label

#### Scenario: Only detected harnesses are offered
- **WHEN** the operator opens the "Continue in" entry of an agent's menu
- **THEN** it lists every detected harness except the agent's current one, and no harness that is not detected

#### Scenario: An exited agent offers no switch
- **WHEN** the agent in a tab has exited
- **THEN** the tab's menu offers no switch

### Requirement: An agent's menu opens from the keyboard
The system SHALL open the selected agent's menu, the one its row opens with
the pointer, from a key, and SHALL let every entry and every entry of a
group it opens be reached and taken from the keyboard alone.

#### Scenario: Continuing an agent without the pointer
- **WHEN** the operator presses the menu key on a selected agent, moves to "Continue in", opens it and takes a harness
- **THEN** the agent continues on that harness exactly as if the entries had been clicked

#### Scenario: Backing out of a group
- **WHEN** the operator opens the "Continue in" group and then goes back or presses Escape
- **THEN** the group closes and the menu stays open on the entry that opened it

### Requirement: The agent's harness is the one it runs now
The system SHALL record, on every launch of an agent, the harness that
launch runs, so every surface that names the agent's harness and every
later resume use the harness it was last launched on. Concurrent switches
of one agent SHALL leave exactly one harness recorded, and the losing
request SHALL be reported rather than applied.

#### Scenario: A switched agent resumes on its new harness
- **WHEN** an agent switched to another harness is later resumed from the recent list
- **THEN** it is relaunched on the harness it was switched to

#### Scenario: Resuming on a different harness records it
- **WHEN** a preserved agent is resumed and the operator picks a harness other than the one it last ran
- **THEN** the agent's recorded harness becomes the one picked

#### Scenario: Two clients switch the same agent
- **WHEN** two clients ask at once to switch the same agent from one harness
- **THEN** one switch is applied and the other is reported as already superseded

### Requirement: Switching back resumes the earlier conversation
The system SHALL keep every harness's conversation of an agent across a
switch, so an agent switched back to a harness it ran before continues that
harness's conversation rather than starting a new one.

#### Scenario: Claude to Codex and back
- **WHEN** an agent is switched from one harness to another and later back to the first
- **THEN** the first harness resumes the conversation it had before the first switch

### Requirement: Every switch writes a briefing
The system SHALL write, on every switch, a briefing describing the agent's
work for the incoming harness, owner-readable only and outside every
checkout, and SHALL NOT pass its content as a process argument. The
briefing SHALL be bounded in size, and SHALL be written from the agent's
Git facts alone when the outgoing conversation cannot be read.

#### Scenario: The briefing never enters a checkout
- **WHEN** an agent is switched
- **THEN** a briefing exists outside every checkout, readable only by its owner, and no file was added to the agent's checkout

#### Scenario: An unreadable transcript still yields a briefing
- **WHEN** the outgoing harness's conversation cannot be read
- **THEN** the briefing holds the agent's branch, base, commits since the base and changed files, and the switch proceeds

#### Scenario: A long conversation stays within bounds
- **WHEN** the outgoing conversation is far larger than the briefing's bound
- **THEN** the briefing stays within the bound, keeping the goal, the latest summary and the most recent turns

### Requirement: The briefing carries the conversation's substance
Where the outgoing harness's integration can read its conversation, the
briefing SHALL carry the first operator turn as the goal, the harness's own
latest compaction summary when one exists, the operator's turns after it,
the last assistant messages and the last command run. It SHALL omit tool
output, reasoning and subagent turns.

#### Scenario: A compacted conversation hands off its summary
- **WHEN** the outgoing conversation was compacted by its harness
- **THEN** the briefing carries the latest compaction summary and only the turns after it

### Requirement: The incoming harness is pointed at the briefing once
The system SHALL start the incoming harness with one short first turn that
points at the briefing, and SHALL give that turn on the first launch after
the switch only. Any later launch of the agent, including the terminal
runtime restoring the workspace, SHALL carry no briefing turn.

#### Scenario: A restore does not replay the briefing
- **WHEN** the terminal runtime restores a workspace holding an agent that was switched earlier
- **THEN** the agent resumes its conversation with no briefing turn added

#### Scenario: A harness that takes no first turn
- **WHEN** the incoming harness's integration declares no way to carry a first turn
- **THEN** the agent starts without one and the briefing is still written

### Requirement: Switching back hands off only what is new
When the incoming harness resumes a conversation it had before, the
briefing SHALL describe only what happened in the agent since that harness
last ran, so the resumed conversation is told what it missed rather than
the whole story again.

#### Scenario: The returning harness is told what it missed
- **WHEN** an agent returns to a harness it ran before, after work on another harness
- **THEN** the briefing covers the other harness's turns and commits since the returning harness last ran

### Requirement: Switching a busy agent asks first
The system SHALL ask the operator to confirm a switch of an agent that is
mid-turn, saying that its current work in progress will be interrupted,
and SHALL switch an idle agent without asking.

#### Scenario: A busy agent
- **WHEN** the operator switches an agent that is mid-turn
- **THEN** the switch waits for confirmation and does nothing if it is declined

#### Scenario: Switching to a harness without continuity
- **WHEN** the chosen harness declares no continuity
- **THEN** the operator is told that its conversation will not be resumed on a later launch
