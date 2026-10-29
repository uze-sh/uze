## Purpose

Lets an extension keep a view and the logic it shows in one file, a
component, written in the KDL and Rhai UZE already reads.

## ADDED Requirements

### Requirement: A component file holds a view and a script in blocks
A `.uze` file SHALL hold at most one `<view>` block and at most one
`<script>` block, each opened and closed by its tag alone on a line
(trailing whitespace, CRLF and a BOM accepted), with only blank lines and
`//` comments outside them, and at least one block. The `<view>` SHALL be
read as a view and the `<script>` as a script, and every position either
reports SHALL be its line and column in the `.uze` file.

#### Scenario: A typo in a view block
- **WHEN** line 12 of `panel.uze`, inside `<view>`, declares `Stak`
- **THEN** the check reports `panel.uze:12:…` naming `Stak`, as it did for a `.kdl` view

#### Scenario: A file saved with Windows line endings
- **WHEN** `panel.uze`'s tag lines end in CRLF
- **THEN** it is read as the same file with LF

### Requirement: The manifest names components, never loose views or scripts
A surface's `view` SHALL name a `.uze` file with a `<view>`; a source's
`script` or `resident`, and an action's `call … in=`, SHALL name a `.uze` file
with a `<script>`. A `.kdl` view or `.rhai` script SHALL be refused, naming
the `.uze` to write. `extension.kdl` and `tests/*.test.kdl` SHALL stay KDL.

#### Scenario: A manifest from before components
- **WHEN** a surface declares `view="board.kdl"`
- **THEN** the check refuses it, suggesting `board.uze` with a `<view>` block

### Requirement: An action runs code by calling a function
An action that runs code SHALL `call "<function>"` to run that function,
with `(input, context)`, of the `<script>` in the file of the surface it
is `on`, or of the file `in=` names, applying the envelope it returns. An
action SHALL NOT run a `script`; it is refused, suggesting `call`.

#### Scenario: The month buttons of the timesheet
- **WHEN** `action "prev" on="panel" { call "prev"; }` and `panel.uze`'s script defines `fn prev(input, context)`
- **THEN** pressing the button runs `prev`, and its envelope's store and refresh apply

#### Scenario: An action written with `script`
- **WHEN** `action "archive" on="board" { script "archive.uze"; }`
- **THEN** the check refuses it, suggesting `call "<fn>" in="archive.uze"`

### Requirement: A call needs consent and is checked before it runs
A manifest with a `call` SHALL need consent as one with a script source
does. The check SHALL refuse a `call` whose file has no `<script>` or no
such function, a `call` into a script a source runs `resident`, and a
call of `answer` or of a resident's entry points.

#### Scenario: A call without consent
- **WHEN** an extension has only fixture sources, wants nothing, and one action with `call`
- **THEN** it is not enabled until the operator consents, as with a script source
