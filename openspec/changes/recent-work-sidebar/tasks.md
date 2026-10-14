## 1. Domain

- [x] 1.1 Add `forget_task(primary, task)` to the tasks service in `uze-application`: removes the agent's record and conversations under the document lock, runs no Git, refuses when the project directory exists; tests for both paths
- [x] 1.2 Expose on `PreservedWork` (or its read model) whether the project directory still exists, answered from the filesystem without Git
- [x] 1.3 Verify that `Published` work whose patch the target carries (squash, rebase) reaches `Integrated` on evaluation and so leaves preserved work; add the missing transition and a test if it does not

## 2. Keymap

- [x] 2.1 In `uze-keys`, rename the toggle to `ToggleRecent` and the `Work` scope to `Recent`, keeping their persisted names and `alt+p`; remove the modal's own actions
- [x] 2.2 Make a keymap file naming a removed action load with the binding reported and ignored; test with a file naming each removed action

## 3. Recent model and reads

- [x] 3.1 Create `src/ui/orchestrator/recent.rs`: rows from preserved work plus joinable subagent checkouts, the two blocks, recency ordering (checkout `last_changed`, else `created_at_unix`), the state-to-reason projection from design.md, selection kept by row identity across leaving and re-entering
- [x] 3.2 Sweep preserved work when recent opens and after anything recent does to it
- [x] 3.3 On opening recent, spawn the checkouts read for every project with a listed row (`spawn_*` in `orchestrator/reads.rs`, `absorb_*` beside it), each answer keyed by project and dropped when stale
- [x] 3.4 Unit-test the projection: every `WorkStateView` lands in its block with its reason and its enter action; a missing project offers only forget

## 4. Recent rendering

- [x] 4.1 Add `render/recent.rs`: one line per unselected row (name, age), the selected row expanded (reason in its token colour plus project or `· new space`, then branch · N ahead · disk), struck-through name for a missing project, a blank line between blocks, all chrome from `src/ui/widget/` and colour from `theme`
- [x] 4.2 Sidebar header: its name opens and closes recent, answering the pointer like the header's other controls, and reads `work › recent` while recent is shown
- [x] 4.3 Footer hint shows only the keys the selected row offers, plus `esc close`
- [x] 4.4 `TestBackend` tests: unselected rows are one line with no glyph; selected row content per state; header count and mark; strike-through

## 5. Actions

- [x] 5.1 Enter resumes (closes recent, selects the resumed agent in the tree, opening a space when needed) or joins for a joinable subagent (stays in recent, toast naming the agent joined into)
- [x] 5.2 `i` delivers ready work only; `d` asks on the row (`discard branch, commits and conversation?  y / n`), `y` discards, `n` or escape declines; `d` on a missing project asks to forget and calls `forget_task`
- [x] 5.3 Tests for each action, including that discard and forget do nothing unconfirmed and that resume from another project leaves the viewed space without a tab

## 6. Remove the work modal

- [x] 6.1 Delete `work.rs`, `work_list.rs`, `checkouts.rs` and `session/work_modal.rs` and their tests, moving the helpers recent uses (`bytes_to_say`, `age_to_say`, `join_of`) into `recent.rs`
- [x] 6.2 Remove the modal's mouse and key routing, its toggle from the action index and status catalog, and any now-unused `uze-application` read-model methods only it called
- [x] 6.3 Port the modal's still-valid tests (cross-project listing, resume into own project's space, two projects one branch name) to recent

## 7. Docs and journeys

- [x] 7.1 Update `docs/architecture/invariants.md`: the checkouts-view entries become recent's, each tied to its new test
- [x] 7.2 Update `journeys/suites/06-recovery/07-work-the-closing-did-not-take.yml` to find the work through recent, and the workspace docs page it proves (`web/content/docs/workspace/agents.mdx`)
- [x] 7.3 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace --no-fail-fast` and `openspec validate recent-work-sidebar --strict`

## 8. Conversations and names

- [x] 8.1 List an agent that ended with nothing to deliver while its last conversation began within 30 days (`PreservedWork::last_active_unix`, `CONVERSATION_KEPT_SECONDS`), and order recent by last activity
- [x] 8.2 Resume such an agent on its branch cut again from the target's reach (`checkout::resume`'s `fresh` start)
- [x] 8.3 Record a name typed over an agent's tab on the agent (`label_agent`, `spawn_agent_label`), and keep it when the first commit names the branch
- [x] 8.4 Resume with the harness the agent last ran with (`PreservedWork::harness`, `ResumeTarget::harness`), asking only when it is no longer set up
- [x] 8.5 Scroll recent with the navigator's `NavigatorScroll`: `widget::Scrollbar` on the sidebar's divider, its handle driven by `EdgeDrag` (along scrolls, across resizes), the wheel scrolls, the keys pull the window to the selection
