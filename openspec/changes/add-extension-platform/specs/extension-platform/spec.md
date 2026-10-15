## Purpose

Lets anyone put a surface in front of an operator in the workspace client by
declaring it in a plugin, with no Rust and no change to UZE, while the host
keeps sole ownership of drawing, input, geometry, the palette and state.

## ADDED Requirements

### Requirement: Built-in and declared extensions are hosted the same way
The workspace client SHALL open, route input to, draw, remember the place of,
and close every extension through one contract, whether the extension is
compiled into UZE or declared by a plugin. Adding an extension SHALL require
no change to the workspace client's code. The built-in `code` and `architect`
extensions SHALL be hosted through that contract with no change an operator
can observe.

> **Unresolved review findings** (design.md): A1, A2, A3.

#### Scenario: The built-ins behave as before
- **WHEN** the operator opens the code surface and the architect surface after the change
- **THEN** every key, pointer gesture, mode and remembered place behaves as it did before

#### Scenario: A declared extension appears without a rebuild
- **WHEN** a plugin carrying an extension is installed and its grants accepted
- **THEN** the extension is listed in the Extensions screen and can be opened in the workspace with the same binary

### Requirement: An extension is declared in one manifest that carries no logic
A plugin SHALL declare an extension in `extensions/<id>/extension.yaml`,
holding `protocol`, `id`, `name`, `description`, `surfaces`, `sources`,
`actions`, `events`, `commands` and `wants`. Values in the manifest SHALL be
literals or field references (`{field}`) into the answer of a source or the
current selection; the manifest SHALL NOT contain conditionals, loops or
expressions. Unknown keys SHALL be refused.

> **Unresolved review findings** (design.md): S15.

#### Scenario: An expression is refused
- **WHEN** a manifest writes `marker: "{done > 0 ? done : '-'}"`
- **THEN** the check fails saying a template is a field reference, not an expression

### Requirement: A manifest's protocol is checked and JSON is read as YAML
A manifest declaring a `protocol` this build does not know SHALL be refused
with the version it declares and the versions this build accepts. JSON SHALL
be accepted as YAML.

> **Unresolved review findings** (design.md): `noyalib` 0.0.x.

#### Scenario: A newer protocol is named, not guessed
- **WHEN** a manifest declares `protocol: 2` to a build that knows only `1`
- **THEN** the extension is refused with a message naming both versions, and the rest of the plugin installs

#### Scenario: A JSON manifest is read
- **WHEN** `extension.yaml` is written as a JSON object
- **THEN** it is read as the same manifest its YAML spelling would be

### Requirement: A surface is a full-frame view or a sidebar section
A surface SHALL be of kind `view` (full frame, opened like the code surface)
or `section` (a collapsible block of the workspace sidebar). A `view` SHALL
declare a layout of `sidebar` (a navigator beside content) or `board` (content
over the whole frame, with the navigator folded into tabs). A surface of kind
`interactive` SHALL be refused as not supported by this version.

#### Scenario: A section joins the sidebar
- **WHEN** an enabled extension declares a `section` surface
- **THEN** the workspace sidebar shows it as a collapsible block titled by the extension, drawn from its source

#### Scenario: Interactive is reserved
- **WHEN** a manifest declares `kind: interactive`
- **THEN** it is refused with a reason saying interactive extensions are not supported yet

### Requirement: A surface is offered only where it applies
A surface MAY declare `applies_when` with a list of paths relative to the
checkout; the surface SHALL be offered only in a checkout where at least one of
them exists. Without `applies_when` a surface SHALL be offered everywhere.

#### Scenario: An openspec board is not offered outside an openspec project
- **WHEN** a surface declares `applies_when: [openspec]` and the checkout has no `openspec` path
- **THEN** the surface is absent from that workspace's tabs, sidebar and action index, and none of its sources run

### Requirement: The host holds every piece of view state
Selection, folds, scroll, the chosen mode and subject, an open menu and an
open confirmation SHALL be held by the host, never by a program. The place
of an open view (subject, mode, selected item) SHALL be remembered per
extension and checkout for as long as the workspace client runs, and restored
when the view is opened again, the way the built-in surfaces remember theirs.

#### Scenario: Place survives reopening
- **WHEN** the operator selects an item in mode `tasks`, closes the view, and opens it again
- **THEN** the same item and mode are shown

### Requirement: The catalog is the vocabulary a surface is built from
A surface SHALL be built only from the host's catalog of navigators, chips,
content, section rows and chrome, each entry a meaning the host draws in the
active theme. Colour SHALL be named only as a semantic role; no manifest or
answer SHALL name a colour value or a glyph.

> **Unresolved review findings** (design.md): the proposed shrink to the openspec board's catalog; M4.

#### Scenario: A colour value is refused
- **WHEN** a manifest writes `role: "#ff0000"`
- **THEN** the check fails listing the semantic roles a value may name

#### Scenario: Markdown looks the same everywhere
- **WHEN** an extension's content is `markdown` and the code surface previews a Markdown file
- **THEN** both are rendered by the same renderer, in the same theme

