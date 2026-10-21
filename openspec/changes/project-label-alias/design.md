## Context

A project is named today by `project_name(path)`, the last component of
a path, in two copies (`src/ui/orchestrator/recent.rs`,
`src/ui/orchestrator/approval.rs`), used by the sidebar row, recent's
selected row and filter, and the approval dialog and its toasts.

Those surfaces do not hold the same path. The sidebar reads
`model.projects`, filled through `uze_application::project_root_of`
(slot key, then the nearest anchor). Recent carries
`PreservedWork.project`, which for an in-place agent is the canonical
pane directory (`services/tasks/placement.rs`), and approval carries the
primary checkout. In a monorepo with a nested `agents.yaml`, or for an
agent started in a subdirectory, they differ.

`config.toml` is the *authored* tier (`persistence-tiers` rule): no shape,
no version, additive keys, never written over when it does not parse.
`uze_core::config` edits one `section.key` under a lock, keeping comments;
a concern in `uze-workspace` owns its section (`notifications`) and
`uze-application` exposes it as a service.

## Goals / Non-Goals

**Goals:**
- One key per project and one function naming it, for every surface.
- The label is the operator's text in their own file.

**Non-Goals:**
- A label anywhere in the project, or a CLI verb for it.
- The Settings screen: it chooses among known values; a label is free
  text bound to one project.
- Following a project that moves, or a label left behind: it is the
  operator's text, never cleaned up. Forgetting a project's work in
  recent removes its record directory, not its label.
- Renaming from recent: `recent-work` offers no action but a row's own.
- Two clients at once: each reads labels when it opens, so a rename in
  one reaches the other on its next open.

## Decisions

**Keyed by the project's root, one table per project.**

```toml
[projects."/home/hiukky/uze"]
label = "uze core"
```

The root is `project_root_of`, the resolution the sidebar already uses.
Recent and approval resolve their path through it too, inside the reads
that produce them (both already off the UI thread), so the model only
ever holds roots. Lookup compares keys by their components, then with
`uze_platform::path::same_path`, so a key written by hand with a trailing
separator matches everywhere, and one with another case or separator on
Windows; a hand-written Windows key needs a literal string
(`'C:\work\app'`). A rename writes under a key already naming the root
and takes every other one away, so a root never holds two labels. A table per project, not a flat map, leaves room
for a later per-project preference under the same key. The path, not the
remote, is the identity: two clones are two projects to tell apart.

**`uze_core::config` learns one level of nesting.** List every table of a
section with a string key in it, and set and remove `key` in
`section."name"`, under the existing lock and comment-preserving edit. A
new parent is an implicit table, so the file reads `[projects."/x"]` with
no bare `[projects]`; a removal leaving the table empty removes it, and
an emptied parent too.

**Read when the workspace opens, written off the UI thread.** Labels are
read once into the model; an unreadable file leaves directory names and
one toast. Each read carries the number it was asked under and only the
latest is kept. A commit is `spawn_project_label` plus an `absorb`, shaped
like `spawn_agent_label`/`LabelResolution`, because the write may wait on
the file lock; the answer carries the root and name it wrote, and the
model changes only when it says it was written, by reading every label
again. Writes go one at a time: a name confirmed while one is out waits,
the latest per root, so the last confirmed is the last written.
The service returns plain types or `uze-application` re-exports: `src/`
names neither `uze_core` nor `uze_workspace`.

**`RenameTarget::Project { tab, root }`.** Keyed by the row, so the
editor appears on that row only, and carrying the root resolved through
`model.projects` when the edit began, so the name lands there even if the
pane is placed again before Enter. Begun by a
`begin_project_rename(model, tab)` beside `begin_rename`. The editor is
drawn with `rename_spans_within` in the caption's place, the harness
yielding room: the text after the caret is cut at its end first, then the
text before it at its head, each cut marked, the caret never elided.

**No new hit for the caption.** A hit pushed for the project's text
would shadow the row's `SelectTab`, which the right-click menu, drag
arming and drop targets all match. The row keeps one `SelectTab`; the
frame records the project text's rect beside it, and only `double_click`
consults it, so every other gesture and the hover are unchanged.

**The menu's group becomes a kind.** `ContextMenu::groups` becomes an
enum, harnesses or rename choices, so `Rename` opens through
`open_submenu`, keyboard included. It is offered only when the agent's
project root is known; a shell tab, or an agent whose project is not yet
known, keeps the plain `Rename`. `Action::RenameSelection` (f2) keeps
renaming the agent directly: the frequent case gets no extra step.

**A submenu is marked by an arrow, an ellipsis only asks.** The menu
spelled a group as `Continue in…`, the same mark `ContextMenu::worded`
gives an entry that asks before acting. A group now carries no ellipsis
and the menu draws a new `Symbol::Submenu` against its right edge, one
glyph per set (`>` in ascii, `›` by default, the set's own in nerd);
`chevron.right` already means "the item under discussion" and is not
reused.

## Risks / Trade-offs

- [The menu's `Rename` gains a step for the common case] → f2 and the
  double click on the name still rename the agent directly.
- [A path key in a file synced between machines through dotfiles] → a
  key for a path absent on a machine is never matched there.
- [Resolving recent's and approval's paths walks for anchors] → done in
  the reads that already run off the UI thread, never at render.
