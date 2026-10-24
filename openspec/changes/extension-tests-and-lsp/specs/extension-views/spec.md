## MODIFIED Requirements

### Requirement: A view is a KDL v2 file of catalog nodes
A surface's view SHALL be a KDL v2 file whose nodes are catalog nodes in
PascalCase with only the properties each accepts. Unknown nodes,
properties and arguments, repeated properties, type annotations and the
wrong case SHALL be refused with file, line, column and a suggestion: the
nearest valid name when one is close, the valid names otherwise. A KDL v1
spelling SHALL get its v2 form.

#### Scenario: A catalog node in the wrong case is named
- **WHEN** a view writes `stack { … }`
- **THEN** the check fails at that line saying catalog nodes are PascalCase and suggests `Stack`

#### Scenario: A misspelled node gets the nearest name
- **WHEN** a view writes `Tabel { … }`
- **THEN** the check fails at that line saying `Tabel` is not a catalog node and suggests `Table`

#### Scenario: A brace pasted from JSX is caught
- **WHEN** a view writes `Text {id}`
- **THEN** the check fails at that line and column saying `Text` takes a string, and suggests `Text "{id}"`

#### Scenario: A repeated property is refused
- **WHEN** a view writes `Button "Go" action="a" action="b"`
- **THEN** the check fails naming the repeated property, rather than keeping either value

#### Scenario: A v1 spelling gets a v2 hint
- **WHEN** a view writes `Toggle id="x" on=true`
- **THEN** the check fails saying KDL v2 writes booleans as `#true`

### Requirement: An extension can be scaffolded into a linked marketplace
`uze agent plugin create --extension <id>` SHALL scaffold, inside a linked
marketplace, a manifest, a view, a fixture and a test that pass the check,
and `uze agent plugin check` SHALL validate a plugin's extensions with the
same rules the install applies.

#### Scenario: The scaffold passes its own check
- **WHEN** `uze agent plugin create --extension board` runs in a linked marketplace
- **THEN** `uze agent plugin check` on the new plugin passes, runs the scaffolded test, and its `section` draws from the scaffolded fixture

## ADDED Requirements

### Requirement: The check binds a view under every tab
The check SHALL bind each view to its source's fixture once for every
choice of each `Tabs` it holds, and warn once for each field the view
reaches for that the fixture lacks, at the reference's line and column,
whichever tab draws it.

#### Scenario: A field under a second tab is warned about
- **WHEN** a view's `Activity` tab binds `{log}` and the fixture has no `log`
- **THEN** the check warns at that reference that `log` is not in the fixture, although `Activity` is not the first tab
