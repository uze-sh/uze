# agent-isolation Specification

## Purpose
Lets an operator launch an agent without deciding anything first, and
isolate that one agent into a checkout of its own at the moment isolation
turns out to be what the work needs.

## Requirements

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

### Requirement: A root agent's work is read but never delivered
Where the work in an agent's checkout stands SHALL be read for an agent in
the space's root as for any other agent: the checkout's answer, which every
agent sharing that checkout reads alike. Only delivering it SHALL be
withheld, because the branch is the operator's and UZE did not cut it.

#### Scenario: Agents sharing the root read one answer
- **WHEN** two agents run in the space's root and the operator's branch holds work the target lacks
- **THEN** both are shown the same account of where that work stands, read from the checkout
- **AND** neither is offered a delivery

### Requirement: Isolation is an action on one agent
The system SHALL offer, for a single agent, a **To worktree** action giving
it a checkout of its own: a slot acquired as the project's policy says, the
agent's record gaining that isolation, and the agent relaunched there. Its
identity SHALL NOT change, and its conversation record SHALL stay its own.
The action SHALL be offered only inside a Git working tree with a commit to
branch from; where a slot cannot be acquired, the agent SHALL be left where
it is and the reason stated.

#### Scenario: An agent is isolated
- **WHEN** the operator isolates an agent running in the space's root
- **THEN** a checkout is created for it, its record carries that isolation, and it is relaunched there
- **AND THEN** the record names the checkout, the branch and what the branch was cut from
- **AND THEN** it is the same agent: the same identity, and the same conversation record

#### Scenario: Isolation is not offered where it cannot be honoured
- **WHEN** the space's root is not a Git working tree, or has no commit to branch from
- **THEN** the action is not offered

#### Scenario: A slot cannot be acquired
- **WHEN** isolating an agent and the checkout cap is reached, or Git refuses
- **THEN** the agent keeps running where it was, and the operator is told why

Isolating SHALL take a copy of the root's uncommitted changes into the
checkout. At the moment an agent is moved, what the tree holds is
usually what that agent was doing, and a checkout without it is one
where the file it was mid-edit on has gone back to its last commit.

Cutting from the last commit instead SHALL be offered as a second
answer, named **To worktree, no changes**, and only where the tree is known to hold uncommitted work. The
knowledge is the evaluation's, never a Git read taken while the surface
draws, so it MAY be up to a refresh old — which is why it gates this
answer and not the carrying one: a stale *clean* reading costs the
operator nothing, where the same staleness on the carrying answer would
take away the very thing they had just edited. Neither answer takes
anything from the operator's own tree.

#### Scenario: The work follows the agent
- **WHEN** an agent is isolated while the space's root has uncommitted changes
- **THEN** a copy of them is in the checkout — edits to tracked files and new files the repository does not ignore alike
- **AND THEN** the root's own working tree is left exactly as it was, whichever answer is given
- **AND THEN** nothing is discarded

#### Scenario: A clean tree asks nothing
- **WHEN** an agent is isolated while the space's root has no uncommitted changes
- **THEN** the action carries the one answer, and neither tree gains or loses anything

#### Scenario: The work is not this agent's to take
- **WHEN** the root is known to hold uncommitted changes and the operator cuts from the last commit
- **THEN** the checkout holds the commit alone, and the changes stay in the root they were made in

#### Scenario: The actions say where the agent goes and what stays
- **WHEN** the menu of an agent in the space's root is opened while the root holds uncommitted changes
- **THEN** it offers "To worktree" and "To worktree, no changes", and no label speaks of isolating or cleaning

### Requirement: The harness decides whether an isolated agent's conversation follows it
Whether the harness's conversation follows an isolated agent SHALL be the
harness's answer, not UZE's: UZE SHALL resume it where the harness can and
start a new conversation where it cannot, as for a harness that binds a
conversation to the directory it ran in, never refusing the isolation over
it.

#### Scenario: A harness that binds its conversation to a directory
- **WHEN** an agent whose harness stores its conversation under the directory it ran in is isolated
- **THEN** the agent is relaunched in the checkout with a conversation of its own, and the isolation is not refused

