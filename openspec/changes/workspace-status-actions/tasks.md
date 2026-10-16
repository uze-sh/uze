## 1. Completion behaviors

- [ ] 1.1 Replace `CompletionBehavior::{Handoff, Merge, Pr}` with `{Merge, Push}` (serde `merge`, `push`, no aliases), held as `Option` in the policy; the strict section's error names `merge | push`
- [ ] 1.2 Make `publish` push without producing the "open the request" message; remove `open_request_message`
- [ ] 1.3 With no behavior declared, the deliver key and recent's deliver refuse with a toast naming the missing `delivery:`
- [ ] 1.4 Rewrite `instruction_clause` for `push`, `merge` and none (D10) and update this repository's `AGENTS.md` region
- [ ] 1.5 Tests: old spellings fail naming the accepted values; a push delivery pushes the gated branch and opens nothing; a merge fast-forwards and pushes nothing; no behavior refuses delivery and writes nothing

## 2. States

- [ ] 2.1 Add `WorkStateView::InReview`; derive with the precedence `delivered > in_review > published > ready`
- [ ] 2.2 Run publication and the request lookup whenever the branch is on the remote, regardless of the behavior
- [ ] 2.3 Make the chip's subject independent of the behavior: `#<n>` in review, otherwise `→ <target>`
- [ ] 2.4 Tests: a new commit after published reads ready; a pushed branch with no request reads published; with a request reads in_review; a squash detected reads delivered

## 3. Declaration and read model

- [ ] 3.1 Add `ActionState` (`on_ready`, `on_publish`, `on_review`, `on_deliver`, `on_conflict`, `on_fail`); remove `actions` before the strict parse; add `declaration::actions(root)` returning the map and the entries left out
- [ ] 3.2 Add `AgentView::action` through `from_agent`'s four call sites, resolved from the final drawn state, `None` for a root agent; add `Evaluation::ignored_actions`, toasted once per primary
- [ ] 3.3 Tests: a declared map reads; an unknown key or empty value is left out and named while the rest reads; the policy's own misspelled-key test still holds

## 4. Sending

- [ ] 4.1 Remove `conflict_message` and `gate_failure_message`; drop `AgentNotice` from `ReturnedToAgent`, `AwaitingRequest` and `Evaluation::notices`, carrying the resulting state
- [ ] 4.2 Add `may_submit(pane) -> Ok | Busy | Typed | Unknown`
- [ ] 4.3 On a delivery outcome or refresh that leaves `published`, `delivered`, `conflicted` or `failed`, send that state's instruction through `may_submit` (toast "held back" when refused; nothing when undeclared)
- [ ] 4.4 On `WorkspaceHit::StatusAction(tab)`, resolve `pane_for_agent`, submit through `notice_bytes` / `submit_notice` (made `pub(super)`) when `Ok` and call `note_agent_prompt_submission`; otherwise toast the reason; never queue or record in prompt history
- [ ] 4.5 Reword the delivery outcome toasts to state words within the toast bounds
- [ ] 4.6 Tests: each operation sends its state's instruction once when declared and nothing when not; observed states send nothing on their own; a held-back send goes on the next click; a second click is refused as busy; typed and untrusted refusals leave the buffer as it was

## 5. Chip

- [ ] 5.1 Remove `ChipState::Static` and `chip_state`'s `None => Static`; add `StatusChip::{Report, Action { hovered, pressed }}`; add the missing muted tokens to `uze-theme`, `theme.schema.json` and every bundled theme
- [ ] 5.2 Draw the chip as `StatusChip` from the state and `AgentView::action`; register `StatusAction` only with an action; remove `WorkspaceHit::Deliver` and its arm in `session/mouse.rs`; extend the status legend with in review
- [ ] 5.3 Put the hovered instruction in the message zone (30 columns, `…`, dropped without room); a `ready` chip without one shows `chord_for(Action::DeliverTask)`
- [ ] 5.4 Rewrite the widget tests (`widget/tests.rs:258-275`) and the orchestrator tests that click or assert the chip as a button (`tests.rs` around 2433, 2508, 2688, 2752, 3053, 3520, 4528, 4551); add `TestBackend` tests for the hint bound, the narrow-strip drop and no control moving

## 6. Journeys, docs and gates

- [ ] 6.1 Move the `05-delivery` journeys to the `deliver-task` key; `pr` worlds move to `push` and `handoff` worlds to `merge` or no delivery, declaring the instructions their checks expect in the agent's pane
- [ ] 6.2 Rewrite the doc comments that describe the chip as a button or `pr` as pushing (`chip.rs:1-7`, `tab_strip.rs:290-296`, `popups.rs:716-728`, `views.rs:124`, `views.rs:344`, `request.rs`)
- [ ] 6.3 Document `delivery: merge | push` and `workspace.actions`, with the instruction set that reproduces today's `pr` flow; move this repository's `agents.yaml` to `push` and declare `on_publish`, `on_conflict` and `on_fail`
- [ ] 6.4 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, `openspec validate --all --strict` and `journey validate`
