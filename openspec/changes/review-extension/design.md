## Context

The code extension (`crates/uze-extensions/src/code.rs` and `code/`, about
14k lines) holds the changes, the files (with the editor and the search
field) and the map as modes, plus the sidebar's timeline. Its module doc
argues for one extension: the selection, a path, has to survive a switch
between halves, and "I see it in the diff, let me fix that line" is the
most-crossed seam. The `extensions` rule repeats that argument.

Two facts have moved since then:

- A surface can already be opened on a path from outside. `Outcome::OpenPath`
  and `Entry::Path` carry a pane's ctrl+click (#339) and the architect's
  links to the code surface (`surfaces.rs`, `open_code_at`), so crossing
  surfaces is one gesture.
- The tab strip's `+N −M` count already is a door of its own
  (`render/tab_strip.rs`, hit as `WorkspaceHit::OpenChanges`, which is
  `Action::ToggleChanges` on alt+g). The host reads it itself, by name: it
  spawns `code::change_summary` (`orchestrator/reads.rs`, `answers.rs`) into
  `GitBadge`, gated on the code extension being on. It is absent on a clean
  checkout on purpose (the comment at `tab_strip.rs:230`).

Other pieces exist. `contract::Checkout.target` names the branch a
checkout delivers to. `Outcome::Agent(AgentAsk::Prompt)` types into an
agent after a confirmation, refuses a busy one and never queues. It types
only one line of text with no control characters, at most 2048 bytes
(`declared/agent.rs`, `strictly_text`, `PROMPT_MAX`), and at most one
prompt per extension and agent per turn and per minute, attended or not.
`uze agent report` proves its caller is a launched agent by the launch key
and always exits 0.

The prototype this design follows is the HTML artifact made with the
operator during exploration (review v4).

## Goals / Non-Goals

**Goals:**
- Give review its own built-in extension, with `code` left to navigate and
  edit, so each module doc can state one job.
- Diff against the target's merge base, so the review covers what the
  branch will deliver and not only what is uncommitted.
- Keep comments and viewed marks when the workspace closes, the slot
  moves or the work is named.
- Close the loop with the agent in both directions, within the bridge's
  bounds.

**Non-Goals:**
- Kinds of comment (change, question, nit), rounds or a diff since the last
  round, a split diff, approve and land from the review, suggestion blocks
  the agent applies verbatim. The operator cut all of these from the
  prototype as noise.
- Posting to a forge.
- Reviewing a checkout that is not the tab's own, or one with no agent.
- Letting a declared (plugin) extension read or write review records.
- Raising the bridge's bounds.

## Decisions

### 1. `review` is a built-in extension; code keeps files and map

`code/changes.rs`, `code/diff.rs`, the changes navigator and the changed
row's actions (open, copy paths, discard) move to
`crates/uze-extensions/src/review/`, with `review.rs` for the surface and
`review/extension.rs` for its `contract::Extension`. The new extension is
registered where built-ins are actually composed, not only in the catalog:
`ExtensionRegistry::builtin` (`registry.rs`), the hosted engines
(`orchestrator/declared.rs`, `builtins()`), its scope in `render.rs` and
`session/keys.rs`, and its switch in `extension_switch.rs`, which today
gives `ToggleChanges` to `code`.

`ContentMode::Diff` and `NavigatorMode::Changes` leave `code`, whose
default mode becomes `Contents`. `code::CATALOG`'s description and usage
drop the changes. The "why one extension" section of `code.rs` is
rewritten for files and map. The timeline stays code's section.

Git porcelain and numstat parsing (`changes::STATUS_ARGS`,
`parse_numstat`, the porcelain parser) move to
`shared/git_status.rs`, because `code/map.rs` (uncommitted marks on the
map) and `code/history.rs` still read them. That is a second extension
needing them, so `shared/` takes them.

*Alternative considered:* review as a fourth half of `code`. Rejected: it
adds persisted state, an agent conversation and a second diff base to a
surface whose rule is that no handler branches on the mode to decide what
the state means.

### 2. The door's label is the extension's answer, read by the extension

A new contract method, `Extension::door_badge(&self) -> Option<DoorBadge>`
(default `None`), answers a short styled label for the extension's view
door, for the checkout it is focused on. It is not named `door_label`,
which `render/bar.rs` already uses. The host draws it where the `+N −M`
count stands today, and only while the review is switched on.

