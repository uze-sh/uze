## Purpose

Gives the agent in front a screen of its own, scoped to that agent and the
project it works in, that says what state the project is in, lets the
operator act where an action already exists, and lets them configure the
project's workspace.

## ADDED Requirements

### Requirement: The agent button opens the overview
A button at the right end of the tab strip, apart from the surfaces'
buttons, SHALL be drawn only in an agent's context, the agent or a shell
opened beside it, and SHALL show that agent's overview where the pane is,
or put it away when it is shown. A chord SHALL do the same. The
overview's header SHALL read `Agent`, the agent's label and its directory.
Selecting a space's row SHALL keep landing on the space's own shell,
creating one in the home directory for a space of agents alone.

#### Scenario: From an agent's tab
- **WHEN** an agent's tab is in front and the operator clicks the agent button
- **THEN** the agent's overview is drawn where the pane was, headed `Agent`, its label and its directory, and the strip keeps the agent's tabs

#### Scenario: A space row
- **WHEN** the operator clicks a space's row
- **THEN** the space's own shell is in front, and neither the button nor an overview is shown

#### Scenario: A space of agents alone
- **WHEN** the operator clicks the row of a space that holds no shell of its own
- **THEN** a shell of the space is opened in the home directory and put in front

### Requirement: The overview is put away only by leaving the agent
The overview SHALL be put away when the operator leaves the agent's
context: selecting a space's row, a shell of the space or another space,
or pressing the agent button or Esc with nothing open inside it; and when
the selection the server reports leaves that context, however that came
about. Selecting another agent, or a shell opened beside one, SHALL keep
the overview shown and turn it to that agent.

#### Scenario: Back to the space's shell
- **WHEN** the overview is shown and the operator selects the space's row
- **THEN** the overview is put away and the space's shell is drawn

#### Scenario: Another agent
- **WHEN** the overview is shown and the operator selects another agent
- **THEN** the overview stays shown and reads that agent

#### Scenario: A confirming broadcast
- **WHEN** the overview is shown and the server reports the same tab in front
- **THEN** the overview stays shown

### Requirement: The overview is a screen, never a tab
The overview SHALL be drawn by the client and SHALL own no process. It
SHALL not appear in a space's tab list, be closed like a tab, or be
restored by the terminal runtime. The space's own shell SHALL keep running
as before.

#### Scenario: A restored workspace
- **WHEN** the terminal runtime restores a workspace
- **THEN** each space comes back with its tabs as they were, and no tab stands for an overview

### Requirement: The overview has five sections, each with its state
The overview SHALL show five sections, Agent, Settings, Plugins, Memory and
Worktrees, in that order, one at a time, chosen from a section bar. Each tab
SHALL carry the worst state among its section's parts, and each section's
first line SHALL name the cause of that state. A drawer SHALL show the
selected row of a section in full.

#### Scenario: A section with a problem
- **WHEN** the lock drifts from the declaration and a declared plugin is not installed
- **THEN** the Plugins tab warns and the section's first line names the drift and the missing plugin

#### Scenario: Opening a row
- **WHEN** the operator selects a plugin in the Plugins section
- **THEN** the drawer shows that plugin's installation, commit, freshness and its place in the lock

### Requirement: The Worktrees section shows the project's worktrees as cards
The Worktrees section SHALL show one card per checkout UZE made for the
agents and subagents of the agent's project, in a grid as wide as the room and only whole
cards: its branch, state, owner and working tab, size on disk, commits
ahead, uncommitted work, path and age. Its tab SHALL warn on uncommitted
work no agent attends. It SHALL be measured off the frame only while in
front or refreshed.

#### Scenario: A card per worktree
- **WHEN** the agent's project has three slots of UZE's and a checkout a person made
- **THEN** three cards are drawn, each with its branch and size, and the cause line sums them

#### Scenario: Scrolling the cards
- **WHEN** the operator turns the wheel over a grid longer than the section
- **THEN** it scrolls a row of whole cards at a time and the selected card stays selected

#### Scenario: A measure that lands late
- **WHEN** an answer arrives for an earlier measure or for an overview put away
- **THEN** it is dropped and the section keeps drawing what it holds

#### Scenario: No worktrees
- **WHEN** every agent of the project works in its project's own checkout
- **THEN** the section says there are no worktrees

