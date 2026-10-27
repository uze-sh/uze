## ADDED Requirements

### Requirement: A plugin may carry extensions
A plugin MAY carry extensions under `extensions/<id>/`, each an
`extension.kdl` and its views, found by that path. `uze agent plugin check`
SHALL validate each with the rules of the extension check; installing SHALL
deliver them to the workspace client only, never to a harness; removing the
plugin from this machine (`uze remove` with its machine flag) SHALL remove
them and their accepted grants.

> **Review findings** (design.md, *Triage*): A5; the removal verb.

#### Scenario: Check covers the plugin's extensions
- **WHEN** a plugin carries an extension whose manifest has an unknown key
- **THEN** `uze agent plugin check` fails naming the extension, the file and the key

#### Scenario: An extension is never a routed resource
- **WHEN** a plugin carrying an extension is installed for a harness
- **THEN** the harness's delivery, receipts and exposure name only the plugin's skills, hooks, MCP servers and agents, and the extension is found by `extensions/<id>/extension.kdl` for the workspace client alone

#### Scenario: Removing the plugin removes its extension
- **WHEN** the operator removes a plugin that carried an enabled extension with `uze remove` and its machine flag
- **THEN** the extension disappears from the Extensions screen and its accepted grants are no longer recorded

#### Scenario: Undeclaring keeps the record
- **WHEN** a project stops declaring a plugin that carried an extension, and the plugin stays in the Store
- **THEN** the extension is no longer offered in that project, and its machine-wide consent record stays for the projects that still declare it

### Requirement: An extension is checked offline before it is installed
`uze agent plugin check <path>` SHALL validate each extension's manifest and
views, verify field references against its fixtures, run its tests with
recorded answers, run sources only with `--run` (scripts in the engine on
any machine, programs only under enforced confinement), and report every
problem with the file, the field and the fix, exiting non-zero when there
is one.

> **Review findings** (design.md, *Triage*): S12, M8.

#### Scenario: A binding to a missing field is caught
- **WHEN** a navigator item names `marker: "{completedTasks}"` and the source's answer has no `completedTasks`
- **THEN** the check fails naming the source, the field and the fields the answer does have

#### Scenario: A script runs in the check, a program does not run unconfined
- **WHEN** `--run` is given on a machine without confinement for an extension with one `script` source and one `run` source
- **THEN** the script source runs in the engine, and the program source is answered from its recorded answer with a note that programs need enforced confinement or `--checkout`
