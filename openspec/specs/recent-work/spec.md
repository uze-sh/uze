# recent-work Specification

## Purpose
The workspace sidebar's recent mode: one list, across every project UZE
has recorded, of the agents no live pane carries, saying which of them
wait on the operator and offering only the actions that move each one on.

## Requirements

### Requirement: Recent is a mode of the sidebar
The workspace sidebar SHALL have two modes, the tree and recent, and SHALL
show one of them at a time. The tree SHALL be the mode the client starts
in. Recent SHALL be entered from the sidebar header or by its key, and
left by its key, by escape, by a click outside it, or by resuming an
agent. Leaving recent and
entering it again SHALL return to the row that was selected, while that
row is still listed. No other surface SHALL list kept work.

#### Scenario: Opening recent replaces the tree
- **WHEN** the operator opens recent
- **THEN** the sidebar lists recent work and the tree is not drawn until recent is left

#### Scenario: The selection survives leaving
- **WHEN** the operator selects a row in recent, leaves recent and opens it again
- **THEN** the same row is selected

### Requirement: Recent lists agents, never checkouts
Recent SHALL list every agent no live pane carries whose work is kept, in
every project UZE has recorded, one row per agent. A checkout SHALL NOT be
a row: the checkout an agent holds is an attribute of that agent's row,
and a checkout no agent holds SHALL NOT be listed. No row and no detail
SHALL name a checkout's directory. An agent a live pane carries SHALL NOT
be listed, since the tree shows it.

#### Scenario: An agent and its checkout are one row
- **WHEN** an agent with no live pane holds a checkout with uncommitted changes
- **THEN** recent lists one row for the agent, and its checkout appears only as that row's disk figure

#### Scenario: A free slot is not listed
- **WHEN** a project has a clean slot no agent holds
- **THEN** recent lists no row for it

#### Scenario: A running agent is not listed
- **WHEN** an agent is carried by a live pane
- **THEN** it is not in recent

### Requirement: Work waiting on the operator comes first
Recent SHALL list in two blocks. The first SHALL hold the work waiting on
the operator: a paused rebase, failed checks, uncommitted changes, a
subagent ready to be joined, and work ready to deliver. The second SHALL
hold the rest. Each block SHALL be ordered by how recently its agent last
changed, most recent first. Integrated work SHALL NOT be listed, nor an
agent that ended with nothing but an old conversation, or none.

#### Scenario: Ready work waits on the operator
- **WHEN** one agent's work is ready to deliver and another's is with the reviewer
- **THEN** the ready one is in the first block and the other in the second

#### Scenario: The rest is listed second
- **WHEN** listed agents hold work being delivered, work with the reviewer, shelved work, work with no changes yet, or ended with only a recent conversation
- **THEN** each is in the second block

#### Scenario: Recency orders each block
- **WHEN** two shelved agents last changed an hour and a day ago
- **THEN** the one from an hour ago is listed above the other

### Requirement: An unselected row says only what identifies it
An unselected row SHALL show the agent's name and how long ago it last
changed, on one line, with no glyph, and every name in the same colour,
except the row of work whose project is gone, whose name SHALL be struck
through. A row waiting on the operator SHALL add one quiet line saying
why it waits. The state of the rest, the project, the branch and the
disk SHALL be said only by the selected row. Each block SHALL be headed by
its name and how many rows it holds.

#### Scenario: Two states, one look
- **WHEN** one listed agent is shelved and another is with the reviewer, and neither is selected
- **THEN** both rows are one line each, drawn alike but for their names and ages

#### Scenario: Waiting work says why, quietly
- **WHEN** a listed agent's checks failed and it is not selected
- **THEN** its row says that its checks failed on a second line, in a muted colour

### Requirement: The selected row says everything the operator acts on
The selected row SHALL expand to say, besides its name and age: why it
waits or what state it is in, in the colour of that reason; the project
it belongs to, and that resuming it opens a space when none is open on
that project; and its branch, how many commits it is ahead of its target
and, when it holds a checkout, the disk that checkout takes.

#### Scenario: A paused rebase, selected
- **WHEN** the operator selects an agent whose rebase paused on conflicts in three files
- **THEN** the row says the conflict and its file count, the project, the branch, how far ahead it is and the disk its checkout takes

#### Scenario: A project with no space open
- **WHEN** the operator selects work whose project has no open space
- **THEN** the row says that resuming opens a space

#### Scenario: No checkout, no disk figure
- **WHEN** the operator selects shelved work whose checkout was released
- **THEN** the row gives no disk figure

### Requirement: The header's name is the way in and out of recent
The sidebar header's name SHALL open and close recent, and SHALL answer
the pointer as the header's other controls do. The header SHALL carry no
count or mark beside it: what waits on the operator is read in recent.

#### Scenario: The name opens recent
- **WHEN** the operator clicks the sidebar header's name while the tree is shown
- **THEN** recent opens and the header reads `work › recent`

#### Scenario: Nothing beside the name
- **WHEN** agents wait on the operator and the tree is shown
- **THEN** the header shows its name alone

### Requirement: Each state offers its primary action and discard
The selected row SHALL offer, as enter, the one action that moves its
state on: joining, for a subagent ready to be joined; resuming, for every
other listed state. It SHALL also offer discard, and work ready to deliver
SHALL additionally offer delivery. Work whose project is gone SHALL offer
only forget. No other action SHALL be offered from recent, and the keys a
row offers SHALL be shown for that row.

