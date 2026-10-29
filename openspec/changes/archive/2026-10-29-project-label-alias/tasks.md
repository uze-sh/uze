## 1. Kept on this machine

- [x] 1.1 `uze_core::config`: list the tables of a section, and set and remove a key of `section."name"`, preserving comments and ordering; a new parent is implicit, an emptied table and an emptied parent are removed; tests beside the existing ones, including a file that does not parse
- [x] 1.2 `uze-workspace`: a concern owning the `projects` section: every label by root, set and clear; whitespace trimmed; empty or the directory's own name clears; clearing an absent label writes nothing; lookup by `same_path`; a test that the project's directory is untouched
- [x] 1.3 `uze-application`: a service exposing the labels to `src/` in plain types or re-exports, instrumented like `Notifications`

## 2. Named alike everywhere

- [x] 2.1 One function naming a project from its root and the labels, replacing both copies of `project_name`
- [x] 2.2 Recent's rows and approvals carry the root as `project_root_of` resolves it, resolved inside the reads that produce them
- [x] 2.3 Read the labels into the model when the workspace opens, one toast when unreadable; the sidebar row, recent's selected row and filter, and approval's dialog and toasts read through 2.1

## 3. Renaming

- [x] 3.1 `RenameTarget::Project(TabId)` and `begin_project_rename`; the caption drawn with `rename_spans` while editing, caret never elided
- [x] 3.2 Commit through `spawn_project_label` and its absorb: the model changes only on a written answer, a failure is a toast
- [x] 3.3 The menu's group becomes a kind; `Rename` opens `Agent` / `Project` when the project root is known, including for a removed checkout; a shell or an agent with no known project keeps a plain `Rename`
- [x] 3.4 `Symbol::Submenu` in `uze-theme` with a glyph in every set and the schema; the menu drops the group's ellipsis and draws the arrow against its right edge, for `Continue in` too
- [x] 3.5 The frame records the project text's rect; `double_click` on it begins the project rename, every other gesture on the row unchanged
- [x] 3.6 Update `tests/architecture/affordances.rs` (how `RenameSelection` is reached) and `src/ui/orchestrator/tests/context_menu.rs` (the agent menu's entries)
- [x] 3.7 Write the arrow-for-submenu, ellipsis-for-asks rule into the `design-system` skill, and run its checklist over the submenu and the caption edit

## 4. Proof

- [x] 4.1 Orchestrator tests for the scenarios of `specs/project-labels/spec.md` that a client can observe; the rest at the level of 1.2
- [x] 4.2 `make check` and `openspec validate --all --strict` green