### Requirement: Only a free worktree can be removed from the overview
The Worktrees section SHALL offer removal only for a free worktree, a slot
no task holds, and only after a question naming it and its size. The
removal SHALL run off the frame and keep the worktree's branch. The
workspace SHALL judge the slot again right before removing it and SHALL
refuse, leaving it intact, one that a task holds, that holds uncommitted
work or that any process stands in.

#### Scenario: A free worktree removed after a question
- **WHEN** the operator asks to remove a free worktree and confirms
- **THEN** its directory is deleted, its branch stays, a toast says the size freed and the section is read again

#### Scenario: The question withdrawn
- **WHEN** the operator answers the question with Esc or Cancel
- **THEN** nothing is removed

#### Scenario: Anything else cannot be removed
- **WHEN** the selected worktree is working, ready, idle, kept, to join or unreadable
- **THEN** no removal is offered

#### Scenario: A worktree that changed since it was shown
- **WHEN** a slot is taken, written in or entered between the question and the removal
- **THEN** it is left intact and a toast says why it was kept

### Requirement: The Agent section reads the agent in front
The Agent section SHALL list the prompts the agent in front was given,
newest first, and no other agent's, and SHALL say what reaches that agent
here: the project's instructions and `.agents`, and what its harness
supports. Activating a prompt SHALL copy it whole. Clearing the prompts
SHALL forget that agent's alone. It replaces the agent drawer.

#### Scenario: Copying a prompt
- **WHEN** the operator moves onto a prompt in the Agent section and activates it
- **THEN** the whole prompt is on the clipboard, however it was folded on screen

#### Scenario: Another agent of the same project
- **WHEN** two agents of one project were each given prompts and the overview is opened on one of them
- **THEN** only that agent's prompts are listed, with no column saying where each was asked

#### Scenario: Clearing an agent's prompts
- **WHEN** the operator clears the prompts in one agent's overview and confirms
- **THEN** that agent's prompts are forgotten, and another agent of the project still lists its own

#### Scenario: Switching agents switches the overview
- **WHEN** the overview is open and the operator selects an agent of another project
- **THEN** the overview reads that agent: its prompts, and its project's plugins, memory, settings and worktrees

