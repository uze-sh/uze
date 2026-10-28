## Context

See proposal.md. The workspace's content column was the tab strip (a row and
its hairline) over the agent area; plugins' doors sat in the strip's button
group. Status rows already draw one line from a plugin's view, folded.

## Goals / Non-Goals

**Goals:** a place for plugins that reads as theirs; free lines of their own
there; one case for every control.

**Non-Goals:** a bar across the sidebar; an extension choosing its position
within a side; opening an overlay from an item; a bar that stands empty.

## Decisions

- **The bar is the content column's, not the frame's** (D1).
- **It stands for what is installed, never for what answers** (D2): the pane
  is sized by whether a plugin offers a door or an item, so no answer
  resizes the agent's terminal.
- **Install order within a side** (D3): no priority to claim the start.
- **An item is a status row's line** (D4): its view folded to one line, at
  its natural width plus the host's inset, at most 32 cells.
- **`opens` names a view or a drawer** (D5).
- **Lowercase at drawing, matched in either case by tests** (D6).