#### Scenario: Ready work
- **WHEN** the operator selects work ready to deliver
- **THEN** resume, deliver and discard are offered, and nothing else

#### Scenario: A subagent ready to join
- **WHEN** the operator selects a subagent's checkout ready to be joined into its agent
- **THEN** enter joins it and discard is offered, and resume is not

#### Scenario: Work with the reviewer
- **WHEN** the operator selects work with the reviewer
- **THEN** resume and discard are offered, and nothing marks it done

### Requirement: Resuming leaves the operator on the resumed agent
Resuming from recent SHALL leave recent and show the tree with the resumed
agent selected, in a space rooted at its project, opened for it when none
was open. It SHALL launch the harness the agent last ran with, without
asking, while that harness is set up, and SHALL ask only when it is not.
The launch SHALL carry the agent's identity, so the agent continues its
conversation the same way it does when the workspace is reopened.

#### Scenario: The harness it ran with
- **WHEN** the operator resumes an agent that ran with a harness still set up, among others
- **THEN** that harness is launched and no harness is asked for

#### Scenario: Resuming into a closed project
- **WHEN** the operator resumes work whose project has no open space
- **THEN** recent is left, a space is opened on the project, and the resumed agent is selected in the tree

### Requirement: Joining keeps the operator in recent
Joining a subagent from recent SHALL replay its commits onto its agent's
branch, SHALL keep recent open, and SHALL say into which agent it joined.
The subagent's row SHALL leave the list once the join lands.

#### Scenario: A join lands
- **WHEN** the operator joins a subagent into its agent from recent
- **THEN** recent stays open, the subagent's row is gone, and the operator is told the agent it joined into

### Requirement: Discarding asks on the row, naming what goes
Discard SHALL ask on the selected row itself, saying that the branch, its
commits and the conversation go, with two answers drawn as buttons, and
SHALL take effect only when the operator goes ahead. The question SHALL
open with the keyboard on the way out, the arrows SHALL move between the
answers and enter SHALL take the one the keyboard is on. Declining, or any
other key, SHALL leave everything as it was.

#### Scenario: Discard is confirmed
- **WHEN** the operator asks to discard a row, moves to going ahead and takes it
- **THEN** its checkout, branch, kept work and conversation are removed and the row leaves the list

#### Scenario: A habitual enter takes nothing
- **WHEN** the operator asks to discard a row and presses enter at once
- **THEN** nothing is removed and the row is as it was

### Requirement: Work whose project is gone can only be forgotten
Work whose project directory no longer exists SHALL be listed with its
name struck through and, when selected, SHALL say that its project was not
found. Forgetting it SHALL remove UZE's record of that agent and its
conversations and SHALL NOT touch any repository.

#### Scenario: The repository was deleted
- **WHEN** a project holding kept work is deleted from disk and the operator opens recent
- **THEN** its work is listed struck through, and selecting it offers only forget

#### Scenario: Forgetting touches no repository
- **WHEN** the operator forgets work whose project is gone
- **THEN** its record leaves UZE and no Git command is run

### Requirement: Work with the reviewer leaves on its own when it lands
Work with the reviewer SHALL leave recent without any operator action once
its target carries its patch, whether it was merged, rebased or squashed,
so that marking work done by hand is never needed.

#### Scenario: A squash merge on the forge
- **WHEN** published work is squash-merged on the forge and the target is fetched
- **THEN** it leaves recent on the next evaluation, without the operator acting on it

### Requirement: Recent never waits on a repository
Drawing recent SHALL use only what UZE recorded and what background reads
already answered; facts that need Git, such as how far a branch is ahead
or the disk a checkout takes, SHALL arrive from background reads and
SHALL be drawn from their last answer meanwhile.

#### Scenario: Many projects
- **WHEN** the operator opens recent on a machine holding many projects
- **THEN** the list is drawn at once, and the per-row facts fill in as their reads answer

### Requirement: A name typed over an agent's tab is the agent's
Renaming an agent's tab SHALL record the name as that agent's label, so
recent lists the agent by it once its tab is gone. Naming the agent's
branch after its first commit SHALL NOT replace a label the operator
typed; a name the agent asks for SHALL, being the last name given.

#### Scenario: The tab is renamed and closed
- **WHEN** the operator renames an agent's tab to "vscode workspace" and closes it
- **THEN** recent lists the agent as "vscode workspace"

#### Scenario: The first commit names the branch
- **WHEN** an agent whose tab the operator named makes its first commit
- **THEN** its branch is named after the commit and its label stays the one the operator typed

### Requirement: Recent scrolls the way the code surface's navigator does
When recent's rows do not fit the column, it SHALL draw the product's
scrollbar on the sidebar's divider. The wheel, and dragging the handle
along the divider, SHALL scroll the list and leave the selection where it
is; moving the selection with the keys SHALL bring the window just far
enough to show the selected row. Dragging the handle across the divider
SHALL resize the column as the divider does.

#### Scenario: The handle is dragged to the foot
- **WHEN** recent holds more lines than the column shows and the operator drags the handle to the foot of the track
- **THEN** the last screenful is shown and the selection has not moved

#### Scenario: The keys after the wheel
- **WHEN** the operator scrolled the list away from the selection and then moves the selection with the keys
- **THEN** the window moves to show the selected row

#### Scenario: The divider still resizes
- **WHEN** the operator drags the handle sideways
- **THEN** the sidebar is resized and recent stays open
