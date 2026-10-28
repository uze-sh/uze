## Purpose

Gives plugins a place of their own under the agent area, apart from UZE's
header, for the doors to their surfaces and for lines of their own, and
draws every control a plugin declares in the product's one case.

## ADDED Requirements

### Requirement: Plugins' doors and items stand in a bar of their own
The tab strip SHALL offer UZE's own surfaces only. The doors to a plugin's
views and drawers SHALL stand in a bar under the agent area, in the content
column, which SHALL take rows only while a plugin offers a door or an item.

#### Scenario: A plugin's drawer is opened from the bar
- **WHEN** a plugin declares a drawer titled "Timesheet"
- **THEN** the tab strip shows no door for it, and the bar shows `timesheet`, which opens it

### Requirement: An extension puts an item in the bar on the side it chooses
A surface of slot `bar` SHALL draw one line from its view, on the bar's left
(the default) or its right as `align` says, after the doors on the left and
from the right end inward on the right, in the order the extensions were
found. A click on it SHALL open the view or drawer of its own extension that
`opens` names; `opens` naming anything else SHALL be refused by the check.

#### Scenario: An item on the right opens its drawer
- **WHEN** an item `align="right" opens="panel"` answers `6h12 today`
- **THEN** it is drawn at the bar's right end, and a click on it opens the drawer `panel`

### Requirement: Every control label of an extension is lowercase
The host SHALL draw the bar's doors and the labels of `Button`, `Tab`,
`Toggle` and a `Form`'s submit in lowercase, whatever the extension wrote.
A test step naming a control SHALL find it in either case.

#### Scenario: A button written in title case
- **WHEN** a view declares `Button "Run phase 12"`
- **THEN** it is drawn `run phase 12`, and `click "Run phase 12"` in a test presses it
