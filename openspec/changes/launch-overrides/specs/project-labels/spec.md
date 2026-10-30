## MODIFIED Requirements

### Requirement: The agent's menu renames the agent or its project
An agent's context menu SHALL offer one `Rename` entry opening `Agent`
and `Project`, by pointer and by keyboard as `Restart in` does, whenever
the agent's project is known. `Agent` SHALL rename the agent as before;
`Project` SHALL edit the project's label inline on that row, seeded with
the name the row shows. Elsewhere the entry SHALL stay a plain `Rename`.

#### Scenario: Renaming the project from the menu
- **WHEN** the operator opens an agent's menu, chooses `Rename` then `Project`, types "uze core" and confirms
- **THEN** every agent of that project names it "uze core", and the agent's own name is unchanged

#### Scenario: Renaming the agent from the menu
- **WHEN** the operator chooses `Rename` then `Agent`
- **THEN** the agent's name is edited inline, as it was before the submenu

#### Scenario: A shell's menu
- **WHEN** the operator opens the menu of a shell tab
- **THEN** it offers a plain `Rename`, renaming the tab

#### Scenario: An agent whose checkout was removed
- **WHEN** the operator opens the menu of an agent whose checkout was removed and whose project is known
- **THEN** `Rename` still offers `Project`

### Requirement: A menu entry that opens a submenu is marked by an arrow
A context menu entry that opens a submenu, or a panel beside the menu,
SHALL be marked by an arrow against the menu's right edge, drawn from the
glyph set in use, and SHALL carry no ellipsis. An ellipsis SHALL mark only
an entry that asks before acting. A plain `Rename` that opens no submenu
SHALL carry no arrow.

#### Scenario: The agent's menu
- **WHEN** the operator opens the menu of an agent whose project is known
- **THEN** `Rename` and `Restart in` each end in an arrow at the menu's right edge, and neither carries an ellipsis

#### Scenario: A shell's menu
- **WHEN** the operator opens the menu of a shell tab
- **THEN** its `Rename` carries no arrow

#### Scenario: An entry that asks first
- **WHEN** the operator opens recent's menu on work that can be discarded
- **THEN** the discard entry ends in an ellipsis and carries no arrow
