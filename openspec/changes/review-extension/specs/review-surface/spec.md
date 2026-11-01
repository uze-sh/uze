## Purpose

The review surface is where a person reads what an agent's branch adds
against its target, says what should change, line by line, and hands that
to the agent in one send, then reads the answers where they asked.

## ADDED Requirements

### Requirement: The review has a door that is always on the tab strip
While the review is switched on in the workspace, the tab strip SHALL
always draw its door in an agent's context: the branch's additions and
deletions against its target when there are any, and `review` when there
are none. Pressing it SHALL open the review, or close it when it is the
surface in front.

#### Scenario: A branch with work on it
- **WHEN** the agent's branch adds 81 lines and removes 16 against its target
- **THEN** the door reads `+81 -16`

#### Scenario: Nothing differs from the target
- **WHEN** the branch and its working tree hold nothing the target does not
- **THEN** the door reads `review` in a held-back tone
- **AND THEN** opening it says that nothing on the branch differs from the target

#### Scenario: The door closes what it opened
- **WHEN** the review is in front and the operator presses its door
- **THEN** the review is put away and the agent's pane is shown

#### Scenario: The review switched off
- **WHEN** the operator switches the review off for the workspace
- **THEN** no door is drawn for it, and its key does nothing

### Requirement: The door counts the branch alone
The door SHALL say how many lines the branch adds and removes against its
target, and nothing more: what waits in the review (drafts, answers, an
agent at work on a send) SHALL be said inside the review, never on the
door.

#### Scenario: Drafts waiting are not on the door
- **WHEN** the operator has written two comments and sent none on a branch adding 81 lines and removing 16
- **THEN** the door reads `+81 -16`

### Requirement: The review diffs the branch against its target
The review SHALL show, per changed file, the difference between the merge
base of the checkout's target and the working tree, committed and
uncommitted work together. An `uncommitted` filter SHALL narrow it to what
is not committed yet. With no known target, or on the target branch
itself, it SHALL show the uncommitted changes and say why.

#### Scenario: Committed and uncommitted work in one diff
- **WHEN** the branch has three commits on `main` and one file edited but not committed
- **THEN** the review lists every file the commits and the edit touched

#### Scenario: Only what is not committed
- **WHEN** the operator chooses the `uncommitted` filter
- **THEN** only the edited, uncommitted file is listed

#### Scenario: No target known
- **WHEN** the checkout's target has not been evaluated
- **THEN** the review shows the uncommitted changes and says the target is unknown

#### Scenario: An agent on the target branch
- **WHEN** the agent works directly on `main`, which is also its target
- **THEN** the review shows the uncommitted changes and says the branch is its own target

### Requirement: The target is the fetched one when there is one
The merge base SHALL be taken against the target as last fetched, when it
has been, and the local target branch otherwise, so work already landed
upstream is not shown as the branch's own.

#### Scenario: A stale local main
- **WHEN** the local `main` is behind the fetched `main`, which already holds two of the branch's commits
- **THEN** the review does not list what those two commits changed

### Requirement: Lines are marked by pointer, keyboard or text selection
The operator SHALL be able to mark a run of diff lines by dragging over
their numbers, by shift-clicking a number, by extending from the cursor
with shift and an arrow, or by selecting text across lines. A drag that
starts on a line number SHALL mark lines, never select text. Marked lines
SHALL stay lit until commented or cleared.

#### Scenario: Dragging over line numbers
- **WHEN** the operator presses on line 52's number and releases on line 56's
- **THEN** lines 52 to 56 are marked and a `comment` affordance stands under the last

#### Scenario: Selecting text
- **WHEN** the operator selects `.ok()?` with the pointer on line 55
- **THEN** line 55 is marked and the comment quotes the selected text

### Requirement: A comment is written under the lines it is about
The comment action SHALL open a composer under the marked lines, or the
cursor's line when none is marked. Its add action, bound by default to a
chord every terminal sends and offered as a control, SHALL keep it as a
draft, and cancel SHALL drop it. A draft SHALL be drawn under its lines,
editable and deletable until sent.

#### Scenario: A draft is added
- **WHEN** the operator marks lines 52-56, opens the composer, types and presses add
- **THEN** a draft stands under line 56 and lines 52-56 carry the comment mark

#### Scenario: An empty comment is not kept
- **WHEN** the operator presses add with nothing typed
- **THEN** no draft is added

