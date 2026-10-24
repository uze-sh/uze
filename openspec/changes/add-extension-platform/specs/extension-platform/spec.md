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
A plugin SHALL declare an extension in `extensions/<id>/extension.kdl`,
holding `protocol`, `id`, `name`, `description`, `surfaces`, `sources`,
`actions`, `events`, `preferences` and `wants`. Values in the manifest SHALL be
literals or field references (`{field}`) into the answer of a source or the
current selection; the manifest SHALL NOT contain conditionals, loops or
expressions. Unknown keys SHALL be refused.

> **Unresolved review findings** (design.md): S15.

#### Scenario: An expression is refused
- **WHEN** a manifest writes `marker "{done > 0 ? done : '-'}"`
- **THEN** the check fails saying a template is a field reference, not an expression

### Requirement: A manifest's protocol is checked
A manifest declaring a `protocol` this build does not know SHALL be refused
with the version it declares and the versions this build accepts, and the
rest of the plugin SHALL install.

> **Unresolved review findings** (design.md): `noyalib` 0.0.x (moot for manifests since D22).

#### Scenario: A newer protocol is named, not guessed
- **WHEN** a manifest declares `protocol 2` to a build that knows only `1`
- **THEN** the extension is refused with a message naming both versions, and the rest of the plugin installs

### Requirement: A view is a KDL v2 file of catalog nodes
A surface's view SHALL be a KDL v2 file in the extension's directory whose
nodes are catalog nodes, named in PascalCase, with only the properties each
accepts. The check
SHALL refuse unknown nodes, unknown properties, unexpected arguments,
repeated properties and type annotations, each with file, line, column and
a suggestion, and SHALL say how to write a KDL v1 spelling in v2.

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

### Requirement: Parsing a view is bounded and outside the client
A view SHALL be parsed only at check, install, enable and a linked
extension's reload, never by the drawing process's render path, and only after a size
cap and a nesting-depth scan; parsing SHALL run under a time limit. The
client SHALL draw from the compiled, validated view, never from KDL.

#### Scenario: A nesting bomb is refused before parsing
- **WHEN** a view nests 400 blocks in 2 KB
- **THEN** the check refuses it for exceeding the nesting limit, and no parser runs on it

#### Scenario: A slow document is cut off
- **WHEN** a malformed view would take the parser longer than its time limit
- **THEN** the view is refused naming the limit, and the workspace keeps working

### Requirement: Views bind data by fixed rules, with no expressions
A view SHALL reference data only as `{field}`, `{a.b}`, `{.}`, `{^.field}`
and the scopes `form.`, `pref.`, `context.`, `ui.` and a source's name; repeat
with `each` over one list field, nested at most twice; and include with
`when`/`unless` (presence) or `show`/`show-any` (a host UI value equal to a
declared literal). Anything else SHALL be refused.

#### Scenario: A lone reference keeps its type
- **WHEN** a `progress` segment writes `weight="{plans}"` and `plans` is the number 19
- **THEN** the weight is the number 19, while `"{plans} planos"` is the string `19 planos`

#### Scenario: Identity is never interpolated
- **WHEN** a view writes `Button "Go" action="{kind}"`
- **THEN** the check fails saying `action` must be a literal naming a declared action

#### Scenario: Presence is the only test
- **WHEN** a node has `when="next"` and the answer's `next` is absent, null, false, empty or zero
- **THEN** the node is not drawn

#### Scenario: A template cannot use itself
- **WHEN** template `a` uses `b` and `b` uses `a`
- **THEN** the check fails naming the cycle

### Requirement: Lookup tables map values to roles and labels
A view MAY declare named lookup tables mapping a value to a role, a label
or a symbol, optionally per locale, used as `{table:field}` (the entry for
the field's value) or `{table::key}` (the entry for a literal key, as a
label). A value missing from a table SHALL draw the table's default, or the
role `Default` with a check warning when it has none.

#### Scenario: A state becomes a role without the program
- **WHEN** a view declares `lookup "tone" { done "success"; partial "warning" }` and binds `role="{tone:state}"` to an item whose `state` is `partial`
- **THEN** the item is drawn in the `warning` role

#### Scenario: Labels follow the operator's locale
- **WHEN** a view declares the table `t` for locales `pt` and `en` and the operator's locale is `pt`
- **THEN** `"{t::next}"` draws the `pt` label

