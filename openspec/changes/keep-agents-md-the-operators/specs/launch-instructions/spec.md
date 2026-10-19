## Purpose

Gives an agent the workspace launched the instructions that only mean
something inside the workspace, through its launch context instead of the
project's files.

## ADDED Requirements

### Requirement: The workspace's instructions are a section of the launch context
Every agent the workspace launches SHALL receive the effective worktree
policy as an instructions section of its launch context, on every launch and
every resume, through its harness's context channel. A launch UZE does not
own SHALL receive none.

#### Scenario: A launched agent receives the policy
- **WHEN** the workspace launches an agent in a project with the default policy
- **THEN** its context carries the instructions section, and the project's checkout carries none of that text

#### Scenario: A harness started by hand
- **WHEN** a person runs the harness in the same project without the workspace
- **THEN** the agent receives no instructions section

### Requirement: Instructions are framed apart from notes
The instructions section SHALL come before every notes section and SHALL be
introduced as instructions from UZE for this launch. The statement that what
follows are notes, not instructions, SHALL introduce only the sections after
it.

#### Scenario: The order of the composed context
- **WHEN** a launch has instructions, a briefing and memory to give
- **THEN** the document opens with the instructions section, followed by the notes framing and then the briefing and memory

### Requirement: The instructions state the agent's own situation
The section SHALL state the agent's actual situation, as the launcher knows
it at compose time: whether it is isolated, its checkout, the branch its
checkout has then, the target and the completion behavior. It SHALL NOT ask
the agent to infer its situation from its working directory.

#### Scenario: An isolated agent
- **WHEN** an agent is launched in slot `ab12cd` of a project targeting `main`
- **THEN** its section names `.worktrees/ab12cd`, branch `agent/ab12cd` and target `main`

#### Scenario: A resume after naming
- **WHEN** the agent named its work `fix/x` and its session is resumed
- **THEN** the resumed section names branch `fix/x`

#### Scenario: An agent in the operator's checkout
- **WHEN** an agent is launched in the operator's own checkout
- **THEN** its section says it is on the operator's branch, to commit there, and never to switch, reset, stash or clean it, and says nothing about delivery

### Requirement: The instructions fit beside the notes
The instructions section SHALL have a share of the context's byte bound of
its own, and the briefing and memory shares SHALL be reduced so the three
add up to the bound. A full briefing and a full memory index SHALL never
push the instructions out.

#### Scenario: Everything at its share
- **WHEN** a launch has a full briefing and more memory than fits
- **THEN** the instructions section is whole, and the document is within the bound

### Requirement: A degraded channel carries the instructions on a new conversation
On a harness whose context channel is degraded or unavailable, a launch
that starts a new conversation SHALL give one visible first turn pointing at
the agent's context, which carries the instructions. A resume SHALL give no
first turn. Both SHALL be reported as degraded, naming the channel.

#### Scenario: A degraded new conversation
- **WHEN** the workspace launches a new agent on a harness whose channel is unavailable
- **THEN** the agent's first turn points at its context, which carries the instructions, and status reports the channel as degraded

#### Scenario: A degraded resume
- **WHEN** that agent's session is resumed with the channel still unavailable
- **THEN** no first turn is given: the conversation keeps what its first turn delivered, and status reports the channel as degraded, so the instructions are not refreshed
