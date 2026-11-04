## Purpose

Lets one marketplace offer what different consumers read, each kind on its own
surface: plugins for agents, extensions for the workspace, and later kinds
beside them, sharing how bytes are acquired and pinned but not how they reach
their consumer.

## ADDED Requirements

### Requirement: A marketplace offers each kind in its own list
A marketplace SHALL list extensions in `extensions[]` beside `plugins[]`, each entry holding `name`, `source` and an optional `category`. Either list MAY be absent. What a thing is SHALL be read from the list naming it, and never inferred from the files it contains.

#### Scenario: A marketplace of extensions alone
- **WHEN** `marketplace.json` carries `extensions[]` and no `plugins`
- **THEN** `uze market add` records it and the listing offers its extensions

#### Scenario: A package holding only an extension's files
- **WHEN** a `plugins[]` entry's directory holds only `extensions/board/extension.kdl`
- **THEN** it is listed as a plugin, and installing it reads nothing under `extensions/`

### Requirement: A name is unique within its marketplace
A marketplace SHALL NOT offer one name under two kinds. `uze market add` and `uze agent market check` SHALL refuse such a marketplace, naming the name and both lists, and record nothing.

#### Scenario: One name in two lists
- **WHEN** `marketplace.json` lists `board` in both `plugins[]` and `extensions[]`
- **THEN** `uze market add` fails naming `board` and both lists, and records nothing

### Requirement: The root verbs resolve the kind from the catalogue
`uze add <name>@<marketplace>`, `uze <name>@<marketplace>`, `uze remove` and `uze update` SHALL act on whichever kind the name is, with no flag naming the kind, and every report SHALL say which kind it acted on.

#### Scenario: Adding an extension
- **WHEN** `uze add gsd@uze` runs and `gsd` is in that marketplace's `extensions[]`
- **THEN** the extension's bytes are installed and the report calls it an extension

### Requirement: An extension reaches no harness
Installing, updating or removing an extension SHALL write nothing a harness reads, create no receipt and run no delivery.

#### Scenario: A harness sees nothing
- **WHEN** an extension is installed on a machine with every harness attached
- **THEN** no harness's configuration, store or plugin list changes

### Requirement: Installing an extension runs nothing
Installing or updating an extension SHALL NOT run its code or record consent. It SHALL be listed on the Extensions screen as off until the operator allows what it may reach and enables it.

#### Scenario: A fresh install is off
- **WHEN** `uze add gsd@uze` completes
- **THEN** the Extensions screen lists `gsd` as off, and none of its surfaces is drawn

### Requirement: An extension is identified by its id and marketplace
An extension's identity SHALL be `<id>@<marketplace>`, where `<id>` is its catalogue name and equals its manifest's `id`. Consent, secrets, preferences and key bindings SHALL be keyed by it. A mismatch between entry and manifest SHALL be refused.

#### Scenario: Entry and manifest disagree
- **WHEN** the entry says `board` and its `extension.kdl` says `id "panel"`
- **THEN** `uze agent market check` and the install refuse it, naming both

### Requirement: Consent from an earlier identity is not carried
Records keyed by an extension's earlier `plugin@marketplace/id` identity SHALL NOT authorize anything under the new identity. The operator SHALL be asked again.

#### Scenario: Upgrading a machine that allowed an extension
- **WHEN** a machine that allowed `gsd` as `workspace@uze/gsd` installs `gsd@uze`
- **THEN** `gsd` is listed as off, and nothing it asked for counts as granted

### Requirement: A plugin carries no extensions
A plugin's `extensions/` directory SHALL NOT be read as extensions. `uze agent plugin check` SHALL warn on one, naming where it belongs.

#### Scenario: A plugin still holding one
- **WHEN** `uze agent plugin check` runs on a plugin holding `extensions/board/extension.kdl`
- **THEN** it warns, naming `board` and the marketplace's `extensions[]` as where it goes

### Requirement: A project declares extensions beside plugins
`agents.yaml` SHALL accept `extensions:` under a marketplace, as bare names, and `agents.lock` SHALL pin each. `uze install` SHALL converge them both ways, as it does plugins. A name the marketplace lists under another kind SHALL be refused, naming that kind.

#### Scenario: A declared extension is installed
- **WHEN** `agents.yaml` declares `extensions: [gsd]` under `uze` and `uze install` runs
- **THEN** `gsd` is installed and `agents.lock` pins it