#### Scenario: A terminal without an enhanced keyboard
- **WHEN** the terminal cannot send `ctrl+enter`
- **THEN** the composer's add is still reachable by its default chord and by its control

### Requirement: A comment carries code
A comment SHALL draw a fenced block as code, coloured as the file's
language, and a backticked span as inline code. The composer SHALL offer
one control, which inserts an empty fenced block named for the file's
language on lines of its own, with the caret on its empty line.

#### Scenario: An empty block to write in
- **WHEN** the operator presses the code-block control on a comment about `naming.rs` and types `refuse(name)?;`
- **THEN** the text holds `refuse(name)?;` inside a block fenced as `rs`, and the saved comment draws it as code

#### Scenario: Inline code typed
- **WHEN** the operator types `` `rev-parse` `` in a comment
- **THEN** the saved comment draws `rev-parse` as code

### Requirement: The open threads can be walked
The review SHALL offer, beside the viewed mark, a walk through the open
comments of the files listed: a step back, how many there are and which
the operator is on, and a step on, by pointer and by key. A step SHALL
cross files and go round the ends, and SHALL show the comment with its
lines marked.

#### Scenario: On to a thread in another file
- **WHEN** the operator steps on from the last thread of `naming.rs` and the next open comment is on `tests.rs`
- **THEN** `tests.rs` is selected, its diff shows the comment with its lines marked, and the count reads `2/2`

### Requirement: A file can be marked viewed
Each listed file SHALL carry a viewed mark the operator toggles by pointer
or by the viewed action. The action on an unviewed file SHALL mark it and
select the next unviewed one. The list SHALL say how many of its files are
viewed.

#### Scenario: Viewed and on to the next
- **WHEN** the operator presses `v` on `naming.rs` with `tests.rs` the next unviewed file
- **THEN** `naming.rs` is marked viewed, `tests.rs` is selected, and the count rises by one

### Requirement: A viewed mark falls away when the file changes
A viewed mark SHALL hold only for the diff it was given on. When the file's
diff changes afterwards, its mark SHALL show that the file changed since it
was viewed, and SHALL NOT count as viewed.

#### Scenario: The agent changes a viewed file
- **WHEN** `naming.rs` was marked viewed and the agent then commits a change to it
- **THEN** `naming.rs` shows it changed since it was viewed, and the viewed count drops by one

### Requirement: Drafts and viewed marks outlive the session
Comments, their answers and viewed marks SHALL be kept per agent among the
project's records, across workspace sessions, slot moves and the agent
naming its work, and SHALL go when the agent is forgotten. A record that
cannot be read SHALL be reported and never written over.

#### Scenario: Reopening the workspace
- **WHEN** the operator writes two drafts, quits the workspace and opens it again on the same agent
- **THEN** the review shows both drafts on their lines

#### Scenario: The agent names its work
- **WHEN** the operator has two drafts and the agent runs `uze agent work name fix/branch-naming`, renaming its branch
- **THEN** both drafts are still there

### Requirement: A send tells the agent where the review is
The send action SHALL type one line into the review's agent through the
agent bridge, which shows it first and refuses an agent that is not ready:
how many comments wait and the command that shows them. Drafts SHALL
become sent comments of a numbered send only once the line is typed.

#### Scenario: Two drafts sent
- **WHEN** the operator sends with two drafts and the agent is ready
- **THEN** one line saying two review comments wait, and naming `uze agent review show`, is shown and then typed, and both comments read `sent`

#### Scenario: A busy agent
- **WHEN** the operator sends while the agent's turn is running
- **THEN** nothing is typed, the drafts stay drafts, and the notice says the agent is busy

#### Scenario: The operator declines
- **WHEN** the operator declines the bridge's confirmation
- **THEN** the drafts stay drafts

### Requirement: The agent reads the review through the CLI
`uze agent review show` SHALL be accepted only from an agent UZE launched,
and SHALL print that agent's sent, unanswered comments: send and comment
number, path, lines, the quoted lines and the text, with code blocks
intact, and how to reply. It SHALL exit 0 however it fails.

#### Scenario: The agent reads its review
- **WHEN** the agent runs `uze agent review show` after send 3 with two comments
- **THEN** it prints `3/1` and `3/2` with their paths, lines, quotes and text

#### Scenario: Run outside an agent
- **WHEN** a plain shell runs `uze agent review show`
- **THEN** nothing is printed but a line saying it is not an agent UZE launched

