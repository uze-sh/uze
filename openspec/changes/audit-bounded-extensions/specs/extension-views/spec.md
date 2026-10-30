## ADDED Requirements

### Requirement: Binding a view is bounded in the work it spends
Binding a view SHALL spend at most a fixed budget of entries, counting every
node and template `use` it walks, drawn or hidden, and every list item it
considers. Past the budget the build SHALL stop walking and draw what it has
with a notice saying the view was folded. The check SHALL refuse a view
whose templates alone expand past the budget, before it is installed.

#### Scenario: A template fan-out is refused by the check
- **WHEN** a view's twelve templates each use the one below ten times
- **THEN** `uze agent plugin check` fails saying the templates expand the view past the bound

#### Scenario: A fan-out compiled elsewhere is folded, not walked
- **WHEN** the binder is handed that view already compiled
- **THEN** the build returns at once with a fold notice, and the drawing thread is never held by it

#### Scenario: Repetition that draws nothing still spends the budget
- **WHEN** an `each` repeats over its own parent's list and every repetition is hidden
- **THEN** the build stops at the budget with a fold notice rather than walking every pair

### Requirement: The nesting scan sees through a line continuation
The pre-parse scan SHALL treat a `\` outside a string, with what may follow
it before the line break, as continuing the node, so a slashdashed node
whose block opens on the next line is still a commented-out block, and one
inside another is refused before the parser runs.

#### Scenario: An escline does not hide nested slashdashed blocks
- **WHEN** a view nests fourteen `/-n \` blocks, each opening on the line after its node
- **THEN** it is refused at once for a block commented out with `/-` inside another, and no parser runs on it
