## 1. Contract

- [ ] 1.1 Add `Extension::door_label(&self, checkout: &Checkout) -> Option<DoorLabel>` with a default of `None`, and `DoorLabel` as styled spans in `contract.rs`
- [ ] 1.2 Add `line: Option<u32>` to `Entry::Path` and `Outcome::OpenPath`, and pass it through `src/ui/orchestrator/declared.rs` and the pane-link and architect callers (`None` where they have none)
- [ ] 1.3 Add `Host::review_record` and `Host::write_review_record` with defaults that answer nothing, and test that a declared extension's host refuses both

## 2. Split the changes half out of code

- [ ] 2.1 Create `crates/uze-extensions/src/review.rs` and `review/extension.rs` with an empty surface, register it in `ExtensionRegistry::builtin`, and give it the `changes` door
- [ ] 2.2 Move `code/changes.rs`, `code/diff.rs` and the changed-file actions of `code/row_menu.rs` (open, copy paths, discard) to `review/`, keeping their tests green
- [ ] 2.3 Remove `ContentMode::Diff` and `NavigatorMode::Changes` from `code.rs`, and the branches that handled them
- [ ] 2.4 Make `code` honour `Entry::Path { line }`: contents in view at that line, caret on it
- [ ] 2.5 Rewrite `code.rs`'s module doc ("Why one extension") for files, history and map, and write `review.rs`'s doc saying what it owns and why it stands apart
- [ ] 2.6 Move the image-in-changes tests from code to review (`A changed image is drawn in the review`)

## 3. Diff against the target

- [ ] 3.1 Read the merge base of `Checkout.target` and `HEAD` as a job, cached per (target tip, HEAD)
- [ ] 3.2 List files and read a file's diff against the merge base, working tree and untracked files included; keep the `HEAD` read for the `uncommitted` filter
- [ ] 3.3 With no evaluated target, fall back to `HEAD` and say the target is unknown
- [ ] 3.4 Draw `all · uncommitted` on the surface's mode row and switch the reads with it
- [ ] 3.5 Test that an answer arriving after the selection moved is dropped

## 4. The door

- [ ] 4.1 Answer `door_label` from the review: `+a -d`, `◆n` for drafts, `◆n` in the success hue for unresolved answers, a working mark while a send is out, and `review` when nothing differs
- [ ] 4.2 Draw the door from `door_label` in `render/tab_strip.rs` in place of the `GitBadge` summary, always in an agent's context, held back at rest and lit under the pointer
- [ ] 4.3 Route `WorkspaceHit::OpenChanges` and `Action::ToggleChanges` to the review view, closing it when it is in front
- [ ] 4.4 Prove it with tests: the door is drawn on a clean checkout, and a click on it equals its key

## 5. Marking and commenting

- [ ] 5.1 Track a marked run of rows: drag and shift-click on line numbers, `shift+↑↓`, and a text selection mapped to its rows and excerpt
- [ ] 5.2 Draw the `c comment` affordance under a marked run
- [ ] 5.3 Composer under the marked rows: a typed line in the design system's look, `ctrl+enter` adds, `esc` drops, and an empty comment is not kept
- [ ] 5.4 Composer controls: a code block (the marked rows, or the selection fenced) and inline code (the selection in backticks)
- [ ] 5.5 Draw comments under their rows with the gutter mark: fenced blocks highlighted as the file's language, backticked spans as code, edit and delete on drafts
- [ ] 5.6 Anchor comments by their rows' text, and draw an unplaceable comment at the end of its file with its quote and "these lines changed"

## 6. Viewed marks

- [ ] 6.1 A viewed mark per file, keyed by the fingerprint of the diff it was given on; a check on the file row, toggled by pointer or `v`
- [ ] 6.2 `v` marks and moves to the next unviewed file; the list's head counts viewed files
- [ ] 6.3 A mark whose fingerprint no longer matches shows "changed since you viewed it" and is not counted

## 7. The review record

- [ ] 7.1 Name `state/review/<repository-id>/<branch>.json` in `UzeHome`, as a record implementing `uze_document::Shaped` (read the `persistence-tiers` rule first)
- [ ] 7.2 Implement the host capabilities in `src/ui/extension_host.rs` over a `uze-application` service, off the drawing thread
- [ ] 7.3 Write the record on every draft, edit, delete, send, answer, resolve and viewed change, and read it when the review opens
- [ ] 7.4 Prune records whose branch no longer exists on the existing evaluation pass
- [ ] 7.5 Report a record that cannot be read, and never write over it

## 8. Sending and answers

- [ ] 8.1 Build the prompt from the drafts (numbered, path and lines, at most six quoted rows, the text, and how to reply with `uze agent review reply <n> <text>`)
- [ ] 8.2 `s` returns `Outcome::Agent(Prompt)` for the checkout's agent; drafts become `sent` only once the bridge reports the prompt typed
- [ ] 8.3 `uze agent review reply <n> <text>`: launch-key authentication as `uze agent report` does it, attach to comment n of that agent's repository and branch, exit 0 always; classify it in `src/command_performance.rs`
- [ ] 8.4 Document the verb in the launch instructions and the official plugin's Skills, never in `uze --help`
- [ ] 8.5 Draw an answer under its comment with `resolve`, and take resolved comments out of the diff and the door's count

## 9. Hand-off and return

- [ ] 9.1 `o` returns `Outcome::OpenPath` with the first marked row's new-side line, or the next row that has one
- [ ] 9.2 Keep the review's file, marks, composer text and scroll in `hold`, so an agent's return shows it as it was left

## 10. Docs, rules and journeys

- [ ] 10.1 Update the `extensions` rule (code is files, history and map; review is the changes; `restore_to_head` has two callers)
- [ ] 10.2 Update the `design-system` skill: the review door is always drawn, and the composer, comment and viewed patterns
- [ ] 10.3 Add `review` to the workspace client's surfaces in `docs/architecture/workspace-components.mmd`; run `make artifacts` and `cargo test -p uze-extensions`
- [ ] 10.4 Move `journeys/suites/04-workspace/04-an-agents-changes-are-its-own.yml` to open the review's door, and add a review journey: comment, send, reply through the CLI, the answer on the comment (`journey-author` skill)
- [ ] 10.5 Update the workspace docs page the changes journey `proves:`, `web/content/docs/workspace/agents.mdx`

## 11. Gate

- [ ] 11.1 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace --no-fail-fast`, `uze agent rules check` and `openspec validate --all --strict`
