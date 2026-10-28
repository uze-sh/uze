# Decisions — extension-bar

## D1 — The bar stands in the content column, under the agent area

- status:   pending
- task:     2.1
- commit:
- trigger:  The operator tried both in the prototype and asked for it inside the pane's container, at the tab strip's level, not across the frame.
- decided:  Two rows at the column's foot (a hairline, then the row), the tab strip's mirror, over the pane and a docked drawer alike.
- rejected: A frame-wide status bar under the sidebar too: the sidebar has nothing to do with what the bar holds.
- blast:    `render::compute_layout`, `render/bar.rs`.

## D2 — The bar is reserved for what is installed, not for what answers

- status:   pending
- task:     2.1
- commit:
- trigger:  Bands already hold that no answer resizes the agent's terminal.
- decided:  `Reserved::bar` is whether a plugin offers a view, a drawer or a bar item in front; an item that answers nothing leaves its place blank rather than giving the row back.
- rejected: Taking the row only while something is drawn: the pane would resize as items answer.
- blast:    `declared/bar.rs::has_bar`.

## D3 — Items keep their extensions' order within a side

- status:   pending
- task:     2.2
- commit:
- trigger:  VS Code lets an item claim a priority; the operator's question was whether an extension should.
- decided:  Within a side, items follow the order the extensions were found in, after the doors on the left and from the right end inward on the right; what does not fit is left out from the end.
- rejected: A `priority` an extension declares: every extension would claim the start.
- blast:    `render/bar.rs`.

## D4 — An item is one folded line, at most 32 cells

- status:   pending
- task:     2.2
- commit:
- trigger:  An item is a line of the extension's own: hours, a pull request, a meter.
- decided:  Its view is asked at 32×1 and folded like a status row (texts cut, never wrapped), drawn at its natural width plus the host's prose inset, with its own buttons answering first and the rest of it opening `opens`.
- rejected: A free width: one extension could take the whole bar.
- blast:    `render/bar.rs::ITEM_MAX`, `declared/status.rs::folded`.

## D5 — `opens` names a view or a drawer of the same extension

- status:   pending
- task:     1.2
- commit:
- trigger:  An overlay opens through its `open` action; a second way through the bar needs a host path the bar does not have yet.
- decided:  The check refuses an `opens` that names no view or drawer of the extension, listing the ones it can.
- rejected: Overlays now: a new open path for a case nobody asked for.
- blast:    `manifest/check.rs::bar_items_open_a_surface`.

## D6 — Control labels are drawn lowercase; tests name them in either case

- status:   pending
- task:     3.1
- commit:
- trigger:  The operator saw "Timesheet" beside `spec` and `code` and asked for one case on what is clicked, to match UZE's core; every other label stays the author's.
- decided:  The binder lowercases `Button`, `Tab` and `Toggle` labels and a form's submit (default `send`); a tab's value, when it names none, stays as written. The bar's doors are lowercase too. A test's `click` and `press-button` match a label in either case. Everything not clicked keeps the author's case: `Text`, headings, `Badge`, `Rule`, `Details`, a `Choice` (a value, as `harness ‹ Claude Code ›` is in the core), data and a bar item's line.
- rejected: Refusing capitals at check: authors' existing views would stop compiling for a cosmetic rule.
- blast:    `bind.rs::control_label`, `extension_test.rs::hit_on`.