### Requirement: UI state declared in a view is held by the host
A `tabs`, `toggle`, `select`, `field` or list selection with an `id` SHALL
keep its value in the host as `ui.<id>`, change what `show` and a list's
`filter` draw without running a program, and reach an action only when the
action names it in `with`. A `form`'s fields SHALL reach its action as
`form.<id>`.

#### Scenario: Switching tabs runs nothing
- **WHEN** the operator switches a `tabs id="tab"` from `all` to `working`
- **THEN** the nodes with `show="ui.tab=working"` are drawn and no program runs

#### Scenario: A form's comments reach the action
- **WHEN** a form repeats `field id="comment.{id}"` per section and the operator submits two comments
- **THEN** the action declaring `with "form.*"` receives both under `form`, and no other action receives them

### Requirement: A surface occupies one of the host's slots
A surface SHALL declare one slot: `view` (the full frame), `section` (a
sidebar block), `drawer` (a panel docked on the right of the agent area,
laid beside the pane), `status` (a row in the sidebar's header), `band` (1
to 6 rows the manifest fixes, on an edge of an agent's pane), `overlay` (a
picker, form or dialog) or `notify`. The host SHALL own every slot's
geometry, SHALL NOT let a surface cover a pane's cells, and SHALL refuse an
unknown slot or `kind="interactive"`.

#### Scenario: A section joins the sidebar
- **WHEN** an enabled extension declares a `section` surface
- **THEN** the workspace sidebar shows it as a collapsible block titled by the extension, drawn from its source

#### Scenario: The pane narrows while a drawer is open
- **WHEN** the operator opens an extension's `drawer` from its button or key
- **THEN** the agent's pane, and its terminal, narrow by the drawer's column, and return to their width when it closes

#### Scenario: A band sits beside its agent, not over it
- **WHEN** an extension declares a 4-row `band` at the top of agent panes and an agent's pane is 30 rows tall
- **THEN** the band takes 4 rows of that pane, the agent's terminal is laid out in the 26 left, and no answer of the band ever changes that size

#### Scenario: A full-frame surface cannot pass for UZE
- **WHEN** an extension's `view` answers cells that draw what looks like an agent pane
- **THEN** the host's attribution strip naming `id · plugin@marketplace` stays drawn on its own row, which no answer can cover

#### Scenario: A status segment is placed by the operator
- **WHEN** two extensions each declare a `status` surface
- **THEN** both rows appear in the sidebar's header in the order the operator chose, never in an order the extensions ask for, and no agent pane changes size

#### Scenario: A surface that does not fit is folded
- **WHEN** a `section` needs more rows than the sidebar can give
- **THEN** it is drawn folded to its title with a mark saying it has more, and nothing of it is cropped mid-row

#### Scenario: Interactive is retired
- **WHEN** a manifest declares `kind="interactive"`
- **THEN** it is refused with a reason pointing at `feed="resident"`

### Requirement: A surface declares how it is fed
A surface SHALL declare a `feed`: `bind` (manifest nodes filled from a
source's data), `compose` (a source answers a catalog tree), `cells`,
`image` or `sprite` (a source answers that medium), or `resident` (a
script kept alive, under the `resident` grant). Changing a surface's feed
SHALL NOT change its slot, actions, keys or grants other than `resident`.

#### Scenario: A composed surface draws the tree it was answered
- **WHEN** a `compose` source answers a `stack` of a segmented `progress` and a `table`
- **THEN** the surface shows that bar and that table in the active theme, and nothing the tree did not name

#### Scenario: A composed node hands its item to the action
- **WHEN** a composed `button` names the action `next` with `item: {"phase": "12"}` and `next` runs `[gsd, plan, "{phase}"]`
- **THEN** pressing it runs the action with `12` as its argument, and the action's confirmation shows `12`

#### Scenario: A composed tree can only invoke declared actions
- **WHEN** a composed `button` names the action `deploy` and the manifest declares no action `deploy`
- **THEN** the button is drawn disabled with a notice naming the missing action, and pressing it runs nothing

#### Scenario: A resident feed needs its grant
- **WHEN** a surface declares `feed="resident"` and `wants` has no `resident`
- **THEN** the check fails naming the surface and the missing grant

### Requirement: A surface is offered only where it applies
A surface MAY declare `applies-when` with a list of paths relative to the
checkout; the surface SHALL be offered only in a checkout where at least one of
them exists. Without `applies-when` a surface SHALL be offered everywhere.

#### Scenario: An openspec board is not offered outside an openspec project
- **WHEN** a surface declares `applies-when "openspec"` and the checkout has no `openspec` path
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
A surface SHALL be built only from the host's catalog, in families (layout,
text, data, measure, status, media, cells, input), each node a
meaning the host draws in the active theme. Colour SHALL be named only as a
semantic role; no manifest or answer SHALL name a colour value or a glyph,
except the pixels of an image.

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

