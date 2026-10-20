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
- **THEN** the space works in no project, and resuming work of `back` does not land in it

### Requirement: A new agent is one question with its harness in a field
Starting a new agent SHALL ask for the directory it runs in, chosen from
the directories that exist and narrowed as a segment is typed, with its
harness in a field beside them, prefilled with the harness the last agent
was started with and moved with `←` and `→`; `Tab` SHALL complete the
highlighted directory. Enter SHALL start
the agent in the directory chosen with the harness in the field. With no
harness set up, Enter SHALL lead to setting one up. Dismissing SHALL start
nothing.

#### Scenario: The common case is one key
- **WHEN** the operator starts a new agent, the last one ran Claude Code, and confirms the directory offered
- **THEN** the agent starts there with Claude Code, and nothing else is asked

#### Scenario: Another harness
- **WHEN** the operator presses `→` before confirming
- **THEN** the field moves to the next harness set up, and the agent starts with it

#### Scenario: Another project in the same space
- **WHEN** the operator starts a new agent and picks a directory of another project
- **THEN** the agent is placed by that project's worktree policy and opens in the space the operator is in

#### Scenario: Dismissed
- **WHEN** the operator dismisses the popup
- **THEN** no agent is placed and no tab opens

### Requirement: A new agent is chosen beside the `new` it was asked from
The popup SHALL be anchored beside the `new` of the space it was asked
from, floating over the pane, and nothing in the sidebar SHALL move while
it is open. Its listing SHALL keep the height it opened with, up to nine
rows, scrolling inside it. Its foot SHALL name its keys, each one a target
that does what the key does.

#### Scenario: The popup opens beside `new`
- **WHEN** the operator presses `new` on a space
- **THEN** the popup opens beside that `new`, over the pane, and every row of the sidebar stays where it was

#### Scenario: The foot names the keys
- **WHEN** the popup is open with a harness set up
- **THEN** its foot reads `enter new · ←→ harness`, with `esc close` against its end, and a click on `new` starts the agent

### Requirement: The directory listing says what it knows
A row of the listing SHALL say, against its end, when its directory is a
project the space's agents work in, with the branch its checkout is on
when known, and otherwise when it is a Git repository.

#### Scenario: A project of the space
- **WHEN** the space's agents work in `uze`, checked out on `main`
- **THEN** the `uze` row reads `space · main`

#### Scenario: A repository elsewhere
- **WHEN** `scribble` is a Git repository no agent of the space works in
- **THEN** its row reads `git`

### Requirement: The new agent's popup offers where the space is working
The popup SHALL be prefilled with the project of the agent whose
context is in front; with a shell of the space in front, with the project
of the space's most recently started agent; and otherwise with the
operator's home directory. Confirming the prefill SHALL cost one key.

#### Scenario: The common case is one confirmation
- **WHEN** an agent of `back` is in front and the operator starts a new agent and confirms the prefill
- **THEN** the new agent starts in `back`

#### Scenario: A shell of the space in front
- **WHEN** a shell of the space is in front, wherever it stands, and the space's last agent was started in `landing`
- **THEN** the popup is prefilled with `landing`

#### Scenario: A space with nothing but its first shell
- **WHEN** the operator starts the first agent of a space whose only tab is a shell in their home directory
- **THEN** the popup is prefilled with the home directory

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

### Requirement: Each agent's row names its project
The sidebar SHALL caption a minimized space in front with the number of
agents it holds, and with nothing while it holds no agent. Every agent's
row SHALL name the project it works in, with its harness against the row's
end, since a space is bound to no directory.

#### Scenario: A space of one project
- **WHEN** a minimized space in front holds two agents of one project
- **THEN** its caption reads `2 agents`, and opened, each agent's row names the project

#### Scenario: A space of three projects
- **WHEN** a space has agents in three projects
- **THEN** opened, each agent's row names its own project

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