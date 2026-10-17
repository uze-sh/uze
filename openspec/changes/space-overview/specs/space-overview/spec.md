## Purpose

Gives each space in the workspace a screen of its own, scoped to the
selected project, that says what state the project is in, lets the
operator act where an action already exists, and lets them configure the
project's workspace.

## ADDED Requirements

### Requirement: The space button opens the overview
A `space` button at the right end of the tab strip, apart from the
surfaces' buttons, SHALL be drawn in every context of the space, an agent's
or the space's own, and SHALL show the space's overview where the pane is,
over the tab in front, or put it away when it is shown. A chord SHALL do the
same. Selecting a space's row SHALL keep landing on the space's own shell,
creating one for a space of agents alone.

#### Scenario: From an agent's tab
- **WHEN** an agent's tab is in front and the operator clicks the space button
- **THEN** the space's overview is drawn where the pane was, and the strip keeps the agent's tabs

#### Scenario: A space row
- **WHEN** the operator clicks a space's row
- **THEN** the space's own shell is in front, and no overview is shown

### Requirement: The overview is put away only by choosing something else
The overview SHALL be put away when the operator selects a tab or an agent,
another space, or presses the space button or Esc with nothing open inside
it; and when the selection the server reports moves off the tab it stands
in front of, however that came about.

#### Scenario: Back to a tab
- **WHEN** the overview is shown and the operator selects one of the space's tabs
- **THEN** the overview is put away and that tab's pane is drawn

#### Scenario: A confirming broadcast
- **WHEN** the overview is shown and the server reports the same tab in front
- **THEN** the overview stays shown

### Requirement: The overview is a screen, never a tab
The overview SHALL be drawn by the client and SHALL own no process. It
SHALL not appear in the space's tab list, be closed like a tab, or be
restored by the terminal runtime. The space's own shell SHALL keep running
as before.

#### Scenario: A restored workspace
- **WHEN** the terminal runtime restores a workspace
- **THEN** each space comes back with its tabs as they were, and no tab stands for an overview

### Requirement: The overview has five sections, each with its state
The overview SHALL show five sections, Agent, Settings, Plugins, Context and
Memory, in that order, one at a time, chosen from a section bar. Each tab
SHALL carry the worst state among its section's parts, and each section's
first line SHALL name the cause of that state. A drawer SHALL show the
selected row of a section in full.

#### Scenario: A section with a problem
- **WHEN** the lock drifts from the declaration and a declared plugin is not installed
- **THEN** the Plugins tab warns and the section's first line names the drift and the missing plugin

#### Scenario: Opening a row
- **WHEN** the operator selects a plugin in the Plugins section
- **THEN** the drawer shows that plugin's installation, commit, freshness and its place in the lock

### Requirement: The Agent section reads the agent in front
The Agent section SHALL list the prompts the agent in front was given,
newest first, or its whole space's, as the operator chooses, and SHALL say
what reaches that agent here: the project's instructions and `.agents`, and
what its harness supports. Activating a prompt SHALL copy it whole. With no
agent in front it SHALL say so. It replaces the agent drawer.

#### Scenario: Copying a prompt
- **WHEN** the operator moves onto a prompt in the Agent section and activates it
- **THEN** the whole prompt is on the clipboard, however it was folded on screen

