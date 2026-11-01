## Why

Agents now write most of a change, and the person who owns it often never
reads it: a large diff goes to someone else's review, or to another agent's,
and nobody looks at the code. UZE already shows a checkout's changes, but
only against its last commit and with no way to say anything back. Reviewing
the agent's branch line by line, and handing what you found back to the
agent, should be the shortest path in the workspace, not an export to a
forge.

The code extension also carries too much. It holds the changes, the files,
the editor, the map and the timeline in one surface. Review adds state that
must outlive a session (drafts, viewed marks), a conversation with the agent
and a different base for the diff. That is a separate job, not another way
of reading the same tree.

## What Changes

- **BREAKING (workspace UI)**: the changes half leaves the `code` surface.
  A new built-in extension, `review`, owns it. `code` keeps `files` and
  `map`; its search stays the field over the file tree.
- The review diffs the checkout against its target's merge base, working
  tree included, with an `uncommitted` filter for what is not committed yet.
- The `+N −M` count on the tab strip becomes the review's door and is always
  drawn. With nothing to review it reads `review`; with drafts it adds `◆n`;
  with answers waiting it turns them to the success hue. Today it disappears
  on a clean checkout.
- Lines are marked by dragging over line numbers, shift-extending from the
  keyboard, or selecting text. `c` opens a comment under the marked lines.
- A comment is plain text with inline code and fenced code blocks. One
  control inserts the marked lines as a block, and another wraps the
  selection as code. Both kinds of code are drawn as code in the diff.
- Each file can be marked viewed (`v` marks it and moves to the next unviewed
  file). A mark falls away when the file changes after it was given.
- Drafts and viewed marks are kept per checkout and branch, across sessions,
  until the branch is gone.
- `s` sends every draft to the checkout's agent as one prompt through the
  existing agent bridge, which shows it before typing and refuses a busy
  agent.
- A launched agent answers a comment with `uze agent review reply <n>
  <text>`. The answer is drawn under the comment, which the operator then
  resolves. A comment whose lines changed after it was sent is kept and
  marked as changed.
- `o` opens the marked line in `code`, by the path hand-off the surfaces
  already use.

## Capabilities

### New Capabilities
- `review-surface`: the review extension: its door, its diff against the
  target, marking and commenting, viewed marks, what persists, sending to
  the agent and the agent's answers, and the hand-off to code.

### Modified Capabilities
- `code-surface`: the surface no longer has a changes half. The map
  requirement stops naming it, and the changed-image requirement moves to
  `review-surface`.

## Impact

- `crates/uze-extensions`: a new `review/` extension takes `code/changes.rs`
  and `code/diff.rs`. `code.rs` loses `ContentMode::Diff` and
  `NavigatorMode::Changes`, along with its doc's case for one extension.
  The registry gets one entry. The contract gets a door label an extension
  answers, defaulting to none.
- `src/ui`: the tab strip draws the review's door from that label instead of
  `GitBadge`'s `ChangeSummary`. `WorkspaceHit::OpenChanges` becomes the
  review's door. The keymap's changes door moves to review.
- `uze-application`: a record for review drafts and viewed marks, named in
  `UzeHome` under `state/`. The diff base comes from the checkout's target,
  which `contract::Checkout` already carries.
- CLI: `uze agent review reply`, hidden like every `uze agent` verb,
  classified in `command_performance.rs`, and documented in the launch
  instructions.
- Docs: the `extensions` rule (code is no longer one extension for four
  halves), the `design-system` skill (the badge is always drawn), and
  `docs/architecture/workspace-components.mmd`.
- Journeys: the code journey's changes scenes move to a review journey.
