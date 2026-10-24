## ADDED Requirements

### Requirement: A plugin may carry extensions
A plugin MAY carry extensions under `extensions/<id>/`, each an
`extension.kdl` and its views. `uze agent plugin check` SHALL validate each
with the rules of the extension check; installing SHALL deliver them to the
workspace client only, never to a harness; removing the plugin SHALL remove
them and their accepted grants.

> **Unresolved review findings** (design.md): A5, removal via `uze plugin remove`.

#### Scenario: Check covers the plugin's extensions
- **WHEN** a plugin carries an extension whose manifest has an unknown key
- **THEN** `uze agent plugin check` fails naming the extension, the file and the key

#### Scenario: Removing the plugin removes its extension
- **WHEN** the operator removes a plugin that carried an enabled extension
- **THEN** the extension disappears from the Extensions screen and its accepted grants are no longer recorded

### Requirement: An extension is checked offline before it is installed
`uze agent plugin check <path>` SHALL validate each extension's manifest and
views, verify field references against its fixtures, run its tests with
recorded answers, run sources only with `--run` under the host's
confinement, and report every problem with the file, the field and the fix,
exiting non-zero when there is one.

> **Unresolved review findings** (design.md): S12, M8.

#### Scenario: A binding to a missing field is caught
- **WHEN** a navigator item names `marker: "{completedTasks}"` and the source's answer has no `completedTasks`
- **THEN** the check fails naming the source, the field and the fields the answer does have