The counts are the review's own read: a `Job` from `jobs()` on the
badge's present cadence, as the rule requires ("what it reads is a `Job`
it hands over, never a read the host spawns for it by name"). Engines are
pumped every pass whether or not their view is visible, so the door stays
current. `GitBadge`, its spawn and its absorb are deleted, and the
`offers_extension(code)` gate moves to review.

The review answers `+a -d`, or `review` (D12: what waits in the review is
not on the door). From the bridge reporting a send typed (§6) until that
agent's next `turn.finished`, which the built-in learns through
`agent_events`, the review reads its record again as soon as the turn ends.

This reverses the tab strip's "absent on a clean checkout" behaviour. That
rule lives in the `tab_strip.rs` comment and the test
`tests/surfaces.rs:895`, not in the `design-system` skill. Both change, and
the skill gains the rule.

### 3. The diff base is the merge base of the fetched target

With a known target, the review takes the merge base of `HEAD` with
`refs/uze/sync/<target>` when it exists, else the local target, and diffs
it against the working tree, untracked files listed as additions. Every
call passes `--end-of-options`, as `checkout/integration.rs` does. The
merge base is cached per (target tip, `HEAD`).

The `uncommitted` filter is today's diff against `HEAD`. It is also what
shows, with the reason said, when no target is evaluated, or when the
checkout is on the target branch itself (an agent on the operator's
branch). A detached `HEAD` still has a merge base.

### 4. Comments are anchored by their lines' text

A comment stores its path, the marked rows (side, line number, text) and
any selected excerpt. It is placed by finding that run of rows in the
current diff. When the run is gone, it is drawn at the end of its file
with its quote and "these lines changed".

### 5. The record is per agent, inside the project's directory, under a lock

Deleting it costs the operator work nothing else knows, so it is a
`record`. The `persisted-state` spec keeps all of a project's records in
one directory. So the record is
`state/projects/<project_id>/reviews/<task_id>.json`, named in `UzeHome`
beside `conversation_path`, with `review_lock_path(project_id, task_id)`
like the agents and conversations locks. It implements
`uze_document::Shaped` with a version ladder. Every write is an atomic
replace under the lock, because the workspace and `uze agent review reply`
both write it.

It is keyed by the task id, not the branch. `uze agent work name` renames
branches, unnamed work is named later from its first commit, and a branch
name holds `/` and characters some filesystems refuse. The task id
survives all of that and slot moves. The record goes with the agent's
other records when the agent is forgotten.

It holds:
- comments: rows, excerpt and text, the operator's fields
- status: `draft | sent | answered | resolved`
- `send` and `n` once sent, and the agent each was sent to
- answers: bounded text
- viewed marks: `path → diff fingerprint`

The extension reaches it only through two new `Host` capabilities,
`review_record` and `write_review_record`, implemented in
`src/ui/extension_host.rs` over a `uze-application` service, off the
drawing thread. A declared extension's host answers neither. The review
learns of an answer by re-reading the record's modification time in its
own job on the badge's cadence, since `watches()` covers checkout paths
only.

### 6. A send is one line; the review travels by CLI

The bridge cannot carry a multi-line review, so the send does not try.
The send action returns `Outcome::Agent(Prompt)` with one line under the
bound and free of command characters, for example:
`2 review comments wait for you (send 3): run uze agent review show, then
answer each with uze agent review reply 3/<n> "<answer>"`.

A new contract method, `Extension::agent_write(&mut self, action, outcome)`
(default no-op), tells the extension whether the write was typed, refused
(with the reason) or declined. It is called from `answer_agent_write` and
`type_agent_writes`. Drafts become `sent` with a send number only on
`typed`, so a refused or declined send leaves them as drafts. The bridge's
one-prompt-per-turn rate applies, and a second send in the same turn is
refused with its reason.

`uze agent review show` prints the agent's sent, unanswered comments in
full, with code blocks intact. `uze agent review reply <send>/<n> <text>`
attaches an answer. Both authenticate as `uze agent report` does, resolve
the record by the launch identity's project and task id (no Git, so both
are `Budgeted`), and exit 0 with a line on stderr when refused. A reply is
accepted only for a comment sent to that agent. It writes only the answer,
never the comment's fields.

