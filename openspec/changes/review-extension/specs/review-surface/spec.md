## Purpose

The review surface is where a person reads what an agent's branch adds
against its target, says what should change, line by line, and hands that
to the agent in one send, then reads the answers where they asked.

## ADDED Requirements

### Requirement: The review has a door that is always on the tab strip
The tab strip SHALL always draw the review's door in an agent's context:
the branch's additions and deletions against its target when there are
any, and the word `review` when there are none. It SHALL NOT disappear on
a clean checkout, and pressing it SHALL open the review, or close it when
it is the surface in front.

#### Scenario: A branch with work on it
- **WHEN** the checkout's branch adds 81 lines and removes 16 against its target
- **THEN** the door reads `+81 -16`

#### Scenario: Nothing differs from the target
- **WHEN** the branch and its working tree hold nothing the target does not
- **THEN** the door reads `review` in a held-back tone
- **AND THEN** opening it says that nothing on the branch differs from the target

#### Scenario: The door closes what it opened
- **WHEN** the review is in front and the operator presses its door
- **THEN** the review is put away and the agent's pane is shown

### Requirement: The door says what waits on the operator
The door SHALL add `◆n` for n unsent drafts, `◆n` in the success hue for n
answered comments not yet resolved, and a working mark while the agent is
on a review it was sent. Drafts SHALL take precedence over answers.

#### Scenario: Drafts waiting to be sent
- **WHEN** the operator has written two comments and sent none
- **THEN** the door reads `+81 -16 ◆2`

#### Scenario: Answers waiting to be read
- **WHEN** the agent answered three sent comments and none is resolved or drafted
- **THEN** the door shows `◆3` in the success hue

### Requirement: The review diffs the branch against its target
The review SHALL show, per changed file, the difference between the merge
base of the checkout's target and the working tree, committed and
uncommitted work together. An `uncommitted` filter SHALL narrow it to what
is not committed yet. A checkout with no known target SHALL fall back to
its last commit and say so.

#### Scenario: Committed and uncommitted work in one diff
- **WHEN** the branch has three commits on `main` and one file edited but not committed
- **THEN** the review lists every file the commits and the edit touched

#### Scenario: Only what is not committed
- **WHEN** the operator chooses the `uncommitted` filter
- **THEN** only the edited, uncommitted file is listed

#### Scenario: No target known
- **WHEN** the checkout's target has not been evaluated
- **THEN** the review shows the changes against the last commit and says the target is unknown

### Requirement: Lines are marked by pointer, keyboard or text selection
The operator SHALL be able to mark a run of diff lines by dragging over
their numbers, by shift-clicking, by extending from the cursor with
shift and an arrow, or by selecting text across lines. The marked lines
SHALL stay lit until commented or cleared.

#### Scenario: Dragging over line numbers
- **WHEN** the operator presses on line 52's number and releases on line 56's
- **THEN** lines 52 to 56 are marked and a `c comment` affordance stands under the last

#### Scenario: Selecting text
- **WHEN** the operator selects `.ok()?` with the pointer on line 55
- **THEN** line 55 is marked and the comment quotes the selected text

### Requirement: A comment is written under the lines it is about
Pressing `c` SHALL open a comment under the marked lines, or the cursor's
line when none is marked. `ctrl+enter` SHALL add it as a draft and `esc`
SHALL drop it. A draft SHALL be drawn under its lines with a mark in the
gutter, and SHALL be editable and deletable until it is sent.

#### Scenario: A draft is added
- **WHEN** the operator marks lines 52-56, presses `c`, types and presses `ctrl+enter`
- **THEN** a draft stands under line 56 and lines 52-56 carry the comment mark

#### Scenario: An empty comment is not kept
- **WHEN** the operator presses `ctrl+enter` with nothing typed
- **THEN** no draft is added

### Requirement: A comment carries code
A comment SHALL draw a fenced block as code, coloured as the file's
language, and a backticked span as inline code. The composer SHALL offer
one control that inserts the marked lines as a fenced block, or fences the
selected text, and one that wraps the selected text in backticks.

