## Purpose

Gives each space in the workspace a screen of its own, scoped to the
selected project, that says what state the project is in and lets the
operator act where an action already exists.

## ADDED Requirements

### Requirement: Selecting a space shows its overview
Selecting a space's row in the sidebar, by pointer or keyboard, SHALL show
that space's overview where the pane is drawn, with the sidebar live and the
tab strip showing the space's own tabs. It SHALL NOT focus or create a
shell. The overview SHALL belong to that space until put away, whatever the
server reports about tabs meanwhile.

#### Scenario: Clicking a space row
- **WHEN** the operator clicks a space's row
- **THEN** the space's overview is drawn where the pane was, and no tab of the space was created or focused

#### Scenario: The server confirms the space
- **WHEN** the overview opened and the server then confirms the space selection or a tab in it changes
- **THEN** the overview stays shown

#### Scenario: A double click renames
- **WHEN** the operator double-clicks a space's row
- **THEN** the overview is shown and the rename starts, and the overview is not put away by it

### Requirement: The overview is put away only by choosing something else
The overview SHALL be put away when the operator selects a tab or an agent,
selects another space (whose overview then shows), or presses Esc, which
returns to the space's selected tab. A chord SHALL bring the overview of the
current space back from any pane.

#### Scenario: Back to a tab
- **WHEN** the overview is shown and the operator selects one of the space's tabs
- **THEN** the overview is put away and that tab's pane is drawn

#### Scenario: Reopening from the keyboard
- **WHEN** a pane of a space is in front and the operator presses the overview chord
- **THEN** that space's overview is shown

### Requirement: The space's own shell is one action away
The overview SHALL offer an action that brings the space's own shell to the
front, creating one at the space's root when the space has none. That
action SHALL be the way back to the space's shells that selecting the row
used to be.

#### Scenario: A space whose tabs are all agents
- **WHEN** the operator takes the shell action on the overview of a space with only agent tabs
- **THEN** a shell opens at the space's root and is in front

### Requirement: The overview is a screen, never a tab
The overview SHALL be drawn by the client and SHALL own no process. It
SHALL not appear in the space's tab list, be closed like a tab, or be
restored by the terminal runtime. The space's own shell SHALL keep running
as before.

#### Scenario: A restored workspace
- **WHEN** the terminal runtime restores a workspace
- **THEN** each space comes back with its tabs as they were, and no tab stands for an overview

### Requirement: The overview reports readiness from the status read model
The overview SHALL report the project's readiness from the read model `uze
status` renders, composed once in the application: instructions file,
manifest against lock, each harness's context and agent-context delivery,
gates this machine cannot run, and commands awaiting approval. A command
awaiting approval SHALL be reviewable from here through the approval dialog.

#### Scenario: Status and the overview agree
- **WHEN** `uze status` reports an issue in a project
- **THEN** the overview of that space reports the same issue

#### Scenario: Commands awaiting approval
- **WHEN** a project's commands wait for approval
- **THEN** the overview names them and reviewing them opens the approval dialog for that project

### Requirement: A space that is not a project says so
When a space's root no longer exists, or is not inside a project, the
overview SHALL say which, offer the shell action, and show no section as
failed.

#### Scenario: A root that is gone
- **WHEN** a space's root was deleted
- **THEN** the overview says the directory no longer exists and draws no section

#### Scenario: A directory that is not a project
- **WHEN** a space's root is a plain directory with no project in or above it
- **THEN** the overview says it is not a project and offers the shell action

### Requirement: The overview lists the space's memory
The overview SHALL list the facts kept for the project and the machine,
each with its kind, scope and origin (UZE, or the harness that keeps it),
and SHALL open a fact's body on request. It SHALL list proposed machine
facts apart, each answerable through the proposal dialog opened on that
fact.

#### Scenario: Facts from every source
- **WHEN** a project has UZE facts and Claude keeps memory for it
- **THEN** the overview lists both, each naming where it is kept

#### Scenario: A proposal answered from the overview
- **WHEN** a proposed machine fact is listed and the operator accepts it there
- **THEN** that fact is accepted exactly as from the proposal toast, and the list moves it among the machine's facts

### Requirement: The overview lists the project's plugins and workspace policy
The overview SHALL list the plugins the project declares, each marked as
installed or not, contributing here or not, and with a newer commit
available or not. It SHALL show what the project's `workspace:` declaration
sets, including where its artifacts live, read-only.

#### Scenario: A declared plugin not installed
- **WHEN** the project declares a plugin that is not installed
- **THEN** the overview marks it as not installed

#### Scenario: An undeclared workspace section
- **WHEN** the project declares no `workspace:` section
- **THEN** the overview shows the defaults the workspace applies, marked as defaults

### Requirement: The overview counts the space's agents by state
The overview SHALL show how many of the space's agents are running, ready
to deliver, and preserved. It SHALL NOT list them one by one, which the
sidebar does.

#### Scenario: Counting
- **WHEN** a space has two running agents and one preserved
- **THEN** the overview shows two running, none ready, one preserved

### Requirement: The overview moves focus by section
The overview SHALL move keyboard focus between its sections and, within a
list, between rows, and SHALL act on the focused row. Every action SHALL be
reachable from the keyboard alone.

#### Scenario: Reaching a proposal by keyboard
- **WHEN** the operator moves focus to the memory section, onto a proposed fact, and activates it
- **THEN** the proposal dialog opens on that fact

### Requirement: Phase one writes no record and no authored or project file
The overview SHALL offer only actions that already exist elsewhere in the
workspace: reviewing commands awaiting approval, answering a proposed
machine fact, and bringing the space's shell to the front. It SHALL NOT
edit the manifest, install or remove a plugin, or forget a fact.

#### Scenario: Nothing of its own written
- **WHEN** the operator uses every control the overview offers
- **THEN** no record, authored file or project file changes but those the approval and proposal answers write

### Requirement: The overview never waits on a read
Every read behind the overview SHALL run off the frame and answer through a
channel, one at a time per space, and an answer for a space no longer shown
SHALL be dropped. Readiness SHALL be read again only when the overview
opens, when asked, or when the project's manifest, lock or instructions
change.

#### Scenario: A slow read
- **WHEN** a read behind the overview takes seconds to answer
- **THEN** the overview draws what it has, marks the section as loading, and the client keeps answering input

#### Scenario: Walking the spaces
- **WHEN** the operator walks past several spaces quickly
- **THEN** a readiness read is made only for the space the walk stops on
