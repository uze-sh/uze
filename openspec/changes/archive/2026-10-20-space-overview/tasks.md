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
- [x] 3.4 `Workspace::apply_declaration(root, edits)`: under `MutationLock`, applies the edits to the file as it is now, checks the result (artifact rule included), and either saves it atomically (creating `agents.yaml` when absent) or refuses with every problem; says whether setup or gate wait for approval again, and never approves them
- [x] 3.5 Remove the preview, its diff and the changed-since check: an edit names a key and lands on the current file
- [x] 3.6 Tests: comments kept, every problem reported, changed-file refusal, anchors read-only, new-file creation

## 4. Widgets (src/ui/widget)

- [x] 4.1 Move the manage header's tabs into `widget/` as a generic tab bar (label, optional mark, active index, shrink to number + mark); manage renders through it unchanged
- [x] 4.2 Move the drawer's core into `widget/` (width and drag state passed in); manage's `drawer` calls it
- [x] 4.3 `Choice` (an inline enum, options shown on focus) and a list editor row set, each only as the Settings section needs it

## 5. Surface (src/ui)

- [x] 5.1 `WorkspaceModel.space_overview` keyed by `SpaceId`; `orchestrator/space_overview.rs` drawn in the pane's rect from `render.rs`'s no-extension arm: header with activity, section bar, cause line, the five sections (Agent first), drawer, footer; loading, "not a project" and "root gone"
- [x] 5.2 `spawn_space_overview` and its `absorb_*`: on open, refresh, a written change and a stamp change of `agents.yaml`/`agents.lock`/`AGENTS.md`; one in flight per space; walk coalesced; stale answers dropped
- [x] 5.3 `land_on_space`/`step_space` open the overview; put away by a tab, an agent row, `+`, another space or Esc; never by `close_extension_left_behind`
- [x] 5.4 An `overview` tab leading the strip (`WorkspaceHit::OpenOverview`), selected while shown
- [x] 5.5 Actions: `review_commands_for(project)`; a fact's body through `memory().show` on the read thread
- [x] 5.6 Settings editing saves each committed change on the read thread, one save out at a time and edits made meanwhile sent after it; a written answer drops the edits it carried, re-reads and raises a toast; a refused one keeps them marked with each problem at its key, and `x` drops them
- [x] 5.7 `Scope::SpaceOverview` and `Scope::SpaceOverviewEditing` in `uze-keys`, `Action::OpenSpaceOverview` in `Scope::Workspace`, default bindings
- [x] 5.8 `TestBackend` tests: the agent button opens it only in an agent's context; the confirming broadcast keeps it; selecting another agent turns it; selecting the space's row puts it away and lands on its shell; Enter copies a prompt; section switch by number; a loading section; a stale answer dropped; an agent whose directory is gone; Review by keyboard; an edit written with no Save; a refused edit kept with its problem and discarded; edits made while a save is out written after it

- [x] 5.9 A Worktrees section: the catalog's grid moved to `widget::grid`; cards from `Workspace::checkouts` read by `spawn_space_worktrees` while the section is in front and on refresh, stale answers dropped; state, owner and live tab per card; drawer; `SelectSection(6)`
- [x] 5.10 `TestBackend` tests: a card per worktree with branch and size; keys and a click select; the wheel scrolls whole cards without selecting; hover lifts a card; the measure is off the frame and a stale answer dropped; the empty state; the warning
- [x] 5.11 Removing a free worktree (D11): `checkout::remove_free_slot` judges the slot again under the task store's and the repository's locks and refuses with a reason; `Workspace::remove_free_checkout` empties the trash after; `RemoveItem` and a drawer button ask with a dialog, `spawn_worktree_removal` runs it off the frame, the header says it is underway, a toast says what happened and the section is read again
- [x] 5.12 Tests: real repositories for a free slot removed with its branch kept, and a held, entered, written-in or foreign one refused and left intact; `TestBackend` for the offer only on a free card, esc withdrawing the question, one removal sent off the frame, a refusal toasted with the card kept, a success read again
- [x] 5.13 A search over the Memory section (`/`, `Scope::SpaceOverviewFilter`): narrows by name, description, kind or origin, the cause line counts what it let through, enter reads the fact, esc takes the words before the overview; `TestBackend` test
- [x] 5.14 The overview's drawer answers end the way the manage drawers do: `widget::drawer::foot` (status, then buttons, at the drawer's bottom, full under the pointer), shared with `view::render_drawer_footer`

## 6. Journey

- [x] 6.1 A journey in `04-workspace`: opening an agent's overview from the agent button gains no shell in the process table; a workspace edit made in the overview changes `agents.yaml` on disk with its comments kept

## 7. Docs and gate

- [x] 7.1 The workspace docs page (`web/content/docs/workspace/`) describes the overview and the Settings section, and the journey's `proves:` names it
- [x] 7.2 `make check` green
