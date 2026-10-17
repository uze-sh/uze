## 1. Port and launch plan (uze-core, uze-workspace, shim)

- [x] 1.1 Add `first_turn_args(&str) -> Vec<OsString>` to the workspace half of `IntegrationPort`, defaulting to none
- [x] 1.2 Add `trailing: Vec<OsString>` to `LaunchPlan`; `src/shim.rs` appends it after the contributed arguments
- [x] 1.3 Add the optional pending-briefing field (target integration, path) to `ConversationRecord`, additive, with a test that a record without it still reads
- [x] 1.4 `continuity::plan` gives the pending briefing only for the matching integration and clears it in the same `conversation::update` that records the launch; tests: given once, a restore carries nothing, another harness leaves it pending, an invocation with arguments neither gives nor clears it
- [x] 1.5 Add `UzeHome::briefing_path(project_id, agent)` in the generated tier; `conversation::forget` removes the briefing; the layering path rule stays green

## 2. Switching (uze-application, uze-workspace)

- [x] 2.1 Change `Agent.harness`'s doc comment to "the harness of the latest launch"; `Resume` placement records the harness it launches (fixes the existing drift), with a test
- [x] 2.2 `Workspace::switch_harness(primary, agent, from, to)`: compare-and-set of `Agent.harness` under `task::locked`, answers the agent's current directory with a fresh key, refuses an agent with no checkout, and reports a superseded switch
- [x] 2.3 Briefing module in `uze-workspace`: render the Git facts (via `uze-git`) and an optional digest into at most 12 KB with the cut order of design D6; written owner-only; tests for the bound, the cut order and the Git-only fallback
- [x] 2.4 `switch_harness` writes the briefing and records the pending entry before answering; the "since you left" delta when the target already has an entry (D7)

## 3. Workspace client (src/ui, uze-keys)

- [x] 3.1 `Action::SwitchHarness` in `uze-keys`; one "Continue in…" entry opening a submenu of the set-up harnesses other than the current one, offered only where `agent_for_tab` resolves; `Alt+.` (`Action::OpenMenu`) opens the selected agent's menu, Right/Left open and close a group
- [x] 3.2 `PlacementRequest::Switch` answered in `spawn_agent_placement` with `replacing: Some(tab)`; busy notice "<label>: continuing in <harness>"; the `continuity_gap` toast for a target without continuity
- [x] 3.3 Inline confirmation when the tab is `Working`; no confirmation when idle
- [x] 3.5 `TestBackend` tests: the group's harnesses, current harness excluded, exited agent offers none, keyboard and pointer paths into the group, confirmation on a working tab

## 4. Integrations: first turn (uze-integrations)

- [x] 4.1 `first_turn_args` for Claude (positional), Codex (positional after `resume <id>`), OpenCode (`--prompt`), Antigravity (`-i`), each unit-tested against the argument order with start and with resume
- [x] 4.2 Lab contract check (`contract/briefing.py`, vendor-neutral, beside continuity): a launch carrying a first turn with and without resume is accepted by the real binary and the turn reaches the synthetic provider

At this point the switch is usable with a Git-only briefing.

## 5. Transcript digest (uze-core, uze-integrations)

- [x] 5.1 Add `conversation_digest(session, cwd) -> Option<Digest>` with neutral `ConversationDigest { goal, summary, turns, last_command }`, defaulting to `None`; the core names no vendor
- [x] 5.2 Claude digest: string-content `user` turns, `text` assistant blocks, `isCompactSummary` summary, skip `tool_result`, `isSidechain`, `isMeta` and API error records; tested on a recorded, scrubbed fixture
- [x] 5.3 Codex digest: `item_completed` `UserMessage`/`AgentMessage`/`CommandExecution`, top-level `compacted` summary, never `response_item` messages; tested on a recorded, scrubbed fixture
- [x] 5.4 Wire the digest into the briefing (2.3) and the delta into its time floor (2.4)
- [x] 5.6 Lab: record each harness's real transcript (`experiments/<vendor>/transcript`) into `tests/_fixtures/foreign/transcripts/`, replayed through its digest by the deterministic suite

## 6. Diagrams, docs, proof

- [x] 6.1 Add the switch edge to `docs/architecture/agent-lifecycle.mmd` (and `workspace-components.mmd` if it names the launch plan); run `make artifacts` and `cargo test -p uze-extensions`
- [x] 6.2 Document the switch in the user-facing workspace docs page it belongs on
- [x] 6.3 Journey (`04-workspace`, stand-in harnesses echoing their argv): switch from one stand-in to another from the menu; check the recorded harness, the briefing outside the checkout with mode 0600, the first turn last in the incoming argv, a restore with no first turn, and a switch back resuming the first conversation; `proves:` names the docs page from 6.2
- [x] 6.4 `make check` green; `openspec validate switch-agent-harness --strict`
