## Purpose

The workspace's tab strip shows where an isolated agent's work stands as feedback read from its checkout and the remote, and lets a project declare what the agent is asked at each state.

## ADDED Requirements

### Requirement: The work-state chip is feedback read from the checkout and the remote
The tab strip SHALL show the selected isolated agent's work state as a chip read from its checkout and the remote, the same whoever brought the work there. The chip SHALL NOT be clickable unless the work is ready, or the project declares an instruction for the state it shows.

#### Scenario: A state with no instruction is only reported
- **WHEN** the selected agent's work is published and the project declares no `on_publish` instruction
- **THEN** the chip shows the published state and a click on it does nothing

#### Scenario: A push by anyone reads the same
- **WHEN** the agent itself pushed its branch
- **THEN** the chip reads the branch as published exactly as if anyone else had pushed it

#### Scenario: Uncommitted changes are a chip only when the project asks something there
- **WHEN** the agent's checkout holds uncommitted changes
- **THEN** the change count reports them, and a chip reading uncommitted is drawn beside it only when the project declares `on_change`

### Requirement: The states the chip reads
The chip SHALL read one of: `ready` (commits the target lacks), `published` (on the remote with nothing left to push and no request), `in_review` (a request is open for it and nothing is left to push), `delivered` (the target holds all of it), `conflicted` (a rebase is paused) or `failed` (the gate refused it). When more than one holds, the one furthest along SHALL be read.

#### Scenario: A new commit after publishing
- **WHEN** the agent commits on a branch that read as published
- **THEN** the chip reads ready again, with the commits not yet on the remote counted

#### Scenario: A new commit while in review
- **WHEN** the agent commits on a branch that read as in review
- **THEN** the chip reads ready again until those commits are on the remote

#### Scenario: A request found on the remote
- **WHEN** the branch is on the remote and the forge publishes a request for its tip
- **THEN** the chip reads in_review with the request's number

#### Scenario: A pushed branch without a request
- **WHEN** the branch is on the remote with nothing left to push and no request exists for it
- **THEN** the chip reads published, not in_review

### Requirement: An agent in the operator's checkout has no chip and no instruction
No work-state chip SHALL be drawn, and no declared instruction SHALL be sent, for an agent working in the space's own root: its branch is the operator's.

#### Scenario: A root agent with ready work
- **WHEN** an agent in the space's root has uncommitted changes and the project declares `workspace.actions.on_change`
- **THEN** no chip is drawn for it and nothing is offered to send

### Requirement: A project declares one instruction per state
A project MAY declare `workspace.actions` in `agents.yaml`: one plain-text instruction per state, keyed `on_change` (uncommitted), `on_publish` (published), `on_conflict` (conflicted) and `on_fail` (failed). Ready, in review and delivered take none, since the next step from each is the delivery, a reviewer or nobody. An unknown key or an empty instruction SHALL be reported and left out, and SHALL NOT stop the rest of the declaration from being read.

#### Scenario: A declared instruction makes its state clickable
- **WHEN** `agents.yaml` declares `workspace.actions.on_publish` and the selected agent's work is published
- **THEN** the chip is drawn as an action

#### Scenario: An unknown state is reported, not fatal
- **WHEN** `workspace.actions` names a key outside that set
- **THEN** the project's agents are still read and placed, the other declared states still work, and the unknown one is reported once

#### Scenario: Work in flight takes no instruction
- **WHEN** a delivery is running for the selected agent
- **THEN** the chip shows it in flight and is not clickable, whatever `workspace.actions` declares

### Requirement: A click submits the declared instruction to the chip's agent
A click on a chip that has an instruction SHALL submit that instruction, verbatim and ended with Enter, to the pane of the agent the chip describes, as one submission. Text that reaches the pane SHALL carry no control character and SHALL be bounded.

#### Scenario: The instruction reaches the agent once
- **WHEN** the operator clicks a chip whose state declares an instruction and the agent is idle with nothing typed
- **THEN** the instruction is submitted once into that agent's pane and a toast says it was sent

#### Scenario: A second click right after the first
- **WHEN** the operator clicks the same chip again before the agent has visibly started on the first submission
- **THEN** nothing more is submitted and the click is answered as the agent being busy

#### Scenario: A skill is asked for in the instruction's own words
- **WHEN** the declared instruction asks the agent to use a named skill
- **THEN** the text is submitted unchanged, with no translation for the harness in the pane

### Requirement: An operation of UZE's sends the instruction of the state it leaves
When a delivery or an automatic rebase that UZE runs leaves the work in a state with an instruction, the system SHALL submit that instruction to the agent once, under the same rules as a click. A state reached any other way SHALL send nothing on its own. `conflicted` and `failed` SHALL have an instruction of UZE's own when the project declares none, since in both the agent's checkout waits on the agent; any other undeclared state SHALL send nothing.

