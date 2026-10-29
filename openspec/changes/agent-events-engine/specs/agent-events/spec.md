## Purpose

How an agent UZE launched reports what it does on every supported harness,
and which agent events the workspace and extensions get from those reports.

## ADDED Requirements

### Requirement: Every supported harness reports through UZE's own hooks
`uze setup` SHALL deliver UZE's own hooks manifest to every supported
harness through the portable hooks delivery, receipt-owned like a plugin's
hooks, reporting a session's start, each tool's start and end and the
turn's end of an agent the workspace launched.

#### Scenario: The same reports from four harnesses
- **WHEN** an agent of each supported harness, launched by the workspace, edits a file and ends its turn
- **THEN** each agent's journal holds a session start, the tool's start and end with the file, and the turn's end

#### Scenario: Removing the setup takes the hooks with it
- **WHEN** the operator removes UZE's setup from a harness
- **THEN** UZE's hook entries are gone from its configuration and the operator's own hooks are untouched

### Requirement: An agent nobody launched reports nothing
UZE's hooks SHALL report only for a process carrying the launch of an
agent the workspace started, and SHALL end at once, starting nothing,
anywhere else, so a harness used outside the workspace pays nothing.

#### Scenario: A harness opened by hand
- **WHEN** the operator runs a supported harness from a shell outside the workspace and it uses a tool
- **THEN** no journal is written and no UZE process is started for it

### Requirement: A report carries portable fields only
A report SHALL carry the event, its time, the tool by its portable name
and the native one, and the checkout-relative path the tool touched, read
from the portable hook fields every harness provides; never a tool's
input or output.

#### Scenario: A shell command's text stays out
- **WHEN** an agent runs a shell command containing a token
- **THEN** its report names the `shell` tool and holds no part of the command

### Requirement: Extensions get only agent events every harness produces
The extension contract SHALL offer an agent event only when every
supported harness produces it, or the workspace derives it the same way
for all of them: `session.started`, `turn.started`, `turn.finished`,
`waiting` and `waiting.ended`.

#### Scenario: A manifest naming a removed event
- **WHEN** an extension declares `refresh-on "agent.approval.asked"`
- **THEN** its manifest does not compile, and the error names `agent.waiting` as what replaces it

### Requirement: Waiting is derived the same way for every harness
The workspace SHALL emit `waiting` for an agent whose reported tool has
started and not finished while its pane has been quiet for a short while,
and `waiting.ended` when that tool finishes or the agent's turn ends.

#### Scenario: A permission prompt
- **WHEN** an agent asks the operator before running a tool and the operator takes a minute to answer
- **THEN** `waiting` is emitted once while it asks, and `waiting.ended` once the tool has run

#### Scenario: A long tool is not waiting
- **WHEN** a tool runs for two minutes while the agent's pane keeps printing its output
- **THEN** no `waiting` is emitted

### Requirement: A turn's start is derived where no harness reports it
The workspace SHALL emit `turn.started` for an agent when its first
report after a turn's end arrives, or when its pane starts working,
whichever comes first, and only once per turn.

#### Scenario: A turn with no tool
- **WHEN** an agent answers a question without using a tool
- **THEN** `turn.started` comes from its pane starting to work, and `turn.finished` from its report

### Requirement: The workspace's agent state follows the reports
The sidebar's agent state SHALL follow an agent's reports, and its pane
only where no report arrived; an agent that is waiting SHALL be shown as
waiting on the operator wherever its state is shown.

#### Scenario: Which agent needs the operator
- **WHEN** one of three working agents is waiting on a permission prompt
- **THEN** the sidebar shows that agent as waiting and the other two as working