#### Scenario: A navigator is a list, a grouped list or a tree
- **WHEN** a surface declares a navigator
- **THEN** it is a flat list, a list grouped by a field, or a tree from a path field, each item with a name, a detail, a trailing marker, a badge and an icon kind

#### Scenario: Chips name the subject and the mode
- **WHEN** a surface declares chips
- **THEN** they are `subjects` (what is being looked at) and `modes` (how it is shown)

#### Scenario: Content is one of the catalog's kinds
- **WHEN** a surface declares content
- **THEN** it is `message` (text and a hint), `markdown`, `code` (text with a language for highlighting), `table` (named columns over a list), `details` (key/value pairs), `progress` (done over total, with a label), `chart` (bar or sparkline over numeric fields), `log` (lines with a tone), or `stack` (a vertical sequence of the above)

#### Scenario: A section row is built from marks, a name and a value
- **WHEN** a `section` surface declares its rows
- **THEN** each row is a step mark (done or not), a status mark (a semantic role), a name and a trailing value

#### Scenario: Chrome is a menu, a confirmation, a notice or a toast
- **WHEN** a surface asks for chrome
- **THEN** it is a menu of actions on an item, a confirmation, a notice or a toast

### Requirement: Actions are named, offered where they apply, and reachable by key and pointer
An action SHALL declare an `id`, a `label`, where it applies (`on: <surface>`
or `on: <surface>.item`), an optional suggested key, an optional
confirmation, and what it does: run a program, or a built-in (`clipboard`,
`open_path`, `refresh`). An action on an item SHALL appear in that item's
menu; every action SHALL appear in the surface's footer and in the action
index with the key the keymap resolved for it.

> **Unresolved review findings** (design.md): A4.

#### Scenario: An item action is in the item's menu
- **WHEN** the operator opens the menu of a navigator item on a surface with an `archive` action on `board.item`
- **THEN** the menu offers `Archive`, and choosing it runs the action for that item

#### Scenario: A confirmed action waits for the answer
- **WHEN** the operator invokes an action that declares a confirmation
- **THEN** the program runs only after the operator accepts, and not at all if they decline

### Requirement: Extensions react to lifecycle events they declared
The host SHALL deliver to an extension only the events it declared, each
mapped to the sources it refreshes or the action it runs. The events SHALL be
at least `checkout.changed`, `work.named`, `agent.started`, `agent.finished`,
`install.converged`, `rebase.paused` and `theme.changed`. Events SHALL be
coalesced so that a burst of the same event causes one refresh.

> **Unresolved review findings** (design.md): the event list must narrow to what is observable.

#### Scenario: An undeclared event costs nothing
- **WHEN** an agent finishes and an extension declared only `checkout.changed`
- **THEN** no program of that extension runs

#### Scenario: A burst is coalesced
- **WHEN** fifty files change in the checkout within a second
- **THEN** a source mapped to `checkout.changed` runs once for that burst

### Requirement: Extensions may add CLI commands under their own name
A command declared by an extension SHALL be reachable as
`uze ext <extension> <command> [args...]`, run in the current project's
checkout with the same program contract, its answer printed as the program
wrote it and its exit code returned. Extension commands SHALL be listed by
`uze ext <extension> --help` and SHALL NOT appear among UZE's own commands.

> **Unresolved review findings** (design.md): S11, M7.

#### Scenario: A declared command runs
- **WHEN** the operator runs `uze ext openspec status`
- **THEN** the declared program runs in the current checkout and its output and exit code are UZE's

#### Scenario: An extension cannot shadow a UZE command
- **WHEN** an extension declares a command named `install`
- **THEN** it is reachable only as `uze ext <extension> install` and `uze install` is unchanged

### Requirement: `uze ext` names extensions, and its own verbs are reserved
`uze ext list`, `uze ext enable <extension>` and `uze ext disable <extension>`
SHALL list, enable (after showing and accepting its grants) and disable
installed extensions, machine-wide. An extension id equal to one of these verbs,
or to `help`, SHALL be refused by the check.

> **Unresolved review findings** (design.md): M7, S15, S4.

#### Scenario: Enabling from the CLI shows the grants
- **WHEN** the operator runs `uze ext enable openspec` in a terminal
- **THEN** the grants are listed and the extension is enabled only if they are accepted

#### Scenario: A reserved id is refused
- **WHEN** a manifest declares `id: list`
- **THEN** the check fails saying `list` is reserved by `uze ext`

### Requirement: The operator can enable, disable and inspect each extension
The Extensions screen SHALL list built-in and installed extensions with their
source plugin, their accepted grants, whether confinement is enforced, and
whether they are enabled, and SHALL let the operator disable and re-enable an
installed extension. A disabled extension SHALL run no program and deliver no
surface.

> **Unresolved review findings** (design.md): enablement scope; S4.

#### Scenario: Disabling stops every program
- **WHEN** the operator disables an extension
- **THEN** its surfaces disappear from the workspace and none of its sources, actions, events or commands run
