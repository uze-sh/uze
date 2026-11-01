## 1. Contract and vocabulary

- [x] 1.1 Rename `Outcome::Review` to `Outcome::Consent` across `uze-extensions` and `src/ui`, so `review` names one thing
- [x] 1.2 Add `Extension::door_badge(&self) -> Option<DoorBadge>` (default `None`) and `DoorBadge` as styled spans
- [x] 1.3 Add `Extension::agent_write(&mut self, action, outcome)` with `Typed | Refused(reason) | Declined` (default no-op); call it from `answer_agent_write` on decline and from `type_agent_writes`; test each path
- [x] 1.4 Add `Extension::selected(&mut self, surface, rows, excerpt)` (default no-op), called when a host text selection over an extension view settles
- [x] 1.5 Add `line: Option<u32>` to `Entry::Path` and `Outcome::OpenPath`; pass the pane link's parsed `:line` through `session/pane_links.rs`, and `None` from the architect
- [x] 1.6 Add `Host::review_record` and `Host::write_review_record`, with defaults answering nothing; test that a declared extension's host refuses both

## 2. Split the changes out of code

- [ ] 2.1 Move the porcelain and numstat parsing (`STATUS_ARGS`, `parse_numstat`, the porcelain parser) to `shared/git_status.rs` and say why in `shared.rs`; point `code/map.rs` and `code/history.rs` at it
- [ ] 2.2 Create `review.rs` and `review/extension.rs`; register the extension in `ExtensionRegistry::builtin`, the hosted engines (`orchestrator/declared.rs`, `builtins()`), and its scope mapping in `render.rs` and `session/keys.rs`
- [ ] 2.3 Move `code/changes.rs`, `code/diff.rs` and the changed row's actions from `code/row_menu.rs` to `review/`, keeping their tests green
- [ ] 2.4 Remove `ContentMode::Diff` and `NavigatorMode::Changes` from `code.rs` and the branches that handled them; make `Contents` the default mode
- [ ] 2.5 Make `code` honour `Entry::Path { line }`: contents in view at that line, caret on it
- [ ] 2.6 Rewrite `code.rs`'s module doc (files and map; the timeline section) and `code::CATALOG`'s description and usage; write `review.rs`'s doc
- [ ] 2.7 Move the image-in-changes tests to review
- [ ] 2.8 Move `ToggleChanges` from code to review in `src/ui/extension_switch.rs`, and replace `open_code(code::CHANGES)` in `session/surface.rs` and `session/keys.rs` with opening the review

## 3. Diff against the target

- [ ] 3.1 Read the merge base of `HEAD` with `refs/uze/sync/<target>`, else the local target, as a job with `--end-of-options`, cached per (target tip, HEAD)
- [ ] 3.2 List files and read a file's diff against it, working tree and untracked files included; keep the `HEAD` read for `uncommitted`
- [ ] 3.3 Fall back to `uncommitted` with the reason when no target is evaluated or the checkout is on its target branch
- [ ] 3.4 Draw `all · uncommitted` on the surface's mode row and switch the reads with it
- [ ] 3.5 Test that an answer arriving after the selection moved is dropped, and that a stale local target hides already-fetched commits

## 4. The door

- [ ] 4.1 Compute the door's counts in the review's own `jobs()` on the badge's cadence; delete `GitBadge`, `spawn_git_read` and `absorb_git_read`, and move the `offers_extension` gate from code to review
- [ ] 4.2 Answer `door_badge`: `+a -d`, drafts `◆n`, answers `◆n` in the success hue, the working mark from `Typed` to the agent's next `turn.finished` (via `agent_events`), or `review`
- [ ] 4.3 Draw the door from `door_badge` in `render/tab_strip.rs`, always while the review is on, held back at rest and lit under the pointer; rewrite the comment at `tab_strip.rs:230`
- [ ] 4.4 Route `WorkspaceHit::OpenChanges` and `ToggleChanges` to the review, closing it when it is in front
- [ ] 4.5 Invert the clean-checkout test in `src/ui/orchestrator/tests/surfaces.rs` (the door is drawn); add tests: no door with the review switched off, and a click on the door equals its key

## 5. Keys and scope

- [ ] 5.1 Add `Scope::Review` and `Scope::ReviewComposing`; the actions `review-comment`, `review-add`, `review-cancel`, `review-send`, `review-viewed`, `review-open-in-code`, `review-extend-mark` and `review-resolve`, with defaults in `crates/uze-keys/src/load.rs` (`review-add` on `ctrl+s`)
- [ ] 5.2 List them in the action index and the review's hint bar; prove each hint with a click == key test