#### Scenario: A push hands the work over
- **WHEN** a push delivery rebases the branch, the gate passes, the branch is pushed, and the project declares `workspace.actions.on_publish`
- **THEN** that instruction is submitted to the agent once

#### Scenario: A conflict found while following the target
- **WHEN** an automatic rebase stops on conflicts and the project declares `workspace.actions.on_conflict`
- **THEN** that instruction is submitted to the agent once, and the chip shows the conflict

#### Scenario: A failed gate with nothing declared still reaches the agent
- **WHEN** a delivery's gate fails and the project declares no `on_fail` instruction
- **THEN** UZE's own instruction for a failed gate is submitted to the agent once

#### Scenario: Nothing declared, nothing typed into the pane
- **WHEN** a push delivery publishes the branch and the project declares no `on_publish` instruction
- **THEN** nothing is submitted to the agent, and the chip and a toast report the push

#### Scenario: States reached outside UZE's operations send nothing
- **WHEN** the agent's own edits leave the checkout uncommitted, or its own push makes it published
- **THEN** nothing is submitted on its own; a declared instruction for that state is sent only by a click

#### Scenario: An automatic send meets a busy agent
- **WHEN** UZE's operation leaves the work in a declared state while the agent is working or the operator has text typed
- **THEN** nothing is submitted, a toast says the instruction was held back, and a click on the chip sends it once the pane allows

### Requirement: An instruction never interrupts the agent or the operator
The system SHALL refuse to submit an instruction while the agent is working, while the operator has text typed in that pane, or while what is typed there cannot be known. The refusal SHALL say which of the three it was, and the instruction SHALL NOT be queued.

#### Scenario: The agent is mid-turn
- **WHEN** the operator clicks an actionable chip while its agent is working
- **THEN** nothing is submitted and a toast says the agent is busy

#### Scenario: The operator has a prompt half typed
- **WHEN** the operator clicks an actionable chip while text they typed sits unsent in that agent's input
- **THEN** nothing is submitted, the typed text is left as it is, and a toast says why

### Requirement: Action and report chips differ at rest
A chip with an instruction and a chip without one SHALL be told apart without the pointer over either, and only the first SHALL change under the pointer. No chip in the strip's status zones SHALL be set apart by a ground of its own.

#### Scenario: Two states side by side are told apart at rest
- **WHEN** one tab's chip has an instruction and another's does not
- **THEN** the two are drawn differently before either is hovered

#### Scenario: A report chip does not react
- **WHEN** the pointer rests on a chip with no instruction
- **THEN** the chip is drawn exactly as at rest

### Requirement: Hovering an action says what it will send
Under the pointer, an actionable chip SHALL name the instruction a click sends, by its key (`send on_publish`), in the strip's message zone, never a cut or moving copy of the instruction's text. A report chip SHALL say there what its state means, in the status legend's words. The hint SHALL NOT move any control, and SHALL be dropped when the zone has no room for it.

#### Scenario: The hint names the key, whatever the instruction's length
- **WHEN** the operator hovers a published chip whose `on_publish` instruction is a long sentence
- **THEN** the message zone reads `send on_publish`, and no control in the strip moves

#### Scenario: A report says what its mark means
- **WHEN** the operator hovers the chip of work in review (`#259` and its mark)
- **THEN** the message zone reads `in review · request open`, and the chip itself is drawn as at rest

#### Scenario: A narrow strip drops the hint
- **WHEN** the strip has no room left for the hint
- **THEN** no hint is drawn and every control stays where it was

### Requirement: A click on ready work delivers it
A click on the chip of ready work SHALL start its delivery, as the key bound to delivering the selected task does; a click on a chip in any other state SHALL NOT. Ready takes no instruction, so the click has one meaning. Delivery SHALL also stay reachable by its key and from recent.

#### Scenario: The key still delivers
- **WHEN** the operator presses the key bound to delivering the selected task and the work is ready
- **THEN** the delivery runs as the project's completion behavior describes

#### Scenario: Hovering ready work names the key that delivers it
- **WHEN** the pointer rests on a ready chip
- **THEN** the chip is drawn as an action, and the message zone names the key bound to delivering

#### Scenario: A click on ready work delivers it
- **WHEN** the operator clicks the chip of ready work
- **THEN** the delivery starts as the key would start it, and nothing is typed into the agent's pane

#### Scenario: A click elsewhere never delivers
- **WHEN** the operator clicks the chip of published, conflicted or failed work
- **THEN** no rebase, gate, fast-forward or push is started
