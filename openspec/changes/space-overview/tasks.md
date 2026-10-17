## 0. Preconditions

- [x] 0.1 `portable-agent-memory` (PR #268) is merged, so `StatusReport.memory` exists on the base

## 1. Naming and status in one place

- [x] 1.1 Rename the Overview read model's `MemoryState` to `ContextState` and its field `memory` to `context`; update the management view's label and `src/ui/tests.rs`
- [x] 1.2 Move `ProjectStatus` composition from `src/main.rs`/`src/cli/status.rs` into `uze-application` (`health().project_status(root)`); `uze status` text output unchanged on a fixture
- [x] 1.3 Per-harness agent context and context-unread reason on the status's harnesses (keyed by integration id), plugins contributing here by name; the CLI renders the same lines from them
- [x] 1.4 `uze status` summarizes memory (one line per scope, count by provenance; proposals and unreadable files still named) per `portable-agent-memory`'s revised "The operator reads memory through status"; the facts stay in `--json`

## 2. Read model (uze-application)

- [x] 2.1 `SpaceOverview` in `services/`: moved status, plugins (lock declared + installed + `differs_from_lock`, contributing, commit and freshness from `plugins().list()`), memory (project entries, machine count, unreadable), preserved work; "not a project" and "root gone" answered as such
- [x] 2.2 A test that the overview's Context and Plugins states and `uze status` agree for one fixture project

## 3. Workspace declaration (uze-core, uze-workspace, uze-application)

- [x] 3.1 `ManifestDocument`: a public writer for any `workspace:` key (set any value, remove); `set_completion` uses it
- [x] 3.2 `uze-workspace`: validate a candidate `workspace:` text against a root, collecting every problem keyed by key (parse, slots, misplaced links, unignored links, unknown keys); `declared` uses it
- [x] 3.3 `Workspace::declaration_view(root)`: every key, value or default (marked), `actions` names, editable or the refusal reason
- [x] 3.4 `Workspace::preview_declaration(root, edits)`: candidate text in memory, problems (artifact rule included), the diff lines, and which commands would wait for approval again
- [x] 3.5 `Workspace::save_declaration(root, edits, expected)`: under `MutationLock`, refuses a file changed since the preview, saves atomically; creates `agents.yaml` when absent
- [x] 3.6 Tests: comments kept, every problem reported, changed-file refusal, anchors read-only, new-file creation

## 4. Widgets (src/ui/widget)

- [x] 4.1 Move the manage header's tabs into `widget/` as a generic tab bar (label, optional mark, active index, shrink to number + mark); manage renders through it unchanged
- [x] 4.2 Move the drawer's core into `widget/` (width and drag state passed in); manage's `drawer` calls it
- [ ] 4.3 `Choice` (an inline enum, options shown on focus), a list editor row set, and a YAML diff view, each only as the Workspace section needs it

## 5. Surface (src/ui)

- [x] 5.1 `WorkspaceModel.space_overview` keyed by `SpaceId`; `orchestrator/space_overview.rs` drawn in the pane's rect from `render.rs`'s no-extension arm: header with activity, section bar, cause line, the four sections, drawer, footer; loading, "not a project" and "root gone"
- [ ] 5.2 `spawn_space_overview` and its `absorb_*`: on open, refresh, save and a stamp change of `agents.yaml`/`agents.lock`/`AGENTS.md`; one in flight per space; walk coalesced; stale answers dropped
- [x] 5.3 `land_on_space`/`step_space` open the overview; put away by a tab, an agent row, `+`, another space or Esc; never by `close_extension_left_behind`
- [x] 5.4 An `overview` tab leading the strip (`WorkspaceHit::OpenOverview`), selected while shown
- [x] 5.5 Actions: Open shell; `review_commands_for(project)`; a fact's body through `memory().show` on the read thread
- [x] 5.6 Workspace editing: pending edits, preview on the read thread, problems at their key, diff with Save/Discard, save on the read thread then re-read
- [x] 5.7 `Scope::SpaceOverview` and `Scope::SpaceOverviewEditing` in `uze-keys`, `Action::OpenSpaceOverview` in `Scope::Workspace`, default bindings
- [ ] 5.8 `TestBackend` tests: a row click draws the overview and creates no tab; the confirming broadcast keeps it; a double click renames; a tab selection puts it away; Esc returns to the selected tab; Open shell on an agent-only space; section switch by number; a loading section; a stale answer dropped; a gone root; Review by keyboard; an invalid edit blocks Save; the diff before save

## 6. Journey

- [ ] 6.1 A journey in `04-workspace`: selecting the space shows its overview and the process table gains no shell; Open shell from an agent-only space starts one at the root; a workspace edit saved from the overview changes `agents.yaml` on disk with its comments kept

## 7. Docs and gate

- [ ] 7.1 The workspace docs page (`web/content/docs/workspace/`) describes the overview and the Workspace section, and the journey's `proves:` names it
- [ ] 7.2 `make check` green
