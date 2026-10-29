## Why

Going from one agent to another and back closed whatever extension surface
the first agent had standing in its pane. A stopgap brought it back, but by
reopening it through the path a door takes, so each extension came back with
whatever its per-checkout place happened to carry: architect and spec looked
right, code always landed on an empty Changes, and the stopgap's own memory
leaked across the space overview. The operator asked for the capability to be
native: each agent keeps its own state across moves between agents.

## What Changes

- A surface belongs to the agent it was opened on, its tab and the shells
  beside it alike. Another agent coming to the front, by any path (a click, a
  key, a space header, an extension asking for an agent, the overview
  following the selection, a reattach), puts it away for that agent, and the
  agent coming back has it standing again as it was left. Another tab of the
  agent in front takes the pane back.
- The space overview opened over a surface puts it away the same way, and
  putting the overview away gives it back.
- The extension contract gains `Entry::Return` and `hold`, both defaulted: a
  return is told apart from an arrival through a door, and `hold` is what a
  surface keeps for a return, which may carry more than its per-checkout
  `place` (code keeps the half it was on).
- The host keeps one record per agent of what was standing, in memory for the
  client's run, never written down.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `space-groups`: a surface is put away for its tab and stands again on return.
- `code-surface`: a return shows the half, selection, tree and scroll left.

## Impact

`uze-extensions` (contract, code extension), the workspace's surface
lifecycle (`src/ui/orchestrator/{surfaces,declared,events,space_overview}.rs`,
`session/mouse.rs`, the attach's end), the design-system skill and
`.agents/rules/extensions.md`.