#### Scenario: Content is a node of the catalog's families
- **WHEN** a surface declares content, bound or composed
- **THEN** each node, spelled in PascalCase in a view and in a compose answer's `type`, is one of layout (`stack`, `row`, `split`, `tabs`, `gap`, `rule`), text (`text`, `markdown`, `code`, `log`), data (`list`, `table`, `details`, `grid`), measure (`progress`, `gauge`, `pips`, `chart`, `heatmap`), status (`badge`, `marks`, `chip`, `activity`, `elapsed`), media (`image`, `sprite`), `cells` or input (`button`, `toggle`, `select`, `field`, `form`, `picker`), and an unknown node is refused naming the families

#### Scenario: Feedback is requested, never drawn by a node
- **WHEN** a composed tree contains a node of type `confirm` or `toast`
- **THEN** the answer is refused as outside the catalog, while the same toast requested through the answer's envelope is drawn by the host under the extension's name

#### Scenario: A progress bar is segmented
- **WHEN** a `progress` node carries six segments, each with a weight, a ratio and a role
- **THEN** the bar is drawn with one segment per entry, widths proportional to the weights, each filled to its ratio in its role's colour

#### Scenario: A row carries several marks
- **WHEN** a `section` row or `list` item carries `marks: [success, warning, muted]`
- **THEN** three marks are drawn in that order, each in its role, beside the row's name and trailing value

#### Scenario: The host animates, programs do not
- **WHEN** a node is `activity` bound to an agent that is working, or `elapsed` since a start time
- **THEN** the host moves the mark and ticks the duration on its own clock and runs no program to do it

#### Scenario: Chrome is a menu, a confirmation, a notice or a toast
- **WHEN** a surface asks for chrome
- **THEN** it is a menu of actions on an item, a confirmation, a notice or a toast

#### Scenario: A form is drawn by the host and answered as input
- **WHEN** an action declares a `form` with a required `text` field `title` and the operator submits it empty
- **THEN** the host keeps the form open with the field marked, runs nothing, and on a valid submit passes `title` in the program's input

### Requirement: Actions are named, offered where they apply, and reachable by key and pointer
An action SHALL declare an `id`, a `label`, where it applies (`on:
<surface>` or `<surface>.item`), an optional key and confirmation, and what
it does: a program, or a built-in (`clipboard`, `open_path`, `refresh`,
`open`, `focus_agent`, an agent action). An item's action SHALL appear in
its menu; every action SHALL appear in the footer and action index with its
resolved key.

> **Unresolved review findings** (design.md): A4.

#### Scenario: An item action is in the item's menu
- **WHEN** the operator opens the menu of a navigator item on a surface with an `archive` action on `board.item`
- **THEN** the menu offers `Archive`, and choosing it runs the action for that item

#### Scenario: A confirmed action waits for the answer
- **WHEN** the operator invokes an action that declares a confirmation
- **THEN** the program runs only after the operator accepts, and not at all if they decline

### Requirement: Extensions react to the events they declared
The host SHALL deliver to an extension only the events it declared, each
mapped to sources to refresh or an action: `checkout.changed` (per open
checkout), `paths.changed`, `every`, the bridge's agent events,
`work.named`, `install.converged`, `rebase.paused`, `theme.changed`. Events
SHALL be coalesced per extension in a window.

> **Unresolved review findings** (design.md): observability of events (see D17).

#### Scenario: An undeclared event costs nothing
- **WHEN** an agent finishes and an extension declared only `checkout.changed`
- **THEN** no program of that extension runs

#### Scenario: A burst is coalesced
- **WHEN** fifty files change in the checkout within a second
- **THEN** a source mapped to `checkout.changed` runs once for that burst

#### Scenario: A declared path that Git ignores still refreshes
- **WHEN** a source is mapped to `paths.changed: [.planning]`, `.planning` is ignored by Git, and a file under it is written
- **THEN** the source runs again within the digest's period

#### Scenario: Each event reaches the program when asked
- **WHEN** a source declares `agent.tool.finished` with `deliver="each"` and three tool calls finish within the coalescing window
- **THEN** the source runs three times, in order, each with that event in `input.event`

#### Scenario: An interval has a floor and pauses when unseen
- **WHEN** a manifest declares `every "1s"` for a source shown only in a folded section
- **THEN** the check reports the interval raised to the 5-second floor, and the source does not run while nothing that shows it is visible

