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

### Requirement: A focused resident surface receives raw keys, except the host's escape
While a resident surface has focus, the host SHALL deliver keys and pointer
gestures to its script, except the host's escape chord and the pause
key for unattended prompts, which SHALL always reach the host and SHALL NOT
be rebindable by an extension.

#### Scenario: The escape chord always leaves
- **WHEN** a resident game has focus and the operator presses the host's escape chord
- **THEN** focus returns to the workspace and the script receives no key for that chord

### Requirement: An extension may offer a global key to open a surface
An action whose built-in is `open` MAY suggest a key in the workspace scope.
It SHALL be bound only when no host binding in any live scope uses the
chord, and SHALL only open that extension's surface.

#### Scenario: A global key opens a picker
- **WHEN** `jump` suggests `ctrl-j` for its `open` action and no host binding uses `ctrl-j`
- **THEN** `ctrl-j` opens the `jump` overlay from anywhere in the workspace

#### Scenario: A global key never shadows the host
- **WHEN** an extension suggests a chord the host binds in the workspace scope
- **THEN** the chord keeps its host meaning and the action has no default key