## 6. Marking and commenting

- [ ] 6.1 Mark runs of rows by a drag or a shift-click on the number column (the extension's pointer target, never a host text drag) and by `review-extend-mark`; map `selected` to rows and an excerpt
- [ ] 6.2 Draw the `comment` chip under a marked run
- [ ] 6.3 Composer under the marked rows: a typed line in the design system's look, add by key and by control, cancel, and an empty comment not kept
- [ ] 6.4 Composer controls: the code block (the marked rows, or the selection fenced) and inline code (the selection in backticks)
- [ ] 6.5 Draw comments under their rows with the gutter mark: fenced blocks highlighted as the file's language, backticked spans as code, edit and delete on drafts
- [ ] 6.6 Anchor comments by their rows' text; draw an unplaceable comment at the end of its file with its quote and "these lines changed"

## 7. Viewed marks

- [ ] 7.1 A viewed mark per file keyed by its diff's fingerprint; a check on the file row toggled by pointer or `review-viewed`
- [ ] 7.2 `review-viewed` marks and selects the next unviewed file; the list's head counts viewed files
- [ ] 7.3 A mark whose fingerprint no longer matches shows "changed since you viewed it" and is not counted

## 8. The review record

- [ ] 8.1 Read the `persistence-tiers` rule; name `state/projects/<project_id>/reviews/<task_id>.json` and `review_lock_path` in `UzeHome`, and implement `uze_document::Shaped` for the record
- [ ] 8.2 A `uze-application` service reading and writing it under the lock with an atomic replace; the host capabilities over it, off the drawing thread
- [ ] 8.3 Write on every draft, edit, delete, send, resolve and viewed change; read on open; re-read when its modification time moves, in the review's job
- [ ] 8.4 Remove the record with the agent's other records when the agent is forgotten; test that drafts survive `uze agent work name` and a slot move
- [ ] 8.5 Report a record that cannot be read, and never write over it

## 9. Sending, reading and answering

- [ ] 9.1 The send line: count, send number, and the `show`/`reply` commands, under `PROMPT_MAX`, one line, no command lead; return `Outcome::Agent(Prompt)` for the checkout's agent
- [ ] 9.2 On `agent_write(Typed)` mark the drafts `sent` with the send number and agent; on `Refused`/`Declined` leave them as drafts and say why
- [ ] 9.3 `uze agent review show`: launch-key authentication as `uze agent report`; print that agent's sent, unanswered comments in full; exit 0 always
- [ ] 9.4 `uze agent review reply <send>/<n> <text>`: same authentication, only for a comment sent to that agent, answer-only write under the lock, 4 KiB per answer and 8 per comment, control and escape characters stripped; exit 0 always
- [ ] 9.5 Classify both as `Budgeted` in `src/command_performance.rs` (they resolve by project and task id, never by Git); document them in the launch instructions and the official plugin's Skills, never in `uze --help`
- [ ] 9.6 Draw an answer under its comment, stripped again on draw, with `resolve`; resolved comments leave the diff and the door's count
- [ ] 9.7 Tests: a forged escape sequence draws as text; a reply from a shell, to an unknown `<send>/<n>`, or to another agent's comment attaches nothing

## 10. Discard, hand-off and return

- [ ] 10.1 Offer discard on a row only when its file has uncommitted changes
- [ ] 10.2 `review-open-in-code` returns `OpenPath` with the first marked row's new-side line, or the next row that has one
- [ ] 10.3 Keep the review's file, marks, composer text and scroll in `hold`

## 11. Docs, rules and journeys

- [ ] 11.1 Update the `extensions` rule: code is files and map, review owns the changes, and `restore_to_head` has two callers
- [ ] 11.2 Update the `design-system` skill: the review door is always drawn, plus the composer, comment, viewed-check and number-column marking patterns
- [ ] 11.3 Add `review` to the workspace client's surfaces in `docs/architecture/workspace-components.mmd`; run `make artifacts` and `cargo test -p uze-extensions`
- [ ] 11.4 Update `web/content/docs/workspace/code.mdx` and `keys.mdx`; add `review.mdx` and its `meta.json` entry
- [ ] 11.5 Move `journeys/suites/04-workspace/04-an-agents-changes-are-its-own.yml` to the review's door; add a review journey (comment, send, `show`, `reply`, the answer on the comment) with the `journey-author` skill

## 12. Gate

- [ ] 12.1 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace --no-fail-fast`, `uze agent rules check` and `openspec validate --all --strict`