### Requirement: An extension keeps a small store of its own
An extension SHALL have one bounded key/value store per project, keyed by
its identity (`plugin@marketplace/id`), or one for all projects only if the
manifest declares it and consent says so. Programs SHALL receive it when
their declaration names it and patch it through any answer. It SHALL be
removed with the plugin; a patch over the bound SHALL be refused.

#### Scenario: A high score survives a restart
- **WHEN** a program answers `store: {"best": 42}` and the workspace client is restarted
- **THEN** the next run of a declaration that names the store receives `best: 42`

### Requirement: Preferences are typed, declared and edited by the operator
A manifest MAY declare preferences of type `text`, `number`, `toggle`,
`choice` or `secret`, edited on the Extensions screen. A program SHALL
receive only those its declaration names. A `secret` SHALL be kept outside
UZE's plain records, never shown again, logged, cached under, or
sent to an agent, and an answer of a run that received it SHALL NOT patch
the store.

#### Scenario: A secret reaches only what names it
- **WHEN** a preference `token` is a `secret` and only the source `usage` names it
- **THEN** `usage` receives the token in its input, every other program does not, and the Extensions screen shows it as set without its value

### Requirement: Reference extensions prove the platform
The platform SHALL ship ten reference extensions as fixture plugins, each
recreating a plugin operators use on a harness, and SHALL be accepted only
when each runs with no code of UZE's written for it. A catalog node SHALL be
added only when a reference extension needs it.

#### Scenario: The GSD band runs beside its agent
- **WHEN** the `gsd-band` reference is enabled in a checkout whose `.planning/STATE.md` has a milestone in progress
- **THEN** a 6-row band on that agent's pane shows the segmented milestone bar, a table of phases with D, P and E marks and plan pips, and a button naming the next GSD step for that phase

#### Scenario: The next step is sent in the agent's own dialect
- **WHEN** the operator presses the band's next-step button on an agent whose harness has that GSD skill and whose pane is ready for input
- **THEN** the host shows the exact invocation and, once accepted, writes it as that agent's harness spells it into that agent's pane only

#### Scenario: The agents HUD reads the bridge, not transcripts
- **WHEN** the `agents-hud` reference is enabled and two agents run on different harnesses
- **THEN** its status segment shows each agent's state, activity and elapsed turn time from the `agents` source, and no program reads a transcript file

#### Scenario: The flight deck watches every harness
- **WHEN** an agent on a harness that reports tool calls starts and finishes one
- **THEN** the `flightdeck` view, fed with `deliver="each"`, lists the call under that agent with its tool name and duration

#### Scenario: A meter refreshes on an interval
- **WHEN** the `gpu-meter` reference is enabled with `every "5s"`
- **THEN** its section draws a sparkline of the last answers and updates every five seconds while visible

#### Scenario: Chimes answer agent events
- **WHEN** an agent asks for an approval and the `chimes` source mapped to `agent.approval.asked` answers an envelope requesting a `notify` and a `sound`
- **THEN** one notification prefixed with the extension's attribution and one cue are emitted for that request

#### Scenario: Jump opens a picker and focuses an agent
- **WHEN** the operator presses `jump`'s global key, types part of a work label of an agent in another space and accepts
- **THEN** the host opens the overlay, filters the picker as they type, and focuses that agent's pane

#### Scenario: A plan is reviewed and answered
- **WHEN** the `plan-review` reference shows a plan as markdown and the operator submits its form with two comments
- **THEN** the comments are sent to the agent as one prompt after the operator sees its exact text

#### Scenario: A pet follows its agent
- **WHEN** the `pets` reference's sprite has frames for `idle`, `working` and `waiting` and the agent starts a turn
- **THEN** the band plays the `working` frames on the host's clock, with no program running per frame

#### Scenario: A live screen is an image
- **WHEN** the `screen` reference answers a PNG every five seconds and the operator clicks it
- **THEN** the section shows it as Sixel where the terminal supports it and as half blocks elsewhere, and the click opens it full size in an overlay

#### Scenario: Snake runs resident
- **WHEN** the `snake` reference's view is focused and the operator presses arrow keys
- **THEN** the script receives the keys, answers a cell grid each frame within its budget, and keeps its best score in its store

### Requirement: An extension's state is reported where its plugin's is
`uze status` and `uze inspect <plugin>` SHALL report each extension a plugin
carries: enabled, disabled, needing consent, or held by a widened update,
with the grants it asks for. No command SHALL enable an extension from inside
an agent's pane.

