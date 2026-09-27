## ADDED Requirements

### Requirement: A plugin may carry extensions
A plugin SHALL be able to carry extensions under `extensions/<id>/`, each with
an `extension.yaml`. `uze agent plugin check` SHALL validate every extension
the plugin carries with the same rules as `uze agent extension check`, and
installing the plugin SHALL deliver its extensions to the workspace client
only, never to a harness. Removing the plugin SHALL remove its extensions and
their accepted grants.

> **Unresolved review finding (plan frozen 2026-09-27; see design.md):** A5 (discovering extensions as routed package resources sends them into harness delivery, contradicting "never to a harness"; dedicated discovery is proposed), and removal must say `uze plugin remove`, because dropping a plugin from `agents.yaml` keeps it in the Store.

#### Scenario: Check covers the plugin's extensions
- **WHEN** a plugin carries an extension whose manifest has an unknown key
- **THEN** `uze agent plugin check` fails naming the extension, the file and the key

#### Scenario: Removing the plugin removes its extension
- **WHEN** the operator removes a plugin that carried an enabled extension
- **THEN** the extension disappears from the Extensions screen and its accepted grants are no longer recorded

### Requirement: An extension is checked offline before it is installed
`uze agent extension check <path>` SHALL validate a manifest against its
schema, verify every field reference against a real answer by running each
source once in a scratch checkout under the same confinement as the host, and
report every problem it finds with the file, the field and the fix, exiting
non-zero when there is one.

> **Unresolved review finding (plan frozen 2026-09-27; see design.md):** S12 (the dry run executes untrusted sources without consent, unconfined where confinement is missing: run only with `--run` under enforced confinement, or against `--checkout` or canned answers) and M8 (verifying every field reference needs `examples:` fixtures and is best effort otherwise).

#### Scenario: A binding to a missing field is caught
- **WHEN** a navigator item names `marker: "{completedTasks}"` and the source's answer has no `completedTasks`
- **THEN** the check fails naming the source, the field and the fields the answer does have