### Requirement: The overview reports readiness from the status read model
The overview SHALL report the project's state from the read model `uze
status` renders, composed once in the application: the declared plugins and
the lock's drift under Plugins; commands that wait for approval or have no
spelling for this machine under Settings.

#### Scenario: Status and the overview agree
- **WHEN** `uze status` reports a plugin missing from the Store
- **THEN** the overview's Plugins section reports the same thing

#### Scenario: Commands awaiting approval
- **WHEN** a project's commands wait for approval
- **THEN** the Settings section marks those lines and its Review opens the approval dialog for that project

### Requirement: The overview's foot lists what to check
The overview's foot SHALL count everything in the agent's project there is to check,
the instructions file and each harness's context and agent-context delivery
included. A click on the count SHALL list it in a dialog, worst first, as
the management's health status does; a click elsewhere or Esc SHALL close it.

#### Scenario: A harness that will not read the context
- **WHEN** `uze status` reports a harness that will not read the project's context
- **THEN** the foot counts it, and the list a click on the count opens names the harness and why

### Requirement: An agent outside a project says so
When the agent's directory no longer exists, or is not inside a project, the
overview SHALL say which and show the Agent section alone.

#### Scenario: A directory that is gone
- **WHEN** the agent's directory was deleted
- **THEN** the overview says the directory no longer exists and draws only the Agent section

#### Scenario: A directory that is not a project
- **WHEN** the agent runs in a plain directory with no project in or above it
- **THEN** the overview says it is not a project and draws only the Agent section

### Requirement: The Memory section lists the project's facts
The Memory section SHALL list the facts kept for the project, each with its
kind, origin (UZE, or the harness that keeps it) and last update, SHALL say
how many machine facts also reach agents in the project, and SHALL report
files it could not read. A search SHALL narrow the facts by name,
description, kind or origin. A fact's body SHALL open in the drawer.
Proposed machine facts SHALL NOT be listed.

#### Scenario: Facts from every source
- **WHEN** a project has UZE facts and Claude keeps memory for it
- **THEN** the section lists both, each naming where it is kept

#### Scenario: Searching the facts
- **WHEN** the operator types words into the Memory section's search
- **THEN** only the facts whose name, description, kind or origin contain them are listed, the count says how many of all, and enter reads the selected one

### Requirement: The Settings section shows every key
The Settings section SHALL list every key the `workspace:` section of
`agents.yaml` accepts, each with its declared value or, when undeclared,
the default the workspace applies marked as a default, and a one-line
description in the drawer. `actions` SHALL be shown by name and read-only.

#### Scenario: An undeclared workspace section
- **WHEN** the project declares no `workspace:` section
- **THEN** every key is shown with its default, marked as such

### Requirement: The Settings section saves each change to agents.yaml
The operator SHALL be able to change, add, remove and reorder workspace
values in place, and each committed change SHALL be written to
`agents.yaml` as it is made, keeping the operator's comments, with no
Save and no diff to confirm. Nothing SHALL be written while a value is
still being typed. Edits SHALL be validated by the parser that reads the
section; a refused edit SHALL NOT be written, and SHALL stay marked with
each problem shown at its key until it is changed or discarded.

#### Scenario: A change is saved as it is made
- **WHEN** the operator chooses `merge` for `delivery`
- **THEN** `agents.yaml` holds `delivery: merge` and a toast names the change

#### Scenario: An invalid value
- **WHEN** the operator sets `slots` to 0
- **THEN** nothing is written, and the key stays edited with the parser's reason under it

#### Scenario: Changing a command asks for approval again
- **WHEN** the operator changes a `setup` line
- **THEN** it is written, the toast says it waits for approval, and it does until approved

#### Scenario: A file changed meanwhile
- **WHEN** `agents.yaml` changes on disk before the operator's next change
- **THEN** that change is written over the file as it is, keeping what else changed

#### Scenario: A file UZE will not edit
- **WHEN** `agents.yaml` uses a YAML anchor
- **THEN** the Settings section is read-only and says why

### Requirement: The Settings section edits the agent's own checkout
The Settings section SHALL read and write the `agents.yaml` of the
checkout the agent works in: the project's own tree, or the agent's
isolated checkout. A change made in an isolated checkout SHALL stay there,
to be committed and delivered with that agent's work, and the section
SHALL say that new agents follow the project's file until it is
delivered. Commands SHALL be judged against the project's approval
wherever they are read.

#### Scenario: An agent in a worktree
- **WHEN** the operator changes `delivery` in the overview of an agent working in `.worktrees/a1`
- **THEN** `.worktrees/a1/agents.yaml` holds the change and the project's own `agents.yaml` is untouched

#### Scenario: An agent in the project's own tree
- **WHEN** the operator changes `slots` in the overview of an agent working in the project's tree
- **THEN** the project's `agents.yaml` holds it, and the next agent is placed by it

#### Scenario: A command added in a worktree
- **WHEN** the operator adds a `gate` line in a worktree's Settings
- **THEN** the line reads as waiting for approval

### Requirement: The overview says nothing of other agents
The overview SHALL NOT count, list or describe agents other than the one
it was opened on: what the space holds is the sidebar's to show.

#### Scenario: A space of several agents
- **WHEN** the overview is opened on one agent of a space holding three
- **THEN** its header names that agent alone, with no count of agents by state

### Requirement: Every action is reachable from the keyboard
The overview SHALL switch sections, move between rows, open the drawer,
edit and open the shell from the keyboard alone, through chords
declared in a scope of its own.

#### Scenario: Reviewing commands by keyboard
- **WHEN** the operator switches to the Settings section, moves onto a line awaiting approval and activates Review
- **THEN** the approval dialog opens for that project

### Requirement: The overview never waits on a read
Every read and validation behind the overview SHALL run off the frame and
answer through a channel, one at a time per agent, and an answer for an
agent no longer shown SHALL be dropped. Status SHALL be read again only
when the overview opens, when asked, after a written change, or when the project's
manifest, lock or instructions change.

#### Scenario: A slow read
- **WHEN** a read behind the overview takes seconds to answer
- **THEN** the overview draws what it has, marks the section as loading, and the client keeps answering input

#### Scenario: Walking the agents
- **WHEN** the operator walks past several agents quickly
- **THEN** a status read is made only for the agent the walk stops on
