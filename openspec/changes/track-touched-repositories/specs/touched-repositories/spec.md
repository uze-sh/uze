## Purpose

Shows, for each agent the workspace launched, every Git repository on the
machine that agent changed, how much of each change is the agent's, and
opens any of them on its changes, so work an agent did outside its own
checkout is never left unseen.

## ADDED Requirements

### Requirement: An agent's touched repositories are listed under it
The workspace sidebar SHALL list, beneath each agent it shows, the Git
repositories that agent touched: the agent's own checkout first, then every
other repository in the order the agent first touched it. Each repository
SHALL be named by its directory name, marked as the agent's own checkout or
as another repository, and SHALL carry the lines added and removed that are
attributed to the agent (see "Git alone measures what is attributed"). A
repository with nothing attributed left to show SHALL stay listed, without
counts, for as long as the agent is shown.

The level SHALL be drawn only when the agent touched at least one
repository other than its own checkout; an agent that stayed home is drawn
as it is today. The agent row SHALL say how many other repositories there
are and SHALL fold and unfold the level; the level SHALL start unfolded,
and the fold SHALL survive the client leaving and coming back.

#### Scenario: An agent that edited a sibling repository
- **WHEN** an agent working in `uze-terminal-orchestrator` edits a file in
  `atlas-design-system-web`
- **THEN** the agent SHALL show two rows beneath it, `uze-terminal-orchestrator`
  marked as its own checkout first and `atlas-design-system-web` marked as
  another repository second
- **AND THEN** the agent row SHALL say there is one other repository

#### Scenario: An agent that stayed in its checkout
- **WHEN** an agent touched no repository besides its own checkout
- **THEN** no level SHALL be drawn beneath it

#### Scenario: A change that was reverted
- **WHEN** the agent's edits in another repository are undone
- **THEN** that repository SHALL stay listed beneath the agent with no counts

#### Scenario: Folding
- **WHEN** the operator folds an agent's level and reattaches the client
- **THEN** the level SHALL still be folded

### Requirement: Opening a touched repository shows its changes
Clicking a repository listed beneath an agent SHALL open the code surface
on that repository's changes, rooted at that repository, in front of that
agent's pane. It SHALL NOT end the agent's selection, rename it, or start
dragging it. Opening a repository that is no longer a Git repository SHALL
say so and open nothing.

#### Scenario: Opening another repository
- **WHEN** the operator clicks `atlas-design-system-web` beneath an agent
- **THEN** the code surface SHALL show the changes of `atlas-design-system-web`
- **AND THEN** the agent SHALL still be the selected one

#### Scenario: A repository that was removed
- **WHEN** the operator clicks a listed repository whose directory was deleted
- **THEN** the workspace SHALL report that it is gone and open no surface

### Requirement: Repositories are witnessed from the launch, on every platform
UZE SHALL learn which repositories an agent touched only from what the
launch it made can observe, and SHALL NOT write into any harness's own
configuration, nor install anything that runs in a session it did not
launch, to do so. Two witnesses SHALL nominate repositories:
- **Git:** every Git command the agent's process tree runs, in any
  repository on the machine. A command that can change a repository
  (commit, add, checkout, reset, rebase, merge, stash, apply, and the like)
  SHALL nominate it; a command that only reads (status, diff, log) SHALL
  NOT, and the repository SHALL be listed only once Git measures a change
  attributed to the agent in it. Git commands the harness runs for its own
  housekeeping SHALL NOT nominate a repository on their own.
- **The edit observer:** every path a harness's file tools write, mapped to
  the repository containing it.

The agent's own checkout SHALL always be listed first. A path outside any
Git repository SHALL nominate nothing. Both witnesses SHALL work on Linux,
macOS and Windows.

A harness whose edit observer cannot be scoped to the one process UZE
launched SHALL be declared Unsupported for the observer, with the reason
shown where the workspace reports what each harness supports, and SHALL
still be witnessed through Git.

#### Scenario: An edit in a repository UZE has never seen
- **WHEN** an agent writes `/home/op/code/scratch-lib/src/a.rs` through its
  harness's file tool, in a repository UZE holds no record of
- **THEN** `scratch-lib` SHALL be listed beneath the agent

#### Scenario: A commit made through the shell
- **WHEN** an agent runs `git -C ../other commit -am fix`
- **THEN** `other` SHALL be listed beneath the agent

