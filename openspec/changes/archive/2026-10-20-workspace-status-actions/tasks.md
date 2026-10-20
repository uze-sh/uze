## 1. Completion behaviors (one green step: rename, code and every caller together)

- [x] 1.1 Replace `CompletionBehavior::{Handoff, Merge, Pr}` with `{Merge, Push}` (serde `merge`, `push`, no aliases), held as `Option` in the policy; the strict section's error names `merge | push`
- [x] 1.2 Replace the `CompletionBehavior::Handoff` that `refresh` passes to `target_tip` (`landing/deliver.rs:132`) with an explicit `TargetSource::{Local, Remote}`; remove `Delivered::Handoff` and `DeliveryOutcome::Handoff`
- [x] 1.3 Make `publish` push without producing the "open the request" message; remove `open_request_message`
- [x] 1.4 Refuse delivery in `uze-application`'s `deliver_task` and `deliver_ready` (`delivery.rs`) when no behavior is declared, so the key and recent share the refusal; make `DeliveryPolicyView.completion` optional; let `set_completion` unset the key
- [x] 1.5 Render the region clause from `Option<CompletionBehavior>` in `WorktreePolicy::instructions` (D9), so the no-delivery bullet makes no claim that delivery is UZE's; update the policy popup's choices
- [x] 1.6 In the same step, migrate every test and fixture naming `handoff`/`pr` or `CompletionBehavior::{Handoff, Pr}`:
  - `tests/acceptance/engine.rs` and `tests/acceptance/package_and_workspace.rs`
  - `crates/uze-testkit/src/scenario.rs` (`prepare(env, …)`)
  - `tests/project/consumer.rs`
  - `tests/lifecycle/manifest_and_lock.rs`
  - `tests/projection/worktree_policy.rs`
  - `crates/uze-application/src/application/services/artifacts.rs`
  - `crates/uze-workspace/src/{declaration.rs,worktree.rs,landing/tests.rs}`
  - `src/ui/orchestrator/tests.rs` around 2565-2729, which covers `ending(Handoff)` and `completion = Pr`
- [x] 1.7 Rewrite the `docs/architecture/invariants.md` entry citing `handoff_never_touches_the_target` and `pr_publishes_and_leaves_the_request_to_the_agent` to the renamed tests, so `tests/architecture/invariants_citations.rs` holds
- [x] 1.8 Tests: old spellings fail naming the accepted values; a push delivery pushes the gated branch and opens nothing; a merge fast-forwards and pushes nothing; no behavior refuses delivery and writes nothing

## 2. States

- [x] 2.1 Add `WorkStateView::InReview` (a request open and nothing left to push); `delivered` only while the target holds all of the branch; derive with the precedence `delivered > in_review > published > ready`
- [x] 2.2 Run publication and the request lookup whenever the branch is on the remote, regardless of the behavior
- [x] 2.3 Make the chip's subject independent of the behavior (`render/popups.rs` `delivery_subject`): `#<n>` in review, otherwise `→ <target>`
- [x] 2.4 Tests: a new commit after published or in review reads ready; a pushed branch with no request reads published; with a request, in_review; a squash detected reads delivered

## 3. Declaration and read model

- [x] 3.1 Add `ActionState` (`on_change`, `on_publish`, `on_conflict`, `on_fail`); remove `actions` before the strict parse; add `declaration::actions(root)` returning the map and the entries left out
- [x] 3.2 Add `AgentView::action` through `from_agent`'s four call sites, resolved from the final drawn state, `None` for a root agent; add `Evaluation::ignored_actions`, toasted once per primary
- [x] 3.3 Tests: a declared map reads; an unknown key or empty value is left out and named while the rest reads; the policy's own misspelled-key test still holds

## 4. Sending

- [x] 4.1 Remove `conflict_message` and `gate_failure_message`; drop `AgentNotice` from `ReturnedToAgent`, `AwaitingRequest` and `Evaluation::notices`, carrying the resulting state
- [x] 4.2 Add `may_submit(pane) -> Ok | Busy | Typed | Unknown`
- [x] 4.3 On a delivery outcome or refresh that leaves `published`, `delivered`, `conflicted` or `failed`, send that state's instruction through `may_submit` (toast "held back" when refused; nothing when undeclared)
- [x] 4.4 On `WorkspaceHit::StatusAction(tab)`, resolve `pane_for_agent`; when `Ok`, submit through `notice_bytes` / `submit_notice` (made `pub(super)`) and call `note_agent_prompt_submission`; otherwise toast the reason; never queue, never record in prompt history
- [x] 4.5 Reword the delivery outcome toasts to state words within the toast bounds
- [x] 4.6 Tests: each operation sends its state's instruction once when declared and nothing when not; observed states send nothing on their own; a held-back send goes on the next click; a second click is refused as busy; typed and untrusted refusals leave the buffer as it was

## 5. Chip

