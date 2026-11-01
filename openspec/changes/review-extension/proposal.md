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
- Comments and viewed marks are kept per agent among the project's
  records, across sessions, slot moves and the agent naming its work.
- `s` types one line into the agent through the existing agent bridge,
  which shows it first and refuses a busy agent. The line says how many
  comments wait and how to read them. The bridge only carries one line of
  bounded text, so the review itself travels by CLI.
- A launched agent reads the review with `uze agent review show` and
  answers a comment with `uze agent review reply <send>/<n> <text>`.
  Answers are bounded and escape-stripped. The answer is drawn under the
  comment, which the operator then resolves. A comment whose lines changed
  after it was sent is kept and marked as changed.
- Discarding a file's changes is offered only where it has uncommitted
  changes.
- `o` opens the marked line in `code`, by the path hand-off the surfaces
  already use.

## Capabilities

### New Capabilities
- `review-surface`: the review extension: its door, its diff against the
  target, marking and commenting, viewed marks, what persists, sending to
  the agent and the agent's answers, and the hand-off to code.

### Modified Capabilities
- `code-surface`: the surface no longer has a changes half. The map,
  map-selection and return requirements stop naming halves, the
  changed-image requirement moves to `review-surface`, and a handed path
  opens at its line.
- `architect-surface`: its "changes" entry point opens the review instead
  of the code surface.

## Impact

- `crates/uze-extensions`: a new `review/` extension takes `code/changes.rs`,
  `code/diff.rs` and the changed row's actions. Git status parsing moves to
  `shared/`, since the map and history still read it. `code.rs` loses its
  changes modes and its doc's case for one extension. The contract gains
  `door_badge`, `agent_write`, `selected` and a line on path hand-offs.
  `Outcome::Review` is renamed to `Outcome::Consent`.
- `src/ui`: the tab strip draws the review's door from `door_badge`.
  `GitBadge` and the host's own change-summary read are deleted.
  `ToggleChanges`, the extension switch, the hosted engines and new
  `Review` scopes and actions move or are added.
- `uze-core`/`uze-application`: a record at
  `state/projects/<project>/reviews/<task>.json` with its lock, in `UzeHome`.
- CLI: `uze agent review show` and `uze agent review reply`, hidden like
  every `uze agent` verb, `Budgeted`, and documented in the launch
  instructions.
- Docs: the `extensions` rule, the `design-system` skill,
  `docs/architecture/workspace-components.mmd`, and the workspace pages
  `code.mdx`, `keys.mdx` and a new `review.mdx`.
- Journeys: the changes journey moves to the review's door, and a review
  journey covers send, show and reply.
- Depends on the `extension-agent-bridge` change (the prompt write and
  agent events).
