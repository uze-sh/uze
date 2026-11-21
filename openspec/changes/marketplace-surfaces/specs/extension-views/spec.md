## MODIFIED Requirements

### Requirement: An extension is declared in a KDL manifest inside its plugin
A marketplace MAY offer an extension through its `extensions[]` list, whose
source directory holds an `extension.kdl` declaring `protocol`, `id`, `name`,
`description`, its `surface`s and its `source`s. The manifest SHALL carry no
logic, SHALL be KDL v2, and SHALL be refused when its protocol is unknown or
its id is reserved.

#### Scenario: A newer protocol is named, not guessed
- **WHEN** a manifest declares `protocol 2` to a build that knows only `1`
- **THEN** the extension is refused with a message naming both versions

#### Scenario: A built-in id is reserved
- **WHEN** a manifest declares `id "code"`
- **THEN** the check fails saying `code` is reserved by a built-in extension

### Requirement: Surfaces occupy a view, a sidebar section or a drawer
A surface SHALL declare the slot `view` (the full frame), `section` (a
collapsible sidebar block) or `drawer` (docked on the right of the agent
area, opened by a gesture, laid beside the pane so no pane cell is covered).
The host SHALL own the geometry, fold a section that does not fit, and keep
an attribution strip on every `view` and `drawer` that no content can cover.
Other slots SHALL be refused as not supported by this version.

#### Scenario: A section joins the sidebar
- **WHEN** an enabled extension declares a `section` surface
- **THEN** the sidebar shows it as a collapsible block titled by the extension, drawn from its view

#### Scenario: A full-frame surface is attributed
- **WHEN** an extension's `view` is open
- **THEN** a row naming `<id>@<marketplace>` is drawn by the host and no part of the view covers it

#### Scenario: The pane narrows while a drawer is open
- **WHEN** the operator opens an extension's `drawer` from its button or key
- **THEN** the agent's pane, and its terminal, narrow by the drawer's column, and return to their width when it closes

#### Scenario: A drawer stays open while the operator types
- **WHEN** a `drawer` is open and the operator types into the agent's pane
- **THEN** the keystrokes reach the agent and the drawer stays open beside it

### Requirement: An extension can be scaffolded into a linked marketplace
`uze agent extension create <id> --market <name>` SHALL scaffold, inside a
linked marketplace, a manifest, a view and a fixture that pass the check, and
`uze agent extension check` SHALL validate an extension with the same rules
the install applies.

#### Scenario: The scaffold passes its own check
- **WHEN** `uze agent extension create board --market tools` runs in a linked marketplace
- **THEN** `uze agent extension check` on the new extension passes and its `section` draws from the scaffolded fixture
