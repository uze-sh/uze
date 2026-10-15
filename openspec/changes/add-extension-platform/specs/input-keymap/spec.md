## ADDED Requirements

### Requirement: Extension actions are keymap actions in the extension's scope
Every action an extension declares SHALL become a keymap action named
`ext.<extension>.<action>`, live only in the scope of that extension's
surface. The operator's keymap file SHALL be able to bind or unbind it like
any other action.

> **Unresolved review findings** (design.md): A4.

#### Scenario: The operator rebinds an extension action
- **WHEN** the keymap file binds `ext.openspec.archive` to `A`
- **THEN** `A` archives on the openspec board and the footer advertises `A`

### Requirement: A suggested key is taken only when it is free
The key the manifest suggests SHALL be the action's default binding only when
no binding in that scope or in any scope that is live beneath it already uses
the chord; otherwise the action SHALL have no default key and stay reachable by
pointer and through the action index.

> **Unresolved review findings** (design.md): A4.

#### Scenario: A suggested key that collides is not taken
- **WHEN** an extension suggests `q` for an action and `q` closes the surface in the host's keymap
- **THEN** `q` still closes the surface, the action has no default key, and it is offered in the footer by pointer and in the action index
