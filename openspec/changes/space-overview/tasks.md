## 0. Preconditions

- [ ] 0.1 `portable-agent-memory` (PR #268) is merged, so `StatusReport.memory` and the proposal dialog exist on the base

## 1. Naming and status in one place

- [ ] 1.1 Rename the Overview read model's `MemoryState` to `ContextState` and its field `memory` to `context`; update the management view's label and `src/ui/tests.rs`
- [ ] 1.2 Move `ProjectStatus` (`StatusReport` + `steps_not_spelled_here` + `commands_awaiting_approval`) from `src/cli/status.rs` into `uze-application`; `uze status` renders the moved model unchanged (its output and `--json` byte-identical on a fixture)

## 2. Read model (uze-application)

- [ ] 2.1 `SpaceOverview` in `services/`: the moved project status, plugins (lock declared + installed, context inspection `contributions`, snapshot freshness), memory from `StatusReport.memory`, worktree policy and artifact places with defaults marked, preserved work for the project; "not a project" and "root gone" answered as such
- [ ] 2.2 A test that the overview's readiness and `uze status` report the same issues for one fixture project

## 3. Surface (src/ui)

- [ ] 3.1 `WorkspaceModel.space_overview` keyed by `SpaceId`, and `orchestrator/space_overview.rs`: sections drawn from the `widget/` vocabulary in the pane's rect, outside `open_extension`; loading and "not a project" states
- [ ] 3.2 `spawn_space_overview` in `reads.rs` and its `absorb_*`: read on open, on refresh and on a stamp change of `agents.yaml`/`agents.lock`/`AGENTS.md`; one in flight per space; opens during a `step_space` walk coalesced; stale answers dropped
- [ ] 3.3 `land_on_space` and `step_space` open the overview; it is put away by a tab, an agent row, `+`, another space or Esc (to the space's selected tab), never by `close_extension_left_behind`
- [ ] 3.4 The tab strip shows the space's own context while the overview is up, with an `overview` button lit in the trailing group that reopens it
- [ ] 3.5 Actions: "Open shell" (focus `space_own_tab` or `open_space_shell`); `review_commands_for(project)` from `remembered.commands_awaiting`; `review_proposal_named(name)`; a fact's body opened in place through `memory().show` on the read thread
- [ ] 3.6 Activity counts from `remembered`'s task views plus the preserved work read
- [ ] 3.7 `Scope` for the overview in `uze-keys` (before `Scope::Surfaces`), `Action::OpenSpaceOverview` in `Scope::Workspace`, adapted in `src/ui/keys.rs`
- [ ] 3.8 `TestBackend` tests: a row click draws the overview and creates no tab; the confirming broadcast does not put it away; a double click renames without putting it away; a tab selection puts it away; Esc returns to the selected tab; "Open shell" on an agent-only space creates and focuses a shell; a loading section; a stale answer dropped; a gone root; keyboard reaches a proposal

## 4. Journey

- [ ] 4.1 A journey in `04-workspace`: selecting the space shows its overview and the process table gains no shell; "Open shell" from an agent-only space starts one at the root; approving commands from the overview records the approval on disk

## 5. Docs and gate

- [ ] 5.1 The workspace docs page (`web/content/docs/workspace/`) describes the overview, and the journey's `proves:` names it
- [ ] 5.2 `make check` green
