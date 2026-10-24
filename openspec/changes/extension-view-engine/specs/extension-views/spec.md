## Purpose

Lets a plugin declare an extension's surfaces and views in KDL and see them
drawn by the workspace from fixture data, with UI state the host holds,
reload on save for authors, and a clear account of why a surface looks as
it does, while running no code the plugin ships.

## ADDED Requirements

### Requirement: An extension is declared in a KDL manifest inside its plugin
A plugin MAY carry extensions under `extensions/<id>/`, each with an
`extension.kdl` declaring `protocol`, `id`, `name`, `description`, its
`surface`s and its `source`s. The manifest SHALL carry no logic, SHALL be
KDL v2, and SHALL be refused, with the rest of the plugin installed, when
its protocol is unknown or its id is reserved.

#### Scenario: A newer protocol is named, not guessed
- **WHEN** a manifest declares `protocol 2` to a build that knows only `1`
- **THEN** the extension is refused with a message naming both versions, and the rest of the plugin installs

#### Scenario: A built-in id is reserved
- **WHEN** a manifest declares `id "code"`
- **THEN** the check fails saying `code` is reserved by a built-in extension

### Requirement: A view is a KDL v2 file of catalog nodes
A surface's view SHALL be a KDL v2 file whose nodes are catalog nodes in
PascalCase with only the properties each accepts. Unknown nodes,
properties and arguments, repeated properties, type annotations and the
wrong case SHALL be refused with file, line, column and a suggestion, and a
KDL v1 spelling SHALL get its v2 form.

#### Scenario: A catalog node in the wrong case is named
- **WHEN** a view writes `stack { … }`
- **THEN** the check fails at that line saying catalog nodes are PascalCase and suggests `Stack`

#### Scenario: A brace pasted from JSX is caught
- **WHEN** a view writes `Text {id}`
- **THEN** the check fails at that line and column saying `Text` takes a string, and suggests `Text "{id}"`

#### Scenario: A repeated property is refused
- **WHEN** a view writes `Button "Go" action="a" action="b"`
- **THEN** the check fails naming the repeated property, rather than keeping either value

#### Scenario: A v1 spelling gets a v2 hint
- **WHEN** a view writes `Toggle id="x" on=true`
- **THEN** the check fails saying KDL v2 writes booleans as `#true`

### Requirement: Parsing is bounded and outside the drawing path
A manifest or view SHALL be parsed only at check, install, enable and a
linked extension's reload, after a size cap and a nesting scan, under a time
limit, never on the thread that draws. The workspace client SHALL draw from
compiled, validated views and SHALL NOT parse KDL.

#### Scenario: A nesting bomb is refused before parsing
- **WHEN** a view nests 400 blocks in 2 KB
- **THEN** it is refused for exceeding the nesting limit, and no parser runs on it

#### Scenario: A slow document is cut off
- **WHEN** a malformed view would take the parser longer than its time limit
- **THEN** the view is refused naming the limit, and the workspace keeps working

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
- **THEN** a row naming `id · plugin@marketplace` is drawn by the host and no part of the view covers it

#### Scenario: The pane narrows while a drawer is open
- **WHEN** the operator opens an extension's `drawer` from its button or key
- **THEN** the agent's pane, and its terminal, narrow by the drawer's column, and return to their width when it closes

#### Scenario: A drawer stays open while the operator types
- **WHEN** a `drawer` is open and the operator types into the agent's pane
- **THEN** the keystrokes reach the agent and the drawer stays open beside it

### Requirement: Views are drawn from the catalog in the active theme
A view SHALL be built only from catalog nodes: `Stack`, `Row`, `Text`,
`Badge`, `Marks`/`Mark`, `Pips`, `Progress`/`Segment`, `List`, `Table`,
`Tabs`/`Tab`, `Toggle`, `Button`, `Activity`, `Elapsed`, `Log`. Colour SHALL
be named only as a semantic role, and the host SHALL animate `Activity` and
tick `Elapsed` without rebuilding the view.

#### Scenario: A progress bar is segmented
- **WHEN** a `Progress` holds six `Segment`s, each with a weight, a ratio and a role
- **THEN** the bar has one segment per entry, widths proportional to the weights, each filled to its ratio in its role's colour

#### Scenario: A colour value is refused
- **WHEN** a view writes `role="#ff0000"`
- **THEN** the check fails listing the roles a value may name

### Requirement: Views bind data by fixed rules, with no expressions
A view SHALL reference data only as `{field}`, `{a.b}`, `{.}`, `{^.field}`
and the scopes `ui.` and `context.`; repeat with `each` over one list field,
nested at most twice; include with `when`/`unless` (presence) or
`show`/`show-any` (a UI value equal to a declared literal). A lone reference
SHALL keep its JSON type; identity properties SHALL be literals.

#### Scenario: A lone reference keeps its type
- **WHEN** a `Segment` writes `weight="{plans}"` and `plans` is the number 19
- **THEN** the weight is the number 19, while `"{plans} planos"` is the string `19 planos`

#### Scenario: Presence is the only test
- **WHEN** a node has `when="next"` and the data's `next` is absent, null, false, empty or zero
- **THEN** the node is not drawn

