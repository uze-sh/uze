## Purpose

Gives an extension's author completion, hover and the check's errors in
their own editor while writing a manifest, a view or a test, since no
editor completes KDL from a schema.

## ADDED Requirements

### Requirement: The language server speaks LSP over stdio
`uze agent plugin lsp` SHALL run a Language Server Protocol server on
standard input and output for `extension.kdl`, a surface's view and
`tests/*.test.kdl`, telling them apart by their path inside an extension's
directory. It SHALL run no code a plugin ships and write no file.

#### Scenario: An editor starts the server
- **WHEN** an editor configured with `uze agent plugin lsp` opens a view inside `extensions/board/`
- **THEN** the server answers `initialize` with completion, hover and diagnostics, and the view is analysed as a view

### Requirement: Diagnostics are the check's own
The server SHALL publish, for an open file, the same errors and warnings
`uze agent plugin check` gives for it, at the same line and column, with
its suggestion, as the author types. A file outside an extension SHALL get
no diagnostics.

#### Scenario: A typo is underlined as it is typed
- **WHEN** an author types `Tabel` in a view
- **THEN** the editor shows an error at that node naming catalog nodes and suggesting `Table`, the same message the check prints

#### Scenario: A missing fixture field is a warning in the view
- **WHEN** a view binds `{stopped}` and the source's fixture has no `stopped`
- **THEN** the editor shows a warning at that reference, as the check does

### Requirement: Completion and hover come from the catalog
Completion SHALL offer, from the catalog the decoder checks against, the
nodes valid where the cursor is, the properties of the node it is in, a
property's allowed values (roles, slots, lookups declared in the file),
and fields of the fixture inside `{…}`. Hover SHALL show a node's or
property's description and accepted values.

#### Scenario: A node offers only its properties
- **WHEN** the cursor is after `Progress ` in a view
- **THEN** completion lists the properties `Progress` accepts and no other

#### Scenario: A role is completed from the roles
- **WHEN** the cursor is inside `role="`
- **THEN** completion lists the semantic roles and the lookup tables the view declares as `{table:…}`

#### Scenario: Hover explains a node
- **WHEN** the author hovers `Segment`
- **THEN** the editor shows what a segment draws and its properties with their kinds