#### Scenario: A repository the agent only read
- **WHEN** an agent runs `git -C ../other log` and changes nothing there
- **THEN** `other` SHALL NOT be listed

#### Scenario: A harness whose observer cannot be scoped
- **WHEN** an agent runs on a harness declared Unsupported for the edit
  observer and commits in another repository
- **THEN** that repository SHALL still be listed beneath the agent

#### Scenario: A session UZE did not launch
- **WHEN** the operator runs the same harness outside the workspace
- **THEN** nothing UZE added SHALL run in that session

#### Scenario: A temporary file outside any repository
- **WHEN** the agent writes `/tmp/notes.md`
- **THEN** no repository SHALL be nominated for it

### Requirement: Git alone measures what is attributed
The lines shown for a repository SHALL be computed from Git, comparing what
the repository holds now — commits and working tree, untracked files
included — with what it held before the agent touched it. No witness SHALL
contribute a count. What is attributed SHALL be:
- for the agent's own checkout when that checkout is isolated, every change
  since the agent was placed in it;
- for a path the edit observer named, the change from that file's content
  before the agent first wrote it; a file the agent created counts whole;
- for a repository nominated by Git alone, every change since the agent
  first ran Git in it, and the row SHALL say that its counts start there.

A file that was already changed before the agent first wrote it SHALL count
only the agent's lines. UZE SHALL write nothing into a repository it does
not own in order to measure it.

#### Scenario: A file that was already dirty
- **WHEN** a file had 40 changed lines before the agent edited it and the
  agent changes 3 more lines in it
- **THEN** the repository SHALL show the 3 lines, not 43

#### Scenario: A commit the agent made elsewhere
- **WHEN** the agent edits and then commits a 12-line change in another
  repository
- **THEN** that repository SHALL still show the 12 lines after the commit

#### Scenario: A shared checkout
- **WHEN** two agents work in the operator's checkout and one of them edits
  `a.rs`
- **THEN** that agent's own checkout SHALL show only the change to `a.rs`

#### Scenario: Two agents, one repository
- **WHEN** two agents each edit a different file in the same other
  repository
- **THEN** each agent SHALL list that repository with only its own file's
  lines

#### Scenario: Nothing is written into the measured repository
- **WHEN** UZE measures a repository it does not own
- **THEN** that repository's object database, refs, index and working tree
  SHALL be unchanged by UZE

### Requirement: What an agent touched is kept as a record
The repositories an agent touched, the evidence for them and the content
they are measured against SHALL be kept as a record of the agent's project,
beside the agent's other records, and SHALL outlive the client and the
agent's process for as long as the agent is kept. What the witnesses report
while the workspace client is not running SHALL be folded into that record
when it next runs, and SHALL NOT be lost. Discarding the agent SHALL
discard its record.

#### Scenario: The client was closed
- **WHEN** the client is closed while an agent edits another repository and
  is opened again later
- **THEN** that repository SHALL be listed beneath the agent

#### Scenario: Discarding the agent
- **WHEN** the operator discards an agent
- **THEN** its record of touched repositories SHALL be gone

### Requirement: Witnessing never disturbs the agent
A witness SHALL never block, deny, slow beyond its bound, or change the
outcome of a tool call or a Git command: every failure to record SHALL let
the call proceed. The environment a launch adds for witnessing SHALL reach
only that agent's process tree, never another pane.

#### Scenario: The record cannot be written
- **WHEN** the trail directory is not writable
- **THEN** the agent's edit SHALL still be applied, and only the evidence
  SHALL be missing

#### Scenario: A plain pane started from inside an agent
- **WHEN** the terminal server is started from within an agent's pane and
  the operator opens a plain shell pane
- **THEN** Git commands in that pane SHALL NOT be recorded for the agent

### Requirement: Reading touched repositories never blocks the workspace
Measuring touched repositories and folding evidence SHALL run off the
thread that draws, and an answer that arrives after the agent or
repository it was asked about is no longer shown SHALL be dropped. A
launch SHALL NOT wait on anything a witness records.

#### Scenario: A slow repository
- **WHEN** one touched repository takes seconds to answer `git diff`
- **THEN** the sidebar SHALL keep drawing and SHALL show that repository's
  counts when they arrive
