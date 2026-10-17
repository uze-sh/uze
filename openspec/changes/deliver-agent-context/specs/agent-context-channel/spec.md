## Purpose

Gives every agent the workspace launches a context of its own through each
harness's most native channel. It reaches the model the way `AGENTS.md`
does: never shown as a turn the operator did not type, and still there
after a resume.

## ADDED Requirements

### Requirement: A launched agent's context is composed for each launch
Before each launch it owns, the system SHALL compose the agent's context
from the sections that apply to that launch. It SHALL deliver nothing when
no section applies. The composed document is generated, owner-readable
only, and outside every checkout.

#### Scenario: Nothing to give
- **WHEN** an agent with no section that applies is launched
- **THEN** no context is composed and the harness is launched exactly as before this change

#### Scenario: Composed again on every launch
- **WHEN** a section's content changes between two launches of an agent
- **THEN** the second launch is given the changed content

### Requirement: The context is bounded in bytes
The composed context SHALL stay within a fixed bound measured in bytes, the
unit the strictest channel limits by. A section that would overflow its
share SHALL be cut by its own rule, not truncated mid-document.

#### Scenario: Non-ASCII text
- **WHEN** a section is written in a language whose characters take several bytes
- **THEN** the composed context still fits the byte bound, and no channel spills it to a file the model is pointed at

### Requirement: The context reaches the model without being shown as a turn
Each harness integration SHALL declare the channel its agents receive their
context through. A channel SHALL put the context before the model without
it appearing in the pane, inside the turn the operator typed, or as a user
message in the harness's own conversation view. A harness with no such
channel SHALL declare its delivery degraded rather than claim it.

#### Scenario: Nothing is shown
- **WHEN** an agent is launched with a context on a harness whose channel is not degraded
- **THEN** the pane shows no turn the operator did not type, and the model's first request carries the context

#### Scenario: Placed like the project's own context
- **WHEN** a harness carries `AGENTS.md` to the model as part of a user-role message
- **THEN** a channel that places the agent's context the same way satisfies this requirement

### Requirement: The context's text is never a process argument or inherited environment
No process argument and no environment variable of a launched harness SHALL
contain the context's text. Only a path to the composed document MAY
appear, and only in the launch that document was composed for.

#### Scenario: Argv holds no text
- **WHEN** an agent is launched with a context
- **THEN** no argument of the harness process contains any line of the context

#### Scenario: A harness the agent starts gets nothing
- **WHEN** a launched agent starts another harness process of its own
- **THEN** that process is given no context

### Requirement: The context holds across resume and is current there
A channel SHALL give the context on a fresh launch and on a resumed
conversation. On a resume, the newest copy the model holds SHALL be the one
composed for that launch. A channel that also leaves earlier copies in the
conversation SHALL declare it.

#### Scenario: Resume carries it
- **WHEN** an agent's conversation is resumed in a new process
- **THEN** the model's next request carries the context composed for that launch

#### Scenario: A channel that accumulates
- **WHEN** a harness keeps each launch's copy in its conversation history
- **THEN** its integration declares that earlier copies remain, and the newest copy is the current one

### Requirement: Only launches UZE owns are given a context
The system SHALL give a context only to the launches continuity applies to.
The operator's own invocations, with arguments they composed or outside the
workspace, SHALL receive none. A channel that stays installed between
launches SHALL give nothing to a conversation UZE did not launch.

#### Scenario: A plain harness invocation
- **WHEN** the operator runs a harness outside the workspace
- **THEN** no agent context is composed, and the model receives nothing from any UZE channel

### Requirement: Degraded delivery is reported
Where a harness's channel is declared degraded, or is unavailable at launch
(for example, a harness-side switch that hooks depend on), `uze status` and
`uze doctor` SHALL say that agent context is not delivered to that harness
and why.

#### Scenario: A held-back channel
- **WHEN** a harness's channel is unavailable because the harness-side switch its hooks need is off
- **THEN** status reports that agent context does not reach that harness, naming the reason

### Requirement: Every channel is proven against the real harness
Conformance SHALL state the four outcomes in vendor-neutral terms, and SHALL
run them against each supported harness's real binary: the context reaches
the model, is not shown as a turn, holds on resume, and is current on
resume. A harness that cannot meet one SHALL declare it, with the reason,
rather than omit the check.

#### Scenario: A harness whose channel shows the context
- **WHEN** a harness's channel is measured to render the context in the pane
- **THEN** the run records it as a declared limitation of that harness, and its integration declares the delivery degraded
