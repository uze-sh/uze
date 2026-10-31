## Why

The code surface's files half read like a viewer with an editor bolted on:
typing needed `e` first, nothing showed which line a click would land on,
and moving to another file with unsaved typing was refused outright, even
when the typing had been undone. The operator asked for an editor that
behaves the way VS Code does: click to type, a highlight on the line, files
kept open in tabs, and a search of every file in a fixed field beside the
content.

## What Changes

- A click on a line of a text file starts typing there. The line under the
  pointer and the caret's line are highlighted across the whole row.
- A buffer is unsaved only while its text differs from the file. Moving to
  another file keeps a changed buffer, marks it in the tree, and reopens it
  from the buffer. Closing the surface asks about every unsaved one.
- Files open in tabs over the content. A file only passed over gets a
  preview tab that the next one replaces. A changed file keeps its tab and
  shows the unsaved dot. A tab is closed by its mark or `Ctrl+W`, and
  closing one with unsaved changes asks once.
- The files list gains a search field over its tree; while it holds
  text, its results take the tree's place, and `Esc` gives the tree back. `git grep` runs as the text is typed, off the drawing thread.
  Results are grouped by file with a count, each line with the match on a
  ground of its own. A result opens with the caret on the match, and the
  match is highlighted in the file too.
- The view contract gains `View::tabs`, `Navigator::field`, the
  `Found`/`Match` rows, the `SelectTab`/`CloseTab`/`FocusField` hits and
  the `Find`/`CloseTab`/`NextTab`/`PreviousTab` commands. The theme gains
  `mark.unsaved`.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `code-surface`: editing, unsaved buffers, tabs and search.

## Impact

`uze-extensions` (`view`, `code`, `code/search.rs`), `uze-keys` (four
actions, bound in the code scopes), `uze-theme` (`mark.unsaved`), the host's
`extension_view` (tab strip, navigator field, line bands), and the code page
of the documentation site.
