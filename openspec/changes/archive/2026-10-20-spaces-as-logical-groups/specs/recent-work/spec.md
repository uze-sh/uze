## MODIFIED Requirements

### Requirement: The selected row says everything the operator acts on
The selected row SHALL expand to say, besides its name and age: why it
waits or what state it is in, in the colour of that reason; the project
it belongs to, and the space resuming it opens in; and its branch, how
many commits it is ahead of its target and, when it holds a checkout, the
disk that checkout takes.

#### Scenario: A paused rebase, selected
- **WHEN** the operator selects an agent whose rebase paused on conflicts in three files
- **THEN** the row says the conflict and its file count, the project, the branch, how far ahead it is and the disk its checkout takes

#### Scenario: A project with no space open
- **WHEN** the operator selects work whose project no open space works in
- **THEN** the row names the space in front as where resuming opens it

#### Scenario: No checkout, no disk figure
- **WHEN** the operator selects shelved work whose checkout was released
- **THEN** the row gives no disk figure

### Requirement: Resuming leaves the operator on the resumed agent
Resuming from recent SHALL leave recent and show the tree with the resumed
agent selected, in the space its project's work lands in, never in a space
opened for it. It SHALL launch the harness the agent last ran with, without
asking, while that harness is set up, and SHALL ask only when it is not.
The launch SHALL carry the agent's identity, so the agent continues its
conversation the same way it does when the workspace is reopened.

#### Scenario: The harness it ran with
- **WHEN** the operator resumes an agent that ran with a harness still set up, among others
- **THEN** that harness is launched and no harness is asked for

#### Scenario: Resuming into a closed project
- **WHEN** the operator resumes work whose project no open space works in
- **THEN** recent is left, no space is created, and the resumed agent is selected in the space that was in front