#### Scenario: A harness that resumes anywhere
- **WHEN** an agent whose harness can resume a conversation from any directory is isolated
- **THEN** the agent is relaunched in the checkout continuing the conversation it was in

### Requirement: An isolated agent is the subject of delivery
The system SHALL treat an isolated agent exactly as a task is treated
today: its work is delivered by the project's completion behaviour when the
project declares one, it can be named, and its checkout is a slot that is
reused when the agent ends. An agent that is not isolated SHALL be none of
those things.

#### Scenario: Readiness is read for every agent
- **WHEN** an agent that is not isolated has work in its checkout
- **THEN** where that work stands is read as for an isolated agent, and only the delivery, which needs a branch UZE cut, is withheld

#### Scenario: Delivery is offered for an isolated agent
- **WHEN** an isolated agent has commits its target does not have and the project declares a completion behavior
- **THEN** the same delivery the project's policy describes is offered for it

#### Scenario: An agent in the root is never delivered
- **WHEN** an agent that is not isolated has been working
- **THEN** no delivery is offered for it: its commits are already on the operator's branch

### Requirement: An agent ends when no live pane carries it
The system SHALL record an agent as ended when no live pane carries its
identity, and SHALL keep the record. An isolated agent's slot SHALL be
released by the same sweep that releases a slot today; an agent that is
not isolated SHALL leave nothing to release.

#### Scenario: The last pane of an agent closes
- **WHEN** the last pane carrying an agent's identity is closed
- **THEN** the agent is recorded as ended and its row leaves the column

#### Scenario: An isolated agent that ended holding work
- **WHEN** an isolated agent ends with commits its target does not have
- **THEN** its slot is not handed to the next agent, and the work stays reachable

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

### Requirement: A space can be created from the keyboard
The system SHALL bind the action that creates a space to a default chord,
so a space can be created without the pointer.

#### Scenario: The chord creates a space
- **WHEN** the operator presses the chord bound to creating a space
- **THEN** the prompt that names the space opens

### Requirement: Preserved work is found without the space it was left in
The preserved-work list SHALL answer from UZE's records of every project it
has recorded an agent for, not from the projects the session opened. Every
recorded agent no live pane carries SHALL be listed, whether or not a space
is open on its project and whatever became of its work, until it is
resumed, joined, discarded or forgotten, or its record is collected. Each
entry SHALL name its project when selected.

#### Scenario: The space the work was left in was closed
- **WHEN** an agent holding work has no live pane and no space is open on its project
- **THEN** its work is listed and, when selected, names the project it belongs to

#### Scenario: A project the session never opened
- **WHEN** the operator opens the list in a session that has opened no space on a project holding preserved work
- **THEN** that project's preserved work is listed, without the operator having opened it first

#### Scenario: Work that is delivered or discarded
- **WHEN** an agent's work has been integrated, however it was merged, or the agent was discarded
- **THEN** the delivered one is still listed as delivered, resumed on its branch cut again from the target in the conversation it left, and the discarded one is not listed

#### Scenario: An agent that ended with only a conversation
- **WHEN** an agent ended with nothing to deliver
- **THEN** it is listed, and resuming it places it on its branch cut again from the target, in that conversation

#### Scenario: An agent that worked in the project's root
- **WHEN** an agent that was not isolated has no live pane
- **THEN** it is listed with no branch, and resuming it places it in the project's root, in the conversation it left

#### Scenario: An agent forgotten
- **WHEN** an agent that held no work was forgotten
- **THEN** its record and its conversation are gone, and it is not listed

#### Scenario: A record collected for its age
- **WHEN** an agent ended more than thirty days ago holding no checkout, branch or shelf
- **THEN** its record and its conversation are collected, and it is not listed

#### Scenario: Two projects, one branch name
- **WHEN** two projects each hold preserved work on a branch of the same name
- **THEN** both are listed, and each names its own project when selected

#### Scenario: The project was deleted
- **WHEN** the directory of a project holding preserved work no longer exists
- **THEN** its work is listed and says that its project was not found

#### Scenario: The list stays quick as projects accumulate
- **WHEN** the operator opens the list on a machine holding many projects
- **THEN** it is drawn from what UZE recorded, without asking Git about any
  repository, and the operator waits on nothing

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