#### Scenario: Declared under the wrong key
- **WHEN** `agents.yaml` declares `plugins: [gsd]` and `gsd` is in `extensions[]`
- **THEN** `uze install` fails, saying `gsd` is an extension

### Requirement: Declaring an extension does not let it run
A project's declaration SHALL only offer an extension. It SHALL run in that project only once the operator allowed its grants and enabled it there, so a cloned project cannot run anything by declaring it.

#### Scenario: Opening a project that declares one
- **WHEN** a project declaring `gsd` opens on a machine that never allowed it
- **THEN** the workspace offers it, and none of its code runs until the operator answers

### Requirement: An extension may name the plugins it needs
An extension's manifest MAY name plugins it needs as `<plugin>@<marketplace>`. An MCP grant SHALL name its plugin, which counts as needing it. A reference SHALL hold no version and SHALL NOT be followed transitively.

#### Scenario: An MCP declaration with no plugin
- **WHEN** a manifest declares `mcp "rtc"` with no `plugin`
- **THEN** the check refuses it, saying to name the plugin

### Requirement: A plugin cannot name an extension
A plugin's manifest SHALL have no way to require an extension, so that every agent keeps working without the workspace.

#### Scenario: Installing a plugin in a machine with no extensions
- **WHEN** a plugin is installed where no extension is installed
- **THEN** nothing about its delivery differs and no gap is reported

### Requirement: An unmet need is shown and never installed
An extension whose named plugin is missing SHALL still install. UZE SHALL show the gap with the command that closes it, and SHALL NOT install the plugin. A call relying on it SHALL fail with that gap, never silently.

#### Scenario: The plugin is not installed
- **WHEN** `uze add board@ai` runs, `board` needs `std@cardinal`, and `std@cardinal` is not installed
- **THEN** `board` installs, the report names the gap with `uze add std@cardinal`, and `std@cardinal` stays uninstalled

#### Scenario: Removing the plugin later
- **WHEN** `std@cardinal` is removed while `board` is installed
- **THEN** `board` stays installed, and the Extensions screen and `uze status` show the gap

### Requirement: Taking back a marketplace takes every kind
`uze market remove` SHALL remove everything installed from that marketplace, of every kind, before removing the registry entry.

#### Scenario: A marketplace with both kinds installed
- **WHEN** `uze market remove ai` runs with a plugin and an extension installed from it
- **THEN** both are gone, the plugin through its receipts, and then the registry entry

### Requirement: The Marketplace screen holds one view per kind
The manage modal's Marketplace screen SHALL offer a view per kind and one of the marketplaces themselves. Each kind's view SHALL group its entries by marketplace, with what ships in the build among them, marked Bundled. A filter SHALL narrow a view to what is installed or has an update.

#### Scenario: Bundled and optional stand together
- **WHEN** the operator opens the extensions view
- **THEN** the built-in extensions stand in the `uze` group marked Bundled, beside what the marketplace offers

#### Scenario: Asking what has an update
- **WHEN** the operator chooses `updates` in the themes view
- **THEN** only themes with a new revision are listed, and an empty result says everything is up to date

### Requirement: An extension is managed where it is installed
There SHALL be no Extensions screen. An extension's drawer in the Marketplace SHALL offer installing, allowing, enabling, disabling, its preferences and removing, each only where it applies. A bundled extension SHALL offer no removal, and SHALL say why.

#### Scenario: From available to running in one place
- **WHEN** the operator installs an extension from its drawer
- **THEN** the same drawer offers allowing and enabling it, with no change of screen

### Requirement: A theme is chosen from either place
The Marketplace's themes view SHALL install a theme and offer using it. The Settings screen SHALL choose among installed themes and link to that view. Removing the theme in use SHALL say that uze goes back to `adaptive`.

#### Scenario: Removing the theme in use
- **WHEN** the operator confirms removing the active theme
- **THEN** uze draws in `adaptive`, and the theme's status is `available`

### Requirement: An extension is authored through its own verbs
`uze agent extension create <id> --market <name>`, `uze agent extension check <path>` and `uze agent extension lsp` SHALL scaffold, validate and serve an extension. The official authoring Skill SHALL cover every kind.

#### Scenario: A scaffold is listed and passes
- **WHEN** `uze agent extension create board --market tools` runs in a linked marketplace
- **THEN** `extensions/board/` exists, `extensions[]` lists it, and `uze agent extension check` on it passes