#### Scenario: The marked lines as a block
- **WHEN** the operator presses the code-block control with lines 53-54 marked and nothing selected
- **THEN** the text gains those lines inside a fenced block, and the saved comment draws them as code

#### Scenario: Inline code
- **WHEN** the operator selects `rev-parse` in the comment and presses the inline-code control
- **THEN** it becomes `` `rev-parse` `` and is drawn as code

### Requirement: A file can be marked viewed
Each listed file SHALL carry a viewed mark the operator toggles by
pointer or by `v`. `v` on an unviewed file SHALL mark it and select the
next unviewed one. The list SHALL say how many of its files are viewed.

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
Drafts, sent comments, their answers and viewed marks SHALL be kept per
repository and branch across workspace sessions, and SHALL be discarded
once that branch no longer exists. A record that cannot be read SHALL be
reported and never written over.

#### Scenario: Reopening the workspace
- **WHEN** the operator writes two drafts, quits the workspace and opens it again on the same agent
- **THEN** the review shows both drafts on their lines

#### Scenario: The branch is gone
- **WHEN** the branch a review was kept for is deleted after landing
- **THEN** its review record is discarded

### Requirement: Every draft is sent to the agent as one prompt
`s` SHALL send every draft as one prompt to the agent whose checkout the
review is on, through the agent bridge, which shows the text before typing
and refuses an agent that is not ready. The prompt SHALL number each
comment and give its path, its lines, the quoted lines and its text. Sent
drafts SHALL become sent comments.

#### Scenario: Two drafts sent
- **WHEN** the operator presses `s` with two drafts and the agent is ready
- **THEN** one prompt naming comments 1 and 2 with their paths and lines is shown, then typed, and both comments read `sent`

#### Scenario: A busy agent
- **WHEN** the operator presses `s` while the agent's turn is running
- **THEN** nothing is typed, the drafts stay drafts, and the notice says the agent is busy

### Requirement: The agent answers a comment where it was asked
`uze agent review reply <n> <text>` SHALL be accepted only from an agent UZE
launched, for a comment sent to that agent, and SHALL attach the text to
comment n. The review SHALL draw the answer under the comment and offer to
resolve it. The prompt SHALL tell the agent how to reply.

#### Scenario: An answer arrives
- **WHEN** the agent runs `uze agent review reply 1 "apply now refuses a taken branch"`
- **THEN** comment 1 shows the answer and a `resolve` action, and the door counts it as waiting

#### Scenario: A reply from outside an agent
- **WHEN** a plain shell runs `uze agent review reply 1 "done"`
- **THEN** nothing is attached and the command says it is not an agent UZE launched

### Requirement: A comment survives the lines it was about changing
A sent comment whose lines are no longer in the diff SHALL stay in the
review, at the end of its file, with its quoted lines and a note that they
changed. Resolving SHALL take a comment out of the diff and the door's
count.

#### Scenario: The agent rewrote the commented function
- **WHEN** comment 1 was on lines 52-56 and the agent's commit replaced them
- **THEN** comment 1 stands at the end of `naming.rs` quoting the old lines, marked as changed

### Requirement: A marked line opens in the code surface
`o` SHALL open the code surface on the marked file at the first marked
line, or the cursor's line, using the path hand-off the surfaces share. A
removed line SHALL open at the nearest line that still exists.

#### Scenario: From the review to the file
- **WHEN** the operator presses `o` with line 53 of `naming.rs` marked
- **THEN** the code surface shows `naming.rs`'s contents with line 53 in view

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
Every Git read the review makes (the file list, a file's diff, the door's
counts, the merge base) SHALL run off the drawing thread, and an answer to
a question no longer asked SHALL be dropped on arrival.

#### Scenario: Moving through files quickly
- **WHEN** the operator holds the down arrow through twenty changed files
- **THEN** the frame keeps drawing, and only the diff of the file selected last is drawn

### Requirement: Returning to an agent shows the review as it was left
When an agent comes back to the front with the review kept for it, the
review SHALL show the same file, marks, open composer text and scroll.

#### Scenario: A half-written comment
- **WHEN** the operator is typing a comment on agent A, selects agent B, then agent A
- **THEN** the review on A shows the composer with the text typed so far