#### Scenario: Identity is never interpolated
- **WHEN** a view writes `Button "Go" action="{kind}"`
- **THEN** the check fails saying `action` must be a literal

#### Scenario: Expansion is bounded
- **WHEN** nested `each` over a fixture would produce more nodes than the node cap
- **THEN** the view is drawn up to the cap with a notice saying it was folded

### Requirement: Templates and lookup tables are declared in the view
A view MAY declare `template`s used with `use`, resolved against the current
item, with no parameters and no cycle; and `lookup` tables mapping a value
to a role or a label, optionally per locale, used as `{table:field}`. A value
missing from a table SHALL draw the table's default.

#### Scenario: A template cannot use itself
- **WHEN** template `a` uses `b` and `b` uses `a`
- **THEN** the check fails naming the cycle

#### Scenario: A state becomes a role without code
- **WHEN** a view declares `lookup "tone" { partial "warning"; }` and binds `role="{tone:e}"` to an item whose `e` is `partial`
- **THEN** the item is drawn in the `warning` role

#### Scenario: Labels follow the operator's locale
- **WHEN** a view declares the table `t` for locales `pt` and `en` and the operator's locale is `pt`
- **THEN** `"{t:next}"` draws the `pt` label

### Requirement: UI state declared in a view is held by the host
A `Tabs`, `Toggle` or `List` selection with an `id` SHALL keep its value in
the host as `ui.<id>`, change what `show` draws without running anything,
and be kept with the view's place for as long as the client runs.

#### Scenario: Switching tabs runs nothing
- **WHEN** the operator switches `Tabs id="tab"` from `phases` to `log`
- **THEN** the nodes with `show="ui.tab=log"` are drawn and nothing else happens

### Requirement: Data comes from fixtures, and actions are recorded
A source SHALL be fed by the fixture it declares (`fixture "…"`), a JSON
file in the extension's directory, bounded in size. A `Button`'s action
SHALL run nothing in this version: the inspector SHALL record the action
and the arguments it would receive.

#### Scenario: A board drawn from a fixture
- **WHEN** a source declares `fixture "fixtures/board.json"`
- **THEN** the surface is drawn from that file's data

#### Scenario: A button is recorded, not run
- **WHEN** the operator presses a `Button` with `action="next"` and `item="{next}"`
- **THEN** nothing runs and the inspector lists `next` with the item it would have received

### Requirement: A linked extension reloads on save
An extension installed from a linked marketplace SHALL reload when its
files change: a changed view, manifest or fixture SHALL redraw its surfaces
within a second, keeping their place. Extensions from other marketplaces
SHALL NOT be watched.

#### Scenario: Editing a view redraws at once
- **WHEN** an author changes a `gap` in a view of a linked extension and saves
- **THEN** the surface redraws with the new gap within a second, keeping the selected tab

#### Scenario: Only linked extensions are watched
- **WHEN** an extension is installed from a marketplace that is not linked
- **THEN** the workspace does not watch its files

### Requirement: An error never blanks a surface
For every extension, a manifest, view or fixture that fails SHALL leave its
surfaces on their last good draw with the error shown over them, naming the
file, line and column where there is one; a surface with no good draw yet
SHALL show the error alone.

#### Scenario: A broken save keeps the surface
- **WHEN** an author saves a view with an unknown node
- **THEN** the surface keeps its last good draw and shows the error with file, line and column over it

### Requirement: The inspector explains any extension
The workspace SHALL offer, for every extension's surface, an inspector
showing the compiled view with why each absent node is absent, the data it
was bound to, the UI state, the recorded actions and the last errors.

#### Scenario: An author finds out why a node is missing
- **WHEN** a `Button` with `when="next"` is not drawn and the author opens the inspector
- **THEN** the tree marks the button as absent because `next` is missing from the data

### Requirement: Nothing an extension supplies reaches the terminal unsanitized
Every string from a manifest, view or fixture SHALL be stripped of control,
format, zero-width, tag and bidirectional override characters after escapes
are decoded, and bounded in display width, before it is drawn.

#### Scenario: An escape sequence is drawn inert
- **WHEN** a fixture holds a name `"\u001b]52;c;ZXZpbA==\u0007build"`
- **THEN** the surface shows `build` and the clipboard is unchanged

### Requirement: An extension can be scaffolded into a linked marketplace
`uze agent plugin create --extension <id>` SHALL scaffold, inside a linked
marketplace, a manifest, a view and a fixture that pass the check, and
`uze agent plugin check` SHALL validate a plugin's extensions with the same
rules the install applies.

#### Scenario: The scaffold passes its own check
- **WHEN** `uze agent plugin create --extension board` runs in a linked marketplace
- **THEN** `uze agent plugin check` on the new plugin passes and its `section` draws from the scaffolded fixture

### Requirement: KDL is highlighted in UZE's own editor
The code surface's editor and preview, and fenced `kdl` blocks in Markdown,
SHALL highlight KDL v2 with the same syntax theme as every other language.

#### Scenario: A view is coloured while it is edited
- **WHEN** the operator opens an extension's `board.kdl` in the code surface
- **THEN** node names, strings, `#true` and slashdash comments are coloured, as a `.rs` file would be
