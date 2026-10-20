## ADDED Requirements

### Requirement: A space has no kind, and its agents choose their directory
The system SHALL give a space no kind to choose. A space is a named group
of agents, and an agent of it MAY run in any directory, a Git repository or
not. A directory SHALL NOT identify a space: the projects a space works in
are the directories its agents were started in.

#### Scenario: Creating a space asks nothing but its name
- **WHEN** the operator creates a space
- **THEN** they give it a name and nothing else, and the space exists

#### Scenario: A directory that is not a repository is a directory like any other
- **WHEN** the operator starts an agent in a directory that is not a Git working tree
- **THEN** the agent starts there, in the space the operator is in

## RENAMED Requirements

- FROM: `### Requirement: An agent starts in the space's own directory`
- TO: `### Requirement: An agent starts in the directory chosen for it`

## MODIFIED Requirements

### Requirement: An agent starts in the directory chosen for it
The system SHALL start every agent that is not isolated in the directory
chosen for it, on whatever branch that directory is on, without creating a
checkout or a branch, and SHALL record it with its identity, the harness it
runs and when it started. Such an agent SHALL have no branch of its own, no
delivery and no preserved work; its conversation SHALL be recorded and
resumed as any agent's is. Several agents MAY share one directory.

#### Scenario: Launching an agent creates nothing on disk
- **WHEN** an agent that is not isolated is launched in a chosen directory
- **THEN** no checkout and no branch are created, and the agent runs in that directory

#### Scenario: The operator's branch is never changed under them
- **WHEN** an agent runs in the directory chosen for it
- **THEN** UZE never switches or resets the branch that directory is on

### Requirement: A space can be created from the keyboard
The system SHALL bind the action that creates a space to a default chord,
so a space can be created without the pointer.

#### Scenario: The chord creates a space
- **WHEN** the operator presses the chord bound to creating a space
- **THEN** the prompt that names the space opens

### Requirement: The column groups an agent by where it works
The sidebar SHALL draw a space's agents in two groups, those working in the
directory chosen for them first and the isolated ones after, each in its
own order, with a blank row between them only when both groups have an
agent. An isolated agent's row SHALL carry the connector that says it
branches off the project; the others' SHALL not. The groups SHALL be drawn
in different colours. Isolating an agent SHALL move its row to the second.

#### Scenario: A space with both kinds of agent
- **WHEN** a space has agents in the directories chosen for them and isolated agents
- **THEN** the former are drawn first, then a blank row, then the isolated ones

#### Scenario: A space with one kind of agent
- **WHEN** every agent of a space is in the same group
- **THEN** no blank row is drawn

#### Scenario: The row moves when the agent is isolated
- **WHEN** an agent is isolated
- **THEN** its row leaves the first group and appears in the second

#### Scenario: Agents of several projects
- **WHEN** a space holds agents of two projects, some isolated and some not
- **THEN** the groups are the same two, whatever project each agent is in

### Requirement: A project may declare that its agents start isolated
The system SHALL let a project declare, as `workspace.worktree` in its
manifest, when an agent launched in it gets a worktree: `always`, at
launch, or `manual`, only when the operator moves it with **To worktree**.
Where it declares `always`, an agent SHALL be placed in a slot at launch,
and the isolation action SHALL have nothing to offer. Where it declares
`manual`, or nothing, an agent SHALL start in the directory chosen for it.
No other value SHALL be accepted, and a refusal SHALL name both.

#### Scenario: A project that isolates by default
- **WHEN** a project's manifest declares `workspace.worktree: always` and an agent is launched
- **THEN** the agent is placed in a checkout of its own at launch

#### Scenario: A project that declares nothing
- **WHEN** a project's manifest declares no `workspace.worktree` and an agent is launched
- **THEN** the agent starts in the directory chosen for it

#### Scenario: A manual project moves agents on request
- **WHEN** a project declares `workspace.worktree: manual` and the operator chooses "To worktree" on an agent
- **THEN** that agent alone is moved to a checkout of its own

#### Scenario: A value that is not a choice
- **WHEN** `workspace.worktree` holds `isolated` or `in-place`
- **THEN** the workspace refuses the declaration, naming `always` and `manual`

## REMOVED Requirements

### Requirement: A space has one kind, and any directory can be one
**Reason**: A directory no longer identifies a space; one root, one space no longer holds.
**Migration**: Replaced by "A space has no kind, and its agents choose their directory".

### Requirement: A resumed agent returns to its own project's space
**Reason**: A space no longer belongs to one project, so there is no space "of" a project to return to, and resuming never opens a space.
**Migration**: See "Resumed work lands where its project is" in `space-groups`: the space holding a tab in the project, else the space in front.
