## Purpose

Lets an operator group agents by what they are working on rather than by
where they run: one space holding agents of several projects, each started
in the directory chosen for it.

## ADDED Requirements

### Requirement: A space is a named group of agents
A space SHALL be a label and the agents and shells it holds, bound to no
directory: two spaces MAY hold agents of one project, and one space MAY
hold agents of several. The projects a space works in SHALL be those its
agent tabs, tabs launched with a program rather than a shell, stand in; a
shell's directory SHALL NOT count, wherever it moved.

#### Scenario: One space, three projects
- **WHEN** the operator starts agents in `front`, `back` and `landing` from one space
- **THEN** the three agents are listed under that space, each running in its own directory

#### Scenario: Two spaces on one project
- **WHEN** two spaces each start an agent in the same project
- **THEN** both spaces keep their agent, and neither is merged into the other

#### Scenario: A shell does not make a project the space's
- **WHEN** a shell of a space with no agent `cd`s into `back`
- **THEN** the space works in no project: it has no caption, and resuming work of `back` does not land in it

### Requirement: A new agent chooses its directory before its harness
Starting a new agent SHALL first ask for the directory it runs in, then for
its harness. The directory SHALL be chosen from the directories that exist,
narrowed as a segment is typed, as the new-space prompt did. Cancelling
either step SHALL start nothing.

#### Scenario: Another project in the same space
- **WHEN** the operator starts a new agent and picks a directory of another project
- **THEN** the agent is placed by that project's worktree policy and opens in the space the operator is in

#### Scenario: Cancelling the directory step
- **WHEN** the operator dismisses the directory step
- **THEN** no agent is placed and no tab opens

### Requirement: A new agent is chosen beside the `new` it was asked from
Both steps of a new agent SHALL be drawn in one popover anchored beside the
`new` of the space it was asked from, floating over the pane: first the
directories, then, in the same popover and at the same place, the
harnesses. Nothing in the sidebar SHALL move while it is open. Dismissing
either step SHALL close it.

#### Scenario: The steps open beside `new`
- **WHEN** the operator presses `new` on a space
- **THEN** the directory step opens beside that `new`, over the pane, and every row of the sidebar stays where it was

#### Scenario: The harness step takes the same place
- **WHEN** the operator confirms a directory
- **THEN** the harnesses are drawn in the same popover, at the same place

#### Scenario: Dismissed
- **WHEN** the operator presses Esc in either step
- **THEN** the popover closes and no agent is placed

### Requirement: The new-agent popover keeps its size
The popover SHALL be as tall as the directory step's listing up to ten
rows and SHALL keep its size through both steps, however many directories
or harnesses there are. What does not fit SHALL scroll inside it.

#### Scenario: A directory with many entries
- **WHEN** the directory step lists forty directories
- **THEN** the popover is ten rows tall and scrolls inside them

#### Scenario: From one step to the other
- **WHEN** the operator confirms a directory and three harnesses are offered
- **THEN** the harness step keeps the size the directory step had

### Requirement: The directory step offers where the space is working
The directory step SHALL be prefilled with the project of the agent whose
context is in front; with a shell of the space in front, with the project
of the space's most recently started agent; and otherwise with the
operator's home directory. Confirming the prefill SHALL cost one key.

#### Scenario: The common case is one confirmation
- **WHEN** an agent of `back` is in front and the operator starts a new agent and confirms the prefill
- **THEN** the new agent starts in `back`

#### Scenario: A shell of the space in front
- **WHEN** a shell of the space is in front, wherever it stands, and the space's last agent was started in `landing`
- **THEN** the directory step is prefilled with `landing`

#### Scenario: A space with nothing but its first shell
- **WHEN** the operator starts the first agent of a space whose only tab is a shell in their home directory
- **THEN** the directory step is prefilled with the home directory

### Requirement: An agent opens in the space it was started from
A new agent SHALL open in the space whose `new` was pressed, whatever
project it runs in and whichever space is selected when its placement
lands. An agent isolated, moved to another harness, or resumed in place of
one of its tabs SHALL open in the space that tab was in. No placement
SHALL open a space of its own.

#### Scenario: Isolating keeps the space
- **WHEN** the operator isolates an agent of a space holding three projects
- **THEN** the relaunched agent opens in that same space

#### Scenario: A project with no space of its own
- **WHEN** the operator starts an agent in a project no space works in
- **THEN** it opens in the space the operator is in, and no space is created

#### Scenario: The operator moved on while it was placed
- **WHEN** the operator starts an agent from one space and selects another before its checkout is ready
- **THEN** the agent opens in the space it was started from

### Requirement: Resumed work lands where its project is
Work resumed from the preserved list SHALL open in a space working in its
project, preferring the space the operator is looking at when it is one,
and otherwise in the space the operator is looking at. Resuming SHALL NOT
create a space.

#### Scenario: A space already holds the project
- **WHEN** the operator, looking at one space, resumes work of a project another space holds an agent of
- **THEN** the agent opens in the space holding the project, and the operator is left looking at it

#### Scenario: No space holds the project
- **WHEN** the operator resumes work of a project no open space works in
- **THEN** the agent opens in the space the operator is looking at

### Requirement: A space holding several projects names each agent's project
The sidebar SHALL caption a space with its project's directory while its
agents stand in one project, with the number of projects while they stand
in several, and with nothing while it holds no agent. In a space whose
agents stand in several projects, each agent's row SHALL name its project.

#### Scenario: A space of one project
- **WHEN** every agent of a space stands in one project
- **THEN** the space's caption names that project and no agent row names it again

#### Scenario: A space of three projects
- **WHEN** a space's agents stand in three projects
- **THEN** its caption says three projects, and each agent's row names its own

#### Scenario: A space with no agent
- **WHEN** a space holds only shells
- **THEN** it has no caption

### Requirement: A project's actions belong to the agent in front
The strip's work status, the spec, architect and code surfaces and the
overview SHALL be offered only while an agent's context is in front: the
agent or a shell opened alongside it. They SHALL read the directory the
agent was placed in, never where a shell of its context moved to. A shell
of the space SHALL offer none of them, wherever it stands.

#### Scenario: A shell of the space in front
- **WHEN** the operator selects the space's row and its own shell is in front, standing inside a repository
- **THEN** the strip shows no work status and no spec, arch, code or overview action

#### Scenario: A shell opened alongside an agent
- **WHEN** a shell opened alongside an agent of `back` is in front, after a `cd` into `front`
- **THEN** the strip's status and surfaces read `back`, the agent's directory

#### Scenario: Closing a surface by leaving the agent
- **WHEN** the code surface is open on an agent and the operator selects the space's row
- **THEN** the surface closes and the space's shell is shown