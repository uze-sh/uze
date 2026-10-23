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
`actions`, `events`, `pipes`, `preferences`, `commands` and `wants`. Values in the manifest SHALL be
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

### Requirement: A surface occupies one of the host's slots
A surface SHALL declare one slot: `view` (the full frame), `section` (a
sidebar block), `status` (a row in the sidebar's header), `band` (1 to 6
rows the manifest fixes, on an edge of an agent's pane), `overlay` (a
picker, form or dialog) or `notify`. The host SHALL own every slot's
geometry, SHALL NOT let a surface cover a pane's cells, and SHALL refuse an
unknown slot or `kind: interactive`.

#### Scenario: A section joins the sidebar
- **WHEN** an enabled extension declares a `section` surface
- **THEN** the workspace sidebar shows it as a collapsible block titled by the extension, drawn from its source

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
- **WHEN** a manifest declares `kind: interactive`
- **THEN** it is refused with a reason pointing at `feed: resident`

### Requirement: A surface declares how it is fed
A surface SHALL declare a `feed`: `bind` (manifest nodes filled from a
source's data), `compose` (a source answers a catalog tree), `cells`,
`image` or `sprite` (a source answers that medium), or `resident` (a
WebAssembly component, under the `resident` grant). Changing a surface's feed
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
- **WHEN** a surface declares `feed: resident` and `wants` has no `resident`
- **THEN** the check fails naming the surface and the missing grant

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
- **THEN** each node is one of layout (`stack`, `row`, `split`, `tabs`, `gap`, `rule`), text (`text`, `markdown`, `code`, `log`), data (`list`, `table`, `details`, `grid`), measure (`progress`, `gauge`, `pips`, `chart`, `heatmap`), status (`badge`, `marks`, `chip`, `activity`, `elapsed`), media (`image`, `sprite`), `cells` or input (`button`, `toggle`, `select`, `field`, `form`, `picker`), and an unknown node is refused naming the families

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
`pipe`, `open`, `focus_agent`, an agent action). An item's action SHALL appear in
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
- **WHEN** a source declares `agent.tool.finished` with `deliver: each` and three tool calls finish within the coalescing window
- **THEN** the source runs three times, in order, each with that event in `input.event`

#### Scenario: An interval has a floor and pauses when unseen
- **WHEN** a manifest declares `every: 1s` for a source shown only in a folded section
- **THEN** the check reports the interval raised to the 5-second floor, and the source does not run while nothing that shows it is visible

### Requirement: Pipes carry data into an extension from outside
`uze ext pipe`, another extension's `pipe` action, or a hook SHALL deliver
a named message, with its sender, to an extension that declares that pipe.
A pipe SHALL only refresh the sources it maps to, its payload sanitized and
bounded in `input.pipe`; it SHALL never run an action. A pipe from another
extension SHALL be accepted only from a sender the receiver names.

#### Scenario: A pipe refreshes a source with its payload
- **WHEN** `uze ext pipe chimes play '{"cue": "done"}'` is run and `chimes` maps the pipe `play` to its `cue` source
- **THEN** the `cue` source runs once with `{"cue": "done"}` in its input

#### Scenario: A pipe from an unnamed extension is refused
- **WHEN** extension `a` pipes to `b` and `b`'s manifest does not name `a` as a sender
- **THEN** nothing of `b` runs and `a` is told the sender is not accepted

#### Scenario: An undeclared pipe is refused
- **WHEN** a message names a pipe the extension does not declare
- **THEN** nothing of the extension runs and the sender is told the pipe is unknown

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
UZE's plain records, never shown again, logged, piped, cached under, or
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
- **THEN** the `flightdeck` view, fed with `deliver: each`, lists the call under that agent with its tool name and duration

#### Scenario: A meter refreshes on an interval
- **WHEN** the `gpu-meter` reference is enabled with `every: 5s`
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
- **THEN** the component receives the keys, answers a cell grid each frame within its budget, and keeps its best score in its store

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
