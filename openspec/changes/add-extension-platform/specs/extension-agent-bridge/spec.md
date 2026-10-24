## Purpose

Lets an extension see what the workspace's agents are doing, whichever
harness runs them, and act on an agent only under grants and safeguards that
keep the operator in charge of what the agent is told.

## ADDED Requirements

### Requirement: Agent events are normalized across harnesses
Under `agent:observe`, the host SHALL report `agent.session.started`,
`agent.turn.started`, `agent.turn.finished`, `agent.tool.started`,
`agent.tool.finished`, `agent.approval.asked` and `agent.approval.resolved`,
each naming the agent, harness and checkout, from a reporter UZE adds to each
launch. Where a harness or its sandbox cannot report one, the host SHALL say
so per harness and not invent it.

#### Scenario: One extension sees two harnesses alike
- **WHEN** an agent on one harness and an agent on another each run a tool
- **THEN** an extension that declared `agent.tool.finished` receives one event per call, in the same shape, naming each agent's harness

#### Scenario: A missing event is reported, not faked
- **WHEN** a harness offers no hook for approval requests
- **THEN** `uze doctor` and the Extensions screen say `agent.approval.asked` is not reported for that harness, and no such event is emitted for its agents

#### Scenario: A turn is known without a hook
- **WHEN** a harness offers no turn hooks
- **THEN** `agent.turn.started` and `agent.turn.finished` are still emitted from what the agent's pane shows

#### Scenario: The reporter exists only inside UZE
- **WHEN** the operator runs the same harness outside UZE
- **THEN** no reporter runs and nothing of UZE's is installed in that harness's configuration

### Requirement: A tool event names the tool, never its content
A tool event SHALL carry the call's id, start and end time, outcome, the
tool's name and, when it has one, a checkout-relative path, after the
credential scrub; it SHALL NOT carry the tool's input or output.

#### Scenario: A command line with a token stays out
- **WHEN** an agent runs a shell command whose arguments contain a token
- **THEN** the `agent.tool.finished` event names the shell tool and the outcome, and carries no part of the command line

### Requirement: Reported events are claims, never permissions
Any process in an agent's pane can report, so an `agent.*` event SHALL only
be drawn and refresh sources. No write into a pane, the clipboard or a path
SHALL be released by an event; the host SHALL decide those from what it
observes of the pane itself.

#### Scenario: A forged turn end opens nothing
- **WHEN** a process in an agent's pane reports `agent.turn.finished` while the harness is still running a turn
- **THEN** the event may be drawn, and any write the extension then asks for is refused because the host observes the pane as busy

### Requirement: The agents of the workspace are a source
A built-in source `agents` SHALL answer, under `agent:observe`, each agent of
the workspace's open spaces: id, harness, state (`working`, `idle`,
`waiting`, `unknown`), work label, checkout, branch, turn start, and usage in
a typed shape where its harness reports it.

#### Scenario: A status row counts working agents
- **WHEN** three agents are open and one is working
- **THEN** the `agents` source answers three entries and exactly one has state `working`

### Requirement: Every agent action names its target
An agent action SHALL name a `target`: `self` (the agent of the band, or of
the selected item) or a reference to an agent id the `agents` source
answered. An action with no resolvable target SHALL run nothing, and no
agent action SHALL reach more than one agent.

#### Scenario: A band acts on its own agent
- **WHEN** two agents each show the same extension's band and the operator presses the band's button on the second
- **THEN** only the second agent is written to

### Requirement: Writing to an agent is filtered, shown as sent, and refused when the pane is not ready
The host SHALL strip control, format, zero-width and tag characters from
text for an agent, refuse a leading `!`, `/`, `#` or `@` unless it is the
integration's own skill spelling, send byte for byte what it showed, and
refuse with the reason, never queue, unless the pane is ready for input,
its harness in front and no approval pending.

#### Scenario: A busy pane refuses
- **WHEN** an extension sends a prompt to an agent whose turn is running
- **THEN** nothing is written, and the extension's surface shows that the agent is busy

#### Scenario: Invisible characters never reach the agent
- **WHEN** a prompt's text contains Unicode tag characters between visible words
- **THEN** the confirmation shows the text without them and the agent receives exactly what the confirmation showed

#### Scenario: A shell escape is refused
- **WHEN** a prompt's text begins with `!`
- **THEN** nothing is written and the notice says the text would run a command in the harness

#### Scenario: A half-typed draft is left alone
- **WHEN** the operator has typed into the agent's prompt and not submitted
- **THEN** the extension's prompt is refused and the draft is unchanged

### Requirement: An agent action is spelled by the agent's integration
`agent.prompt` SHALL take a text or a skill invocation (a skill and its
arguments) that the agent's integration spells in its harness's syntax, for
a skill UZE delivered or one the integration lists as native.
`agent.restart` SHALL use the harness's new conversation and SHALL always
ask the operator first.

#### Scenario: A skill invocation follows the harness
- **WHEN** an action sends the skill `gsd:execute-phase` with argument `12` to an agent
- **THEN** the text written into the agent's pane is that skill's invocation as the agent's harness spells it, and it is the same action for every harness

#### Scenario: An unknown skill is refused before it is sent
- **WHEN** an action names a skill the agent's harness has neither delivered nor natively
- **THEN** nothing is written to the pane and the surface's notice names the skill and the harness

### Requirement: A prompt from a gesture is shown before it is sent
An `agent.prompt` invoked by the operator SHALL need `agent:prompt` and SHALL
be sent only after the host shows, under the extension's name, the full
text and the target agent, and the operator accepts.

#### Scenario: Declining sends nothing
- **WHEN** the operator presses a next-step button and declines the confirmation
- **THEN** nothing is written to any pane

### Requirement: Unattended prompts carry only declared text
An `agent.prompt` caused by an event, a timer or a resident component
without a gesture SHALL need `agent:prompt:auto` for that project and agent,
SHALL send only the manifest's fixed text or a declared skill with arguments
from a closed set, SHALL be rate-limited per agent and attributed, and SHALL
stop while the operator's pause is on.

#### Scenario: Answered text is never sent unattended
- **WHEN** an event-mapped source answers a prompt text and the manifest's unattended prompt is the fixed text `continue`
- **THEN** only `continue` can be sent, and only when the pane is ready

#### Scenario: A flood is cut by the limit
- **WHEN** an extension sends more unattended prompts to one agent than the rate allows
- **THEN** the excess is dropped with a notice naming the extension, and no more than the limit reaches the agent

#### Scenario: The operator pauses automation
- **WHEN** the operator presses the pause key for unattended prompts
- **THEN** no unattended prompt from any extension is written until they resume

#### Scenario: An update revokes unattended prompting
- **WHEN** an extension holding `agent:prompt:auto` is updated to a new digest with the same grants
- **THEN** `agent:prompt:auto` is held until the operator accepts it again, while its other grants follow the update rules
