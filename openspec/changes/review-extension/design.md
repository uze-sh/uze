## Context

The code extension (`crates/uze-extensions/src/code.rs` and `code/`, about
14k lines) holds four halves: changes, files (with the editor and the
search field), history and map. Its module doc argues for one extension:
the selection, a path, has to survive a switch between halves, and "I see
it in the diff, let me fix that line" is the most-crossed seam. The
`extensions` rule repeats that argument.

Two facts have moved since then:

- A surface can already be opened on a path from outside. `Outcome::OpenPath`
  and `Entry::Path` carry a pane's ctrl+click (#339) and the architect's
  links to the code surface, so crossing from one surface to another is one
  gesture and no longer a close and a re-navigation.
- The tab strip's `+N −M` count (`render/tab_strip.rs`, fed by
  `GitBadge { summary: code::ChangeSummary }` and hit as
  `WorkspaceHit::OpenChanges`) already is a door of its own, separate from
  `code`'s door. It is absent on a clean checkout on purpose: "a badge with
  nothing to say says nothing rather than zero".

The pieces a review needs mostly exist. `contract::Checkout.target` names
the branch a checkout delivers to. `Outcome::Agent(AgentAsk::Prompt)` types
into an agent after showing the text and refuses a busy one
(`extension-agent-actions`). `uze agent report` already proves a caller is a
launched agent by its launch key.

The prototype this design follows is the HTML artifact made with the
operator during exploration (review v4).

## Goals / Non-Goals

**Goals:**
- Give review its own built-in extension, with `code` left to navigate and
  edit, so each module doc can state one job.
- Diff against the target's merge base, so the review covers what the
  branch will deliver and not only what is uncommitted.
- Keep drafts and viewed marks when the workspace closes.
- Close the loop with the agent in both directions, through mechanisms UZE
  already has.

**Non-Goals:**
- Kinds of comment (change, question, nit), rounds or a diff since the last
  round, a split diff, approve and land from the review, suggestion blocks
  the agent applies verbatim. The operator cut all of these from the
  prototype as noise.
- Posting to a forge. A pull request's review stays the forge's business.
- Reviewing a checkout that is not the tab's own.
- Letting a declared (plugin) extension read or write review records.

## Decisions

### 1. `review` is a built-in extension that takes the changes half whole

`code/changes.rs`, `code/diff.rs`, the changes navigator and the changed
row menu (open, copy path, discard) move to `crates/uze-extensions/src/review/`,
with `review.rs` for the surface and `review/extension.rs` for its
`contract::Extension`, plus one `ExtensionRegistry::builtin` entry.
`ContentMode::Diff` and `NavigatorMode::Changes` leave `code`. The "why one
extension" section of `code.rs` is rewritten to cover files, history and
map.

Discard stays with the changed file's row, so the review becomes a second
caller of `restore_to_head`. The `extensions` rule, which today names
`code/request.rs` as the only caller of the write capabilities, names both.

*Alternative considered:* keeping review as a fourth half of `code`. It
keeps the selection for free, but adds persisted state, an agent
conversation and a second diff base to a surface whose rule is "no handler
branches on the mode to decide what the state means". Rejected.

*Alternative considered:* moving `diff.rs` to `shared/`. Rejected: once the
changes half leaves, `code` no longer draws a diff, and `shared/` takes a
module only when a second extension needs it.

### 2. The door is the extension's, and is always drawn

A new contract method, `Extension::door_label(&self, checkout) ->
Option<DoorLabel>` (default `None`), lets an extension answer a short
styled label for its view's door. The host draws it in the place the
`+N −M` count stands today. The review answers counts, `◆n`, the
answered hue, or `review`. `GitBadge`'s `ChangeSummary` stops being read
by the tab strip, and `WorkspaceHit::OpenChanges` becomes the review
view's door (keeping its name in the keymap's `ToggleChanges` until a
rename is worth the churn).

The count stays a cheap read on the badge's existing timer. It runs
`git diff --numstat <merge-base>`, with the merge base cached per target
tip, plus the untracked-file accounting the badge already does.

*Alternative considered:* keeping the badge in the host and adding a second
`review` chip. Rejected: two chips for one door, and the operator asked for
the count itself to become the door.

