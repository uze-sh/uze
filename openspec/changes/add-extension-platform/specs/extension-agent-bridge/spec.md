## Purpose

Lets an extension see what the workspace's agents are doing, whichever
harness runs them, and act on an agent only under grants and safeguards that
keep the operator in charge of what the agent is told.

## ADDED Requirements

### Requirement: Agent events are normalized across harnesses
The host SHALL report `agent.session.started`, `agent.turn.started`,
`agent.turn.finished`, `agent.tool.started`, `agent.tool.finished` and
`agent.approval.asked`, each carrying the agent's id, harness and checkout,
and for tools the tool's name and a bounded summary, from a reporting hook
UZE delivers to each harness. Where a harness cannot report an event, the
host SHALL say so per harness rather than invent it.

#### Scenario: One extension sees two harnesses alike
- **WHEN** an agent on one harness and an agent on another each run a tool
- **THEN** an extension that declared `agent.tool.finished` receives one event per call, in the same shape, naming each agent's harness

#### Scenario: A missing event is reported, not faked
- **WHEN** a harness offers no hook for approval requests
- **THEN** `uze doctor` and the Extensions screen say `agent.approval.asked` is not reported for that harness, and no such event is emitted for its agents

#### Scenario: A turn is known without a hook
- **WHEN** a harness offers no turn hooks
- **THEN** `agent.turn.started` and `agent.turn.finished` are still emitted from what the agent's pane shows

### Requirement: The agents of the workspace are a source
A built-in source `agents` SHALL answer, for each agent whose checkout the
surface is open on, its id, harness, state, work label, checkout, branch,
and usage where its harness reports it, and SHALL need no grant.

#### Scenario: A status segment counts working agents
- **WHEN** three agents are open on the checkout and one is working
- **THEN** the `agents` source answers three entries and exactly one has state `working`

### Requirement: An agent action is spelled by the agent's integration
`agent.prompt` SHALL take a text or a skill invocation (a skill name and its
arguments); the host SHALL write it into the agent's pane as typed input,
with a skill invocation spelled in that agent's harness's own syntax.
`agent.interrupt` and `agent.restart` SHALL use the harness's own interrupt
and new conversation.

#### Scenario: A skill invocation follows the harness
- **WHEN** an action sends the skill `gsd:execute-phase` with argument `12` to an agent
- **THEN** the text written into the agent's pane is that skill's invocation as the agent's harness spells it, and it is the same action for every harness

#### Scenario: An unknown skill is refused before it is sent
- **WHEN** an action names a skill the agent's harness does not have delivered
- **THEN** nothing is written to the pane and the surface's notice names the skill and the harness

### Requirement: A prompt from a gesture is shown before it is sent
An `agent.prompt` invoked by the operator SHALL be sent only after the host
shows, under the extension's name, the exact text and the agent it goes to,
and the operator accepts. It SHALL reach only an agent whose checkout the
surface is open on, and SHALL need the `agent:prompt` grant.

#### Scenario: Declining sends nothing
- **WHEN** the operator presses a next-step button and declines the confirmation
- **THEN** nothing is written to any pane

### Requirement: Unattended prompts are bounded, queued and visible
An `agent.prompt` caused by an event or a pipe SHALL need `agent:prompt:auto`,
SHALL be rate-limited per agent, SHALL wait while the operator types in that
pane or a turn runs (in a bounded queue), and SHALL be marked in the pane as
sent by the extension. One host key SHALL pause every unattended prompt.

#### Scenario: A prompt waits for the turn to end
- **WHEN** an extension with `agent:prompt:auto` sends a prompt on an event while the agent's turn is running
- **THEN** the prompt is held and written only after `agent.turn.finished`, marked as sent by that extension

#### Scenario: A flood is cut by the limit
- **WHEN** an extension sends more unattended prompts to one agent than the rate allows
- **THEN** the excess is dropped with a notice naming the extension, and no more than the limit reaches the agent

#### Scenario: The operator pauses automation
- **WHEN** the operator presses the pause key for unattended prompts
- **THEN** no unattended prompt from any extension is written until they resume, and queued ones are shown as paused