An answer is bounded (4 KiB, at most 8 answers per comment), and control
and escape characters are removed on write and again when drawn
(`Clean::label`'s treatment). The launch key is inherited by every process
the agent starts, and any process of the operator's user can edit
`$UZE_HOME`. So the key identifies the agent and is not a trust
boundary, and the record is read as untrusted input.

*Alternative considered:* the whole review in the prompt. Rejected: the
bridge refuses line breaks and anything over 2048 bytes, and the
confirmation must show the text whole.

*Alternative considered:* answers in the transcript or the journal.
Rejected: transcripts are each harness's own, and the journal carries no
free text.

### 7. Marking, keys and scope

The review gets its own `Scope::Review`, plus `Scope::ReviewComposing`
while the composer has the keys, as `CodeEditing` does for the editor. It
also gets named actions with defaults in `crates/uze-keys/src/load.rs`:
`review-comment` (c), `review-add` (ctrl+s, which every terminal sends,
since ctrl+enter is enhancement-only), `review-cancel` (esc),
`review-send` (s), `review-viewed` (v), `review-open-in-code` (o),
`review-extend-mark` (shift+↑↓) and `review-resolve`. Each is in the
action index and has a pointer twin: the hint bar's clickable hints, the
`comment` chip, the composer's `add` control, the viewed checks and
`resolve`.

A drag that starts on a line number belongs to the review: the view marks
the number column as the extension's pointer target, so the host does not
start a text selection there. A text selection elsewhere stays the
host's (`src/ui/selection.rs`). A new contract method,
`Extension::selected(&mut self, surface, rows, excerpt)` (default no-op),
tells the extension which rows and text it covered, so `c` can comment on
it.

### 8. Discard stays with the changed row, for uncommitted changes only

`restore_to_head` restores to `HEAD`. Against the merge base, a file
changed only by commits would stay listed after a discard. So the row
offers discard only when the file has uncommitted changes. The review
becomes the second caller of the write capabilities, and the `extensions`
rule names both.

### 9. Open in code carries a line

`Entry::Path` gains `line: Option<u32>` and `Outcome::OpenPath` carries
it. `code` scrolls to that line and puts the caret there. The pane link
already parses a trailing `:line` (`pane_link.rs`) and drops it in
`session/pane_links.rs`. It now passes it through, and the architect
passes `None`.

### Diagram

`docs/architecture/workspace-components.mmd` lists the workspace client's
surfaces ("spec, architect and code"), so `review` is added. No container
or crate changes.

### Vocabulary

`Outcome::Review` (`contract.rs`) and the design-system's withheld-extension
"review" row already use the word for "review this extension's consent".
The new extension's id is `review`, and the door reads `review`. Before
implementation starts, `Outcome::Review` is renamed to
`Outcome::Consent`, so one word does not mean two things in the contract.

## Candidate ADRs

- **Review leaves the code surface**: it reverses the recorded "one
  extension so the selection survives" decision, now that path hand-off
  makes crossing surfaces one gesture.
- **Agents answer the operator through a launch-keyed CLI verb into a UZE
  record**: a new channel from agent to UZE carrying bounded free text,
  beside the journal.

## Risks / Trade-offs

- **Losing the in-surface seam** → mitigated by open-in-code with a line.
  The way back is the review's door, and the review holds its place per
  agent.
- **Merge-base reads on the badge's cadence** → cached per (target tip,
  `HEAD`). Numstat is re-read only when either moves or the working tree
  reports a change.
- **An agent ignores the send, or never replies** → the comment stays
  `sent`, and once its lines change it says so. The operator resolves by
  hand.
- **The agent cannot run the CLI** (a harness whose sandbox blocks it) →
  the line still says how many comments wait. Show the review in the
  prompt's place for such harnesses as a follow-up, not in this change.
- **A forged answer** from a process holding the key, or writing the file
  → bounded, escape-stripped, drawn as an agent's answer only, and never
  able to change what the operator wrote.
- **Journey churn** → `04-an-agents-changes-are-its-own.yml` drives the
  changes list through `code`. It moves to the review's door and still
  proves the same thing.