This reverses a design-system rule (the badge vanishes on a clean
checkout), so the `design-system` skill is updated in the same change.

### 3. The diff base is the merge base of the target, working tree included

With a known target the review asks for `git merge-base <target> HEAD`
and diffs that commit against the working tree, untracked files listed
as additions. The `uncommitted` filter is today's diff against `HEAD`.
With no evaluated target it uses `HEAD` and says the target is unknown.
All of this goes through the jobs the host runs off the drawing thread
(`ui-orchestrator-reads`).

### 4. Comments are anchored by their lines' text, not their numbers

A comment stores its path, the marked rows (side, line number and text)
and any selected excerpt. It is placed by finding that run of rows in the
current diff. When the run is gone, the comment is drawn at the end of its
file with its quoted rows and "these lines changed". Line numbers alone
would move silently onto unrelated code after the agent's next commit.

### 5. Drafts and viewed marks are a record under `state/`

Deleting them costs the operator work nothing else knows, so they belong in
the `record` tier. The record is
`state/review/<repository-id>/<branch>.json`, named in `UzeHome`,
implementing `uze_document::Shaped` with a version ladder. It holds the
comments (status `draft | sent | answered | resolved`, the answer and its
author) and the viewed marks as `path → diff fingerprint`. The extension
reads and writes it only through two new `Host` capabilities,
`review_record` and `write_review_record`, implemented in
`src/ui/extension_host.rs` over `uze-application`. A record whose branch
no longer exists is pruned on the workspace's existing evaluation pass.

*Alternative considered:* inside the repository's Git directory, beside
`uze-checkout.json`. Rejected: the record is per branch, not per checkout,
and a review should survive moving the work to another slot.

### 6. Sending is one `AgentAsk::Prompt`; answering is one CLI verb

`s` builds one prompt (numbered comments, path and lines, up to six quoted
lines, the text, and how to reply) and returns `Outcome::Agent` for the
checkout's agent. The bridge's confirmation is the preview, and its
refusal of a busy agent leaves the drafts as drafts. Comments become
`sent` only once the bridge reports the prompt typed.

The agent answers with `uze agent review reply <n> <text>`. It is
authenticated as `uze agent report` is (by the launch key), appends the
answer to the record for that agent's repository and branch, and exits 0
however it fails, so a turn is never blocked. The review watches its
record and redraws.

*Alternative considered:* reading answers out of the agent's transcript.
Rejected: transcript formats are each harness's own, and core may not name
a harness.

*Alternative considered:* answers as journal events. Rejected: the journal
carries no free text by design, and an answer is free text.

### 7. Open in code passes a line

`Entry::Path` gains `line: Option<u32>` and `Outcome::OpenPath` carries it.
`code` scrolls its contents to that line and puts the caret there. The
review sends the first marked row's new-side line, or the next row that
has one when the marked row was removed.

### Diagram

`docs/architecture/workspace-components.mmd` lists the workspace client's
surfaces ("spec, architect and code"), so `review` is added there. No
container or crate changes: the extension lives in `uze-extensions` and
its record in the existing state tier.

## Candidate ADRs

- **Review leaves the code surface**: it reverses the recorded "one
  extension so the selection survives" decision, now that path hand-off
  makes crossing surfaces one gesture.
- **Agents answer the operator through a launch-keyed CLI verb into a UZE
  record**: a new channel from agent to UZE carrying free text, beside the
  bounded journal.

## Risks / Trade-offs

- **Losing the in-surface seam** → mitigated by `o` and the line-carrying
  path hand-off. A return trip is `esc` on `code` and the review's door,
  and the review holds its place per agent.
- **Merge-base reads on every badge tick are slower than `git status`** →
  cache the merge base per (target tip, HEAD), and recompute numstat only
  when either moves or the working tree reports a change.
- **An agent may never reply** → the comment still shows `sent`, and once
  its lines change it shows that they changed. The operator resolves by
  hand.
- **Prompt length** → comments quote at most six lines each. A review that
  is still too long for one prompt is refused by the bridge's own bound
  with its reason. Writing it to a file and sending its path is a
  follow-up, not part of this change.
- **Journey churn** → `04-an-agents-changes-are-its-own.yml` drives the
  changes list through `code`. It moves to the review's door unchanged in
  what it proves.
