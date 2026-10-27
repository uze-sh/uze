## ADDED Requirements

### Requirement: Extension actions are keymap actions in a table of the extension's own
Every action an extension declares SHALL become a keymap action named
`ext.<plugin>@<marketplace>/<id>.<action>`, live only while that
extension's surface is in front, in a table of that extension's own
resolved after the host's, so two extensions never collide and the host
always wins. The operator's keymap file SHALL bind or unbind it like any
other action.

> **Review findings** (design.md, *Triage*): A4, resolved by D8.

#### Scenario: The operator rebinds an extension action
- **WHEN** the keymap file binds `ext.acme@github/openspec.archive` to `A`
- **THEN** `A` archives on the openspec board and the footer advertises `A`

#### Scenario: Two extensions may suggest the same letter
- **WHEN** two installed extensions each suggest `r` for a surface action
- **THEN** each binding holds on its own surface, since only the extension in front is consulted

#### Scenario: A conflict inside one extension drops one binding
- **WHEN** an extension's two actions both suggest `r`
- **THEN** the second stays unbound with a problem naming both actions, and the extension still loads

#### Scenario: A name of an absent extension is a warning
- **WHEN** the keymap file binds `ext.acme@github/gone.open` and no such extension is installed
- **THEN** the keymap loads with a warning naming the entry, as it does for a newer uze's names

### Requirement: A suggested key is taken only when it is free
The key the manifest suggests SHALL be the action's default binding only when
the live stack resolves the chord to nothing; otherwise the action SHALL have
no default key and stay reachable by pointer and through the action index.
Collisions SHALL be reported in `uze doctor` and on the Extensions screen
beside the action.

> **Review findings** (design.md, *Triage*): A4.

#### Scenario: A suggested key that collides is not taken
- **WHEN** an extension suggests `q` for an action and `q` closes the surface in the host's keymap
- **THEN** `q` still closes the surface, the action has no default key, and it is offered in the footer by pointer and in the action index

### Requirement: A suggestion follows the pane's rule for bare letters
A surface action MAY suggest a bare letter, since the surface seals it. An
`open` action SHALL suggest only a chord with a modifier or a function key.
An action that declares a confirmation, or acts on an agent, SHALL NOT
suggest a bare letter, and the check SHALL refuse one.

#### Scenario: A destructive action cannot hold a bare letter
- **WHEN** an action declares `confirm` and suggests `d`
- **THEN** the check refuses the suggestion, saying a confirmed action needs a modifier

### Requirement: A focused resident surface receives raw keys, except the host's escape
While a resident surface has focus, the host SHALL deliver keys and pointer
gestures to its script, except the host's escape chord and the pause
key for unattended prompts, which SHALL always reach the host and SHALL NOT
be rebindable by an extension.

#### Scenario: The escape chord always leaves
- **WHEN** a resident game has focus and the operator presses the host's escape chord
- **THEN** focus returns to the workspace and the script receives no key for that chord

### Requirement: An extension may offer a global key to open a surface
An action whose built-in is `open` MAY suggest a key in the workspace scope,
in the global part of the extension's table. It SHALL be bound only when no
host binding in any live scope uses the chord, SHALL only open that
extension's surface, and when two extensions suggest the same chord the
first by identity SHALL take it and the other SHALL stay unbound, said in
`uze doctor`.

> **Review findings** (design.md, *Triage*): R-U7, resolved by D8.

#### Scenario: A global key opens a picker
- **WHEN** `jump` suggests `ctrl-j` for its `open` action and no host binding uses `ctrl-j`
- **THEN** `ctrl-j` opens the `jump` overlay from anywhere in the workspace, and `jump`'s `open` is listed in the action index with `ctrl-j`

#### Scenario: A global key never shadows the host
- **WHEN** an extension suggests a chord the host binds in the workspace scope
- **THEN** the chord keeps its host meaning and the action has no default key

#### Scenario: Disabling drops the table
- **WHEN** the operator disables an extension whose `open` held `ctrl-j`
- **THEN** `ctrl-j` resolves to nothing and the action index no longer lists the action