- [x] 5.1 Remove `ChipState::Static` and `chip_state`'s `None => Static`; add `StatusChip::{Report, Action { hovered, pressed }}`; add the missing muted tokens to `uze-theme`, `theme.schema.json` and every bundled theme
- [x] 5.2 Draw the chip as `StatusChip` from the state and `AgentView::action`; register `StatusAction` only with an action; remove `WorkspaceHit::Deliver` and its arm in `session/mouse.rs`; extend the status legend with in review
- [x] 5.3 Put the hovered instruction in the message zone (30 columns, `…`, dropped without room); a `ready` chip without one names `chord_for(Action::DeliverTask)` there on hover
- [x] 5.4 Rewrite the widget tests (`widget/tests.rs:258-275`) and the orchestrator tests that click or assert the chip as a button (`tests.rs` around 2433, 2508, 2688, 2752, 3053, 3520, 4528, 4551); add `TestBackend` tests for the hint bound, the narrow-strip drop and no control moving

## 6. Journeys, docs and gates

- [x] 6.1 Move every journey world off `handoff`/`pr`: 03-context, 04-workspace, 05-delivery and 06-recovery to `merge`, `push` or no `delivery:` as each scene needs; the `07-upgrade` worlds to `merge` or no key, since the released binary rejects `push`; the `05-delivery` chip clicks to the `deliver-task` key; declare the instructions each world's checks expect in the agent's pane
- [x] 6.2 Cover the instruction in the `05-delivery` journeys: a click on the status (`on_change`) and the automatic sends (`on_publish` after a push, `on_fail`, `on_conflict`), each gated on the instruction reaching the real agent pane and checked against Git and the task store (see decisions D6)
- [x] 6.3 Rewrite the doc comments that describe the chip as a button or `pr` as pushing: `chip.rs:1-7`, `tab_strip.rs:290-296`, `popups.rs:716-728`, `views.rs:125`, `views.rs:344`, `crates/uze-workspace/src/landing/request.rs`
- [x] 6.4 Update the shipped and published text for `delivery: merge | push` and `workspace.actions`, including the instruction set that reproduces today's `pr` flow:
  - `plugins/uze/skills/worktree/SKILL.md` ("three things come back to you", the `pr` request flow)
  - `web/content/docs/reference/project-files.mdx`
  - `web/content/docs/workspace/agents.mdx`
  - `docs/capabilities/uze-skill.md`
  - `docs/architecture/agent-lifecycle.mmd` (its `handoff` edge), followed by `make artifacts` and `cargo test -p uze-extensions`
- [x] 6.5 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, `openspec validate --all --strict` and `journey validate`
- [x] 6.6 Last: move this repository's `agents.yaml` to `delivery: push` and declare `on_publish`, `on_conflict` and `on_fail` (the installed release rejects `push`, so this lands with the release that ships the change)

## 7. Four keys, after trying it by hand

- [x] 7.1 Narrow `ActionState` to `on_change`, `on_publish`, `on_conflict`, `on_fail`; `WorkStateView::answered_by` maps `Uncommitted` to `on_change` and gives `Ready`, `InReview` and `Integrated` none; drop `Merged` from the automatic sends
- [x] 7.2 Add `ActionState::fallback` for `on_conflict` and `on_fail`, and `declaration::instructions(root)` (declared over fallbacks), read by `instructions_of`
- [x] 7.3 Draw an `uncommitted` status beside the change count only when `on_change` is declared; the count keeps opening the changes
- [x] 7.4 Tests, the reference page, journey `05-delivery/01` (the click moves to `on_change`), and this repository's `agents.yaml`
- [x] 7.5 Name the hovered action by its key (`send <key>`) instead of the instruction's start
- [x] 7.6 A click on ready work delivers it (the deliver key's action), drawn as an action with the `deliver <chord>` hint; any other state never delivers on a click
- [x] 7.7 A report chip says on hover what its state means (`in review · request open`, `delivered to <target>`)

## 8. Status latency, after trying it by hand

- [x] 8.1 `landing::refresh` takes the repository write lock without waiting (`Duration::ZERO`): the evaluation pass holds the tasks document, and waiting there held a delivery's outcome behind a slow `pre-push` until the lock's 60 s timeout
- [x] 8.2 Split the evaluation into `read_tasks` (no network) and `ask_about_requests` (the `git ls-remote`, carried as `RequestQuestion`); the client draws the first answer and replaces the views with the second
- [x] 8.3 Poll a per-checkout `change_stamp` (`HEAD`, index, rebase state, the branch's local and `origin` refs; `stat` only) every second and evaluate a repository whose stamp moved
- [x] 8.4 A click on the target's `↓N` pulls it (`landing::pull_target`: fetch and fast-forward, through Git's merge where the operator stands, refused over uncommitted changes in the way); `↓N · +a -d` separated by a dot; in review drawn as `#N` alone
- [x] 8.4 A click on the target's `↓N` pulls it (`landing::pull_target`: fetch and fast-forward, through Git's merge where the operator stands, refused over uncommitted changes in the way); `↓N · +a -d` separated by a dot; the hover hint ends in a dot, not a hairline; no hairline against the surfaces' group; in review drawn as `#N` alone
- [x] 8.5 Cap every toast at a 24-column title and a 32-column detail, shorten the messages over it (refusal reasons, action warnings, target concern, recovered record), and guard the literals with `every_toasts_literal_words_fit_the_toast`
