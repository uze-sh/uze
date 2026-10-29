## Why

A plugin's views and drawers had their doors in the tab strip, beside
`spec`, `arch` and `code`, titled as each author wrote them ("Timesheet").
The operator read that as plugins extending UZE's own header, and asked for
a place of their own, like VS Code's status bar, where an extension can also
put a line of its own (hours logged, a pull request's state, a meter) on the
side it chooses. Labels differed in case from UZE's own.

## What Changes

- A bar under the agent area, in the content column, the tab strip's mirror
  at its foot. It stands only while a plugin puts something in it.
- The doors to plugins' views and drawers move there from the tab strip,
  which keeps UZE's own surfaces only.
- A new slot, `bar`: one item, a line drawn from its view, `align="left"`
  (the default) or `"right"`, opening the view or drawer `opens` names.
- Every control label the host draws for an extension is lowercase: the
  bar's doors, and `Button`, `Tab`, `Toggle` and a `Form`'s submit.

## Capabilities

### New Capabilities
- `extension-bar`: the bar, its doors and items, and lowercase controls.

### Modified Capabilities
None.

## Impact

`uze-contract` (slot, `BarDecl`), the manifest decoder and check, the host's
layout, tab strip and a new bar renderer, the binder (labels), the test
runner (steps match labels in either case), journeys 15–28 and the journey
runner (`in: bar`), the `uze:author` extension reference, the design-system
skill and `docs/extensions/`.