### Requirement: The agent answers a comment where it was asked
`uze agent review reply <send>/<n> <text>` SHALL be accepted only from the
agent the comment was sent to, SHALL attach the text as its answer, and
SHALL never change the comment's own fields. The review SHALL draw the
answer under the comment and offer to resolve it.

#### Scenario: An answer arrives
- **WHEN** the agent runs `uze agent review reply 3/1 "apply now refuses a taken branch"`
- **THEN** comment 3/1 shows the answer and a `resolve` action, and the door counts it as waiting

#### Scenario: A reference that names no comment
- **WHEN** the agent replies to `3/9` and send 3 has two comments
- **THEN** nothing is attached and the command says there is no such comment

#### Scenario: Another agent's comment
- **WHEN** a second agent on the same project replies to a comment sent to the first
- **THEN** nothing is attached

### Requirement: An answer is bounded and drawn as text
An answer SHALL be bounded in bytes, and a comment SHALL hold a bounded
number of answers. Control and escape characters SHALL be removed when an
answer is written and again when it is drawn. The launch key identifies
the agent; it is not a trust boundary, and the record is read as untrusted.

#### Scenario: An escape sequence in an answer
- **WHEN** an answer holds a terminal escape sequence
- **THEN** the review draws it as plain text, without the sequence

#### Scenario: An oversized answer
- **WHEN** an answer exceeds the bound
- **THEN** it is cut at the bound and the review says it was cut

### Requirement: A comment survives the lines it was about changing
A sent comment whose lines are no longer in the diff SHALL stay in the
review at the end of its file, with its quoted lines and a note that they
changed. Resolving SHALL take a comment out of the diff and the door's
count.

#### Scenario: The agent rewrote the commented function
- **WHEN** comment 3/1 was on lines 52-56 and the agent's commit replaced them
- **THEN** comment 3/1 stands at the end of `naming.rs` quoting the old lines, marked as changed

### Requirement: A change is discarded only where it is uncommitted
The review SHALL offer to discard a file's changes only when that file has
uncommitted changes, and discarding SHALL restore it to the last commit,
after the confirmation the surface already asks for.

#### Scenario: A file changed only by commits
- **WHEN** `docs/workspace.md` differs from the target only through committed work
- **THEN** its row offers no discard

### Requirement: A marked line opens in the code surface
The open-in-code action SHALL open the code surface on the marked file at
the first marked line, or the cursor's, by the path hand-off the surfaces
share. A removed line SHALL open at the nearest line that still exists.

#### Scenario: From the review to the file
- **WHEN** the operator opens in code with line 53 of `naming.rs` marked
- **THEN** the code surface shows `naming.rs`'s contents with line 53 in view

### Requirement: Every review key is a named action with a pointer twin
Comment, add, cancel, send, viewed, open in code, extend the mark and
resolve SHALL be named, rebindable actions in the review's own scope,
listed in the action index, and each SHALL have a control the pointer
reaches, its hint clicked being its key.

#### Scenario: A rebound send
- **WHEN** the operator's keymap binds the review's send to another chord
- **THEN** the hint bar shows that chord, and clicking the hint sends

### Requirement: A changed image is drawn in the review
When the selected file is a binary file in one of the drawn image formats
and exists in the working tree, the review SHALL draw it as it stands in
the working tree instead of saying Git has no lines to compare. A deleted
image, or one that cannot be decoded, SHALL keep that message.

#### Scenario: An updated screenshot
- **WHEN** `web/public/poster.png` is modified and selected in the review
- **THEN** the working tree's version of the image is drawn under its path

#### Scenario: A deleted image
- **WHEN** a tracked `logo.png` is deleted and selected in the review
- **THEN** the review says Git records that it changed, with no image

### Requirement: Reading the review never blocks the workspace
Every read the review makes (the file list, a file's diff, the door's
counts, the merge base, its record) SHALL run off the drawing thread, and
an answer to a question no longer asked SHALL be dropped on arrival.

#### Scenario: Moving through files quickly
- **WHEN** the operator holds the down arrow through twenty changed files
- **THEN** the frame keeps drawing, and only the diff of the file selected last is drawn

### Requirement: Returning to an agent shows the review as it was left
When an agent comes back to the front with the review kept for it, the
review SHALL show the same file, marks, composer text and scroll.

#### Scenario: A half-written comment
- **WHEN** the operator is typing a comment on agent A, selects agent B, then agent A
- **THEN** the review on A shows the composer with the text typed so far