### Requirement: The overview reports readiness from the status read model
The overview SHALL report the project's state from the read model `uze
status` renders, composed once in the application: the instructions file
and each harness's context and agent-context delivery under Context; the
declared plugins and the lock's drift under Plugins; commands that wait for
approval or have no spelling for this machine under Settings.

#### Scenario: Status and the overview agree
- **WHEN** `uze status` reports a harness missing agent context or a plugin missing from the Store
- **THEN** the overview's Context or Plugins section reports the same thing

#### Scenario: Commands awaiting approval
- **WHEN** a project's commands wait for approval
- **THEN** the Settings section marks those lines and its Review opens the approval dialog for that project

### Requirement: A space that is not a project says so
When a space's root no longer exists, or is not inside a project, the
overview SHALL say which and show no section.

#### Scenario: A root that is gone
- **WHEN** a space's root was deleted
- **THEN** the overview says the directory no longer exists and draws no section bar

#### Scenario: A directory that is not a project
- **WHEN** a space's root is a plain directory with no project in or above it
- **THEN** the overview says it is not a project and draws no section bar

### Requirement: The Memory section lists the project's facts
The Memory section SHALL list the facts kept for the project, each with its
kind, origin (UZE, or the harness that keeps it) and last update, SHALL say
how many machine facts also reach agents in the project, and SHALL report
files it could not read. A fact's body SHALL open in the drawer. Proposed
machine facts SHALL NOT be listed.

#### Scenario: Facts from every source
- **WHEN** a project has UZE facts and Claude keeps memory for it
- **THEN** the section lists both, each naming where it is kept

### Requirement: The Settings section shows every key
The Settings section SHALL list every key the `workspace:` section of
`agents.yaml` accepts, each with its declared value or, when undeclared,
the default the workspace applies marked as a default, and a one-line
description in the drawer. `actions` SHALL be shown by name and read-only.

#### Scenario: An undeclared workspace section
- **WHEN** the project declares no `workspace:` section
- **THEN** every key is shown with its default, marked as such

### Requirement: The Settings section edits agents.yaml after a diff
The operator SHALL be able to change, add, remove and reorder workspace
values in place. Edits SHALL be validated by the parser that reads the
section, every problem shown at its key, and Save refused while any
remains. Save SHALL show the `agents.yaml` diff, keeping the operator's
comments, and write only on confirmation. It SHALL NOT write over a file
changed since the diff was made.

#### Scenario: An invalid value
- **WHEN** the operator sets `slots` to 0
- **THEN** the key shows the parser's reason and Save is refused

#### Scenario: Saving shows the diff
- **WHEN** the operator changes `delivery` and saves
- **THEN** the diff of `agents.yaml` is shown, and the file changes only when the operator confirms it

#### Scenario: Changing a command asks for approval again
- **WHEN** the operator saves a change to a `setup` line
- **THEN** the diff says the line will wait for approval, and after the save it does

#### Scenario: A file changed meanwhile
- **WHEN** `agents.yaml` changes on disk between the diff and the confirmation
- **THEN** nothing is written and the operator is asked to review again

#### Scenario: A file UZE will not edit
- **WHEN** `agents.yaml` uses a YAML anchor
- **THEN** the Settings section is read-only and says why

### Requirement: The overview counts the space's agents by state
The overview SHALL show how many of the space's agents are running, ready
to deliver, and preserved. It SHALL NOT list them one by one, which the
sidebar does.

#### Scenario: Counting
- **WHEN** a space has two running agents and one preserved
- **THEN** the overview shows two running, none ready, one preserved

### Requirement: Every action is reachable from the keyboard
The overview SHALL switch sections, move between rows, open the drawer,
edit, save and open the shell from the keyboard alone, through chords
declared in a scope of its own.

#### Scenario: Reviewing commands by keyboard
- **WHEN** the operator switches to the Settings section, moves onto a line awaiting approval and activates Review
- **THEN** the approval dialog opens for that project

### Requirement: The overview never waits on a read
Every read and validation behind the overview SHALL run off the frame and
answer through a channel, one at a time per space, and an answer for a
space no longer shown SHALL be dropped. Status SHALL be read again only
when the overview opens, when asked, after a save, or when the project's
manifest, lock or instructions change.

#### Scenario: A slow read
- **WHEN** a read behind the overview takes seconds to answer
- **THEN** the overview draws what it has, marks the section as loading, and the client keeps answering input

#### Scenario: Walking the spaces
- **WHEN** the operator walks past several spaces quickly
- **THEN** a status read is made only for the space the walk stops on