#### Scenario: A held extension is visible from the CLI
- **WHEN** an update widened an extension's grants and the operator runs `uze status`
- **THEN** the report names the extension as held, the new grant, and that it is accepted on the Extensions screen

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

### Requirement: A linked extension reloads on save
An extension installed from a linked marketplace SHALL reload when its files
change: a view or manifest change SHALL redraw from the last answers without
running a program, a script or program change SHALL rerun the visible
sources, and a widened grant set SHALL wait for consent in UZE's chrome. It
SHALL run under the same grants and confinement as any extension.

#### Scenario: Editing a view redraws at once
- **WHEN** an author changes a `gap` in a view of a linked extension and saves
- **THEN** the surface redraws with the new gap from its last answers and no program runs

#### Scenario: A linked extension gets nothing extra
- **WHEN** a linked extension's program writes outside the checkout without `write:checkout`
- **THEN** the write is denied exactly as for an extension from any other marketplace, and the inspector shows the denial

#### Scenario: Only linked extensions are watched
- **WHEN** an extension is installed from a marketplace that is not linked
- **THEN** the workspace does not watch its files

### Requirement: An error never blanks a surface
For every extension, a view, manifest, script or answer that fails SHALL leave
the surface on its last good draw with the error shown over it, naming the
file, line and column where there is one.

#### Scenario: A broken save keeps the surface
- **WHEN** an author saves a view with an unknown node
- **THEN** the surface keeps its last good draw and shows the error with file, line and column over it

### Requirement: A source may be fed by a fixture
A source MAY declare a fixture file. A source with only a fixture SHALL be fed
by it; a linked extension's inspector MAY switch a source to its fixture for
the session; an extension from a non-linked marketplace SHALL NOT read its
fixtures at runtime. An action that would run nothing SHALL be recorded in the
inspector with the arguments it would receive.

#### Scenario: A board drawn before any script exists
- **WHEN** a source declares only `fixture "fixtures/board.json"` and the author saves a change to that file
- **THEN** the surface redraws from the new fixture

#### Scenario: A button is recorded, not run
- **WHEN** the operator presses a button whose action belongs to a fixture-fed source
- **THEN** nothing runs and the inspector lists the action with the arguments it would have received

### Requirement: The inspector explains any extension
The workspace SHALL offer, for every extension's surface, an inspector showing
its runs, events, UI state and store, grant denials and confinement, the
compiled view with why a node is absent, and its cost. On a linked extension
it SHALL also rerun with edited input, emit a synthetic event, change the
slot's size and the theme, and record events into a fixture.

#### Scenario: A synthetic event reaches a linked extension
- **WHEN** the author emits `agent.turn.finished` from the inspector of a linked extension
- **THEN** the sources mapped to that event run as if an agent had finished a turn, and the event is shown as synthetic

#### Scenario: An operator finds out why a surface is empty
- **WHEN** an installed extension's section shows nothing and the operator opens its inspector
- **THEN** the tree names the node that is absent and why, such as a `when` field missing from the answer

### Requirement: An extension's tests run without a TUI
`uze agent plugin check <path>` SHALL run an extension's test files headlessly on the
host's renderer, with recorded answers by default and real programs under
confinement only on request, comparing cell snapshots per slot, size and
theme, and SHALL fail when a test fails.

#### Scenario: A gesture is tested end to end
- **WHEN** a test gives a recorded answer, selects `api`, presses `r` and accepts the confirmation
- **THEN** the test passes only if the `redeploy` action ran with `--` and `api` and the `recent` source was refreshed

#### Scenario: A visual change fails its snapshot
- **WHEN** a view's change moves a column and its snapshot was recorded before
- **THEN** the test fails showing the cell difference, and `--update-snapshots` rewrites the snapshot

#### Scenario: Tests need no program by default
- **WHEN** every test of an extension gives recorded answers
- **THEN** `uze agent plugin check` runs no program and passes on a machine without the extension's tools

### Requirement: KDL is highlighted in UZE's own editor
The code surface's editor and preview, and fenced `kdl` blocks in Markdown,
SHALL highlight KDL v2 files, including manifests, views and tests, with the
same syntax theme as every other language.

#### Scenario: An extension's view is coloured while it is edited
- **WHEN** the operator opens `extensions/gsd-band/board.kdl` in the code surface
- **THEN** node names, strings, `#true`, raw strings and slashdash comments are coloured, as a `.rs` file would be
