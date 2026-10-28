# Extension protocol and catalog reference

<!-- Generated from the uze-contract crate by `cargo run --quiet -p uze-contract --features schema --example extension-docs`; do not edit. -->

The exhaustive list of what a declared extension may write and answer: every slot, source kind, grant, event, action, envelope member and catalog node. It is generated from the contract the workspace checks against, so it says what the build accepts. To learn to write an extension, read the `uze:author` Skill's extension reference (`plugins/uze/skills/author/references/extension.md`); the machine-readable form, with a worked manifest per slot and per source kind, is [`schema.json`](schema.json).

Owner: `crates/uze-contract` (`src/docs.rs`). A change to the contract changes this page in the same commit, or the root suite's drift test fails.

## Protocol

- `protocol 1`: a manifest declaring another is refused with both numbers.
- An id is lowercase letters, digits and single hyphens, starting with a letter, at most 32 long. Reserved: `code`, `architect`, `spec`, `timeline`, `uze`.

## Slots

| slot | where |
|---|---|
| `view` | The full frame, opened from the tab strip like the built-in surfaces. |
| `section` | A collapsible block of the workspace sidebar. |
| `drawer` | A panel docked beside the agent's pane, opened by a gesture. |
| `status` | One row of the sidebar's header. |
| `band` | `rows=1`…`6` along the `place="top"` (or `"bottom"`) of each agent's pane, taken before the pane is sized. |
| `overlay` | A card over the frame, opened by an `open` action and closed by Esc. |

## Sources

| kind | written | answered by |
|---|---|---|
| `script` | `script "x.rhai"` | A Rhai file whose `answer(input, context)` returns the envelope. |
| `run` | `run "<program>" "<arg>"… timeout="5s"` | A program the workspace runs, pinned and confined where the machine can; its standard output is read as `output` says. |
| `fixture` | `fixture "fixtures/x.json"` | A JSON file whose contents are the source's data. |
| `events` | `events <n>` | The workspace's own: the last `n` events the extension saw, newest first. |
| `agents` | `agents` | The workspace's own: every agent of the open spaces, under `agent:observe`. |
| `resident` | `resident "x.rhai"` | A Rhai file kept running while a surface showing it is open, under `resident`: `start`, `event`, `input`, `render`, `stop`. |

A source also takes `input name="<template>"…`, `every "<n>s|m|h"` (at least 5s), `refresh-on "<event>" <paths…>`, `output`, `deliver`, `store`, `preferences` and `applies-when "<path>"`: a source whose path the checkout lacks is never asked. A surface takes the same as a property, `applies-when="<path>"`, and is then offered only in a checkout that has it.

### `output`

| output | how standard output is read |
|---|---|
| `json` | One JSON value, or an envelope around one (the default). |
| `lines` | One item per line, its fields named in order and split at a tab or the declared separator, each trimmed. |
| `view` | A composed answer: a tree of catalog nodes the workspace validates and draws. |

### `deliver`

| deliver | how events reach the source |
|---|---|
| `latest` | Events arriving together are asked about once, with the last (the default). |
| `each` | Asked once per event, in order, at most 16 a burst; the next says how many were dropped. |

## Grants

| grant | reaches |
|---|---|
| `read:checkout` | `read_checkout(path)` and `list_checkout(path)`, beneath the checkout root. |
| `clipboard` | A `clipboard` action. |
| `open-path` | An `open-path` action. |
| `store` | The extension's store, one per project. |
| `store:global` | The extension's store, one for the machine, consented to on its own. |
| `notify` | A notification and a sound an answer asks for, rate-limited. |
| `agent:observe` | The `agents` source, `agent.*` events and `focus-agent`. |
| `agent:prompt` | `agent-prompt` on the operator's gesture, shown first. |
| `agent:prompt:auto` | `agent-prompt … when=`: fixed text sent on an agent's event, allowed per agent for a day. |
| `agent:restart` | `agent-restart`, always asked. |
| `resident` | A `resident` source: its script kept running while shown, `timer`, and raw keys and pointer while focused. |
| `exec:<program>` | Running that program, resolved once on the search path, pinned and confined. |
| `net:<host>` | `fetch` to that one host, over HTTPS, and `secret` preferences sent to it. |

## Events (`refresh-on`)

| event | when |
|---|---|
| `paths.changed` | A file beneath one of the listed paths, relative to the checkout, changed. |
| `checkout.changed` | The checkout's branch, head or working tree changed. |
| `agent.session.started` | An agent's session started. Needs `agent:observe`. |
| `agent.turn.started` | An agent began a turn. Needs `agent:observe`. |
| `agent.turn.finished` | An agent finished a turn. Needs `agent:observe`. |
| `agent.tool.started` | An agent started a tool. Needs `agent:observe`. |
| `agent.tool.finished` | An agent's tool finished. Needs `agent:observe`. |
| `agent.approval.asked` | An agent asked the operator to approve something. Needs `agent:observe`. |
| `agent.approval.resolved` | The operator answered an agent's approval. Needs `agent:observe`. |

## Actions

| does | written | what |
|---|---|---|
| `script` | `script "x.rhai"` | Runs the file's `answer` and applies its envelope. |
| `refresh` | `refresh "<source>"…` | Asks these sources again. |
| `clipboard` | `clipboard "<template>"` | Copies the text to the clipboard; under `clipboard`. |
| `open-path` | `open-path "<template>"` | Opens a file inside the checkout; under `open-path`. |
| `open` | `open "<surface>"` | Opens another surface of the extension; an overlay's `open` key must be a function key or carry ctrl or alt. |
| `run` | `run "<program>" "<arg>"…` | Runs a program, confined where the machine can, and applies the envelope it answers. |
| `agent-prompt` | `agent-prompt "<text>" target="self"` or `skill="<plugin>:<skill>"` | Types a prompt into an agent, shown first and never queued; under `agent:prompt`, or `agent:prompt:auto` with `when=`. |
| `agent-restart` | `agent-restart target="self"` | Starts an agent's harness on a new conversation, always asked; under `agent:restart`. |
| `focus-agent` | `focus-agent target="{id}"` | Puts an agent's pane in front; under `agent:observe`. |

An action is `action "<id>" label=… on="<surface>" key=… confirm=… { <does> }`. `key` is a suggestion, modifiers joined by `-` (`ctrl`, `alt`, `shift`) before a printable character or one of `enter`, `esc`, `tab`, `space`, `backspace`, `delete`, `insert`, `up`, `down`, `left`, `right`, `home`, `end`, `pageup`, `pagedown`, `f1`, `f2`, `f3`, `f4`, `f5`, `f6`, `f7`, `f8`, `f9`, `f10`, `f11`, `f12`. An action with `confirm`, and one that reaches an agent, needs a modifier. Inside its block, `with "ui.<id>" "form.<id>" "form.*"` names the UI state and form fields its input receives, and after a `run` or `script`, `then { agent-prompt "…{answer.<field>}…" target=… }` types a prompt filled from the first step's answer, on a gesture only, under `agent:prompt`.

## The envelope

| member | what |
|---|---|
| `data` | What the source's views bind; required. |
| `refresh` | Source ids to ask again. |
| `toast` | A line the workspace shows briefly. |
| `notice` | A line said on the surface until the next answer. |
| `store` | Keys of the extension's store to replace, `null` removing one; under `store`. |
| `notify` | `{title, body}`: a notification, attributed; under `notify`. |
| `sound` | A cue: `done`, `attention` or `error`; under `notify`. |

A script answers the envelope; a program answers it or a bare value, read as its `data`. Any other member fails the answer.

## Preferences

| kind | holds |
|---|---|
| `text` | Text. |
| `number` | A number, between `min` and `max` when given. |
| `toggle` | `#true` or `#false`. |
| `choice` | One of the values listed after its name. |
| `secret` | Kept in the system's keychain, read only by `fetch`'s `secret` option. |

## Catalog

A view names these nodes, PascalCase. A property's value is a literal, a `{field}` reference or text mixing both, except a `name`, which is a literal: identity never depends on data. In a composed answer every value is a literal.

### Properties every node takes

| property | value | what | composed |
|---|---|---|---|
| `each` | name | Repeat this node once per item of a list field. | no |
| `when` | name | Draw only while the field is present: not missing, null, false, empty or zero. | no |
| `unless` | name | Draw only while the field is absent. | no |
| `show` | name | Draw only while a UI value equals a declared literal: `ui.<id>=<value>`. | no |
| `show-any` | name | Draw while any of several `ui.<id>=<value>` pairs, comma-separated, holds. | no |
| `grow` | flag | Take a share of what is left of the row. | yes |
| `min-width` | count | Never draw narrower than this many columns. | yes |

### `Stack`

Its children one under another, `gap` rows apart.

- Holds: `Stack`, `Row`, `Text`, `Badge`, `Marks`, `Pips`, `Progress`, `List`, `Table`, `Tabs`, `Toggle`, `Button`, `Activity`, `Elapsed`, `Log`, `Rule`, `Details`, `Markdown`, `Code`, `Gauge`, `Chart`, `Stat`, `Kbd`, `Grid`, `Cells`, `Image`, `Sprite`, `Select`, `Field`, `Form`.

| property | value | what |
|---|---|---|
| `gap` | count | Rows (in a `Stack`) or columns (in a `Row`) between children. |

### `Row`

Its children side by side, `gap` columns apart; `grow` children share what is left, the rest are placed by `align`.

- Holds: `Stack`, `Row`, `Text`, `Badge`, `Marks`, `Pips`, `Progress`, `List`, `Table`, `Tabs`, `Toggle`, `Button`, `Activity`, `Elapsed`, `Log`, `Rule`, `Details`, `Markdown`, `Code`, `Gauge`, `Chart`, `Stat`, `Kbd`, `Grid`, `Cells`, `Image`, `Sprite`, `Select`, `Field`, `Form`.

| property | value | what |
|---|---|---|
| `gap` | count | Rows (in a `Stack`) or columns (in a `Row`) between children. |
| `align` | `start` · `center` · `end` | Where a row with nothing that grows sits: `start`, `center` or `end`. |

### `Text`

Text in a role, `strong` for emphasis; with `truncate` one line cut with an ellipsis, otherwise a paragraph that wraps.

- Arguments: text.
- Holds no other node.

| property | value | what |
|---|---|---|
| `role` | role | The semantic colour, or `{table:field}` to look one up. |
| `strong` | flag | Emphasis. |
| `truncate` | flag | One line, cut with an ellipsis, instead of a paragraph that wraps. |

### `Badge`

A short label on a ground of its role's colour.

- Arguments: text.
- Holds no other node.

| property | value | what |
|---|---|---|
| `role` | role | The semantic colour, or `{table:field}` to look one up. |

### `Marks`

Role-coloured marks in a row, one per `Mark`.

- Holds: `Mark`.

### `Mark`

One mark of a `Marks`, in its role's colour.

- Holds no other node.

| property | value | what |
|---|---|---|
| `role` | role | The semantic colour, or `{table:field}` to look one up. |

### `Pips`

`done` of `total` as a row of filled and empty pips.

- Holds no other node.

| property | value | what |
|---|---|---|
| `done` | count | How many pips are filled. |
| `total` | count | How many pips there are. |

### `Progress`

A bar: one fill at `ratio`, or one segment per `Segment`.

- Holds: `Segment`.

| property | value | what |
|---|---|---|
| `ratio` | number | How full, from 0 to 1. |
| `role` | role | The semantic colour, or `{table:field}` to look one up. |

### `Segment`

One segment of a `Progress`: as wide as its `weight`, filled to its `ratio`, in its role.

- Holds no other node.

| property | value | what |
|---|---|---|
| `weight` | number | How wide, relative to the other segments. |
| `ratio` | number | How full, from 0 to 1. |
| `role` | role | The semantic colour, or `{table:field}` to look one up. |

### `List`

Rows repeated over `each`; with an `id`, the host holds which one is selected; `filter="ui.<id>"` keeps the rows whose fields hold what a `Field` has typed.

- Holds: `Stack`, `Row`, `Text`, `Badge`, `Marks`, `Pips`, `Progress`, `List`, `Table`, `Tabs`, `Toggle`, `Button`, `Activity`, `Elapsed`, `Log`, `Rule`, `Details`, `Markdown`, `Code`, `Gauge`, `Chart`, `Stat`, `Kbd`, `Grid`, `Cells`, `Image`, `Sprite`, `Select`, `Field`, `Form`.

| property | value | what |
|---|---|---|
| `id` | name | The name the host keeps this node's state under, as `ui.<id>`. |
| `empty` | text | What to draw when there are no rows. |
| `filter` | name | `ui.<id>` of a `Field`: keep only the rows whose fields hold its text. |

### `Table`

Named columns, one argument per heading; each child `Row` is a line whose children are its cells.

- Arguments: text, optional; text, optional; text, optional; text, optional; text, optional; text, optional; text, optional; text, optional.
- Holds: `Row`.

| property | value | what |
|---|---|---|
| `empty` | text | What to draw when there are no rows. |

### `Tabs`

A choice the host holds as `ui.<id>`; `show="ui.<id>=<value>"` picks what is drawn under it.

- Holds: `Tab`.

| property | value | what |
|---|---|---|
| `id` | name | The name the host keeps this node's state under, as `ui.<id>`. |

### `Tab`

One choice of a `Tabs`: its label, and the `value` `show` compares with.

- Arguments: text.
- Holds no other node.

| property | value | what |
|---|---|---|
| `value` | name | What `show` compares the choice with, and what a chosen one is held as. |

### `Toggle`

A switch the host holds as `ui.<id>`, `on` at first when given.

- Arguments: text.
- Holds no other node.

| property | value | what |
|---|---|---|
| `id` | name | The name the host keeps this node's state under, as `ui.<id>`. |
| `on` | flag | Whether the switch starts on. |

### `Button`

A label that records its `action` with its `item` when pressed.

- Arguments: text.
- Holds no other node.

| property | value | what |
|---|---|---|
| `action` | name | The action a press records, by name. |
| `item` | any value | What the action receives: a field, or `{.}` for the whole item. |

### `Activity`

A mark the host animates while `busy`.

- Holds no other node.

| property | value | what |
|---|---|---|
| `agent` | name | The agent whose activity this mark follows. |
| `busy` | flag | Whether the mark is animated. |

### `Elapsed`

The time since `since`, ticked by the host.

- Holds no other node.

| property | value | what |
|---|---|---|
| `since` | text | A timestamp: milliseconds, or an RFC 3339 date. |
| `role` | role | The semantic colour, or `{table:field}` to look one up. |

### `Log`

Lines, each a string or an object with `text` and a `role`.

- Holds no other node.

| property | value | what |
|---|---|---|
| `lines` | any value | A list field of lines, or one string of several. |

### `Rule`

A hairline across the width, with its label when given.

- Arguments: text, optional.
- Holds no other node.

| property | value | what |
|---|---|---|
| `role` | role | The semantic colour, or `{table:field}` to look one up. |

### `Details`

Key/value pairs, one `Entry` each, with the keys in one aligned column.

- Holds: `Entry`.

### `Entry`

One pair of a `Details`: its key, then its value, in its role.

- Arguments: text; text.
- Holds no other node.

| property | value | what |
|---|---|---|
| `role` | role | The semantic colour, or `{table:field}` to look one up. |

### `Markdown`

Text written in Markdown: headings, lists, quotes, emphasis and fenced code, wrapped.

- Arguments: text.
- Holds no other node.

### `Code`

A block of code highlighted for its `language`, one row per line, cut rather than wrapped.

- Arguments: text.
- Holds no other node.

| property | value | what |
|---|---|---|
| `language` | text | The language the code is highlighted for, such as `rust` or `json`. |

### `Gauge`

A labelled meter at `ratio`, in `role` until it reaches `warning` or `danger`.

- Arguments: text, optional.
- Holds no other node.

| property | value | what |
|---|---|---|
| `ratio` | number | How full, from 0 to 1. |
| `role` | role | The semantic colour, or `{table:field}` to look one up. |
| `warning` | number | The ratio from which the gauge is drawn in the `warning` role. |
| `danger` | number | The ratio from which the gauge is drawn in the `danger` role. |

### `Chart`

A list of numbers (`series`) as one `sparkline` row or as `bar`s `height` rows tall.

- Holds no other node.

| property | value | what |
|---|---|---|
| `series` | any value | A list field of numbers. |
| `kind` | `sparkline` · `bar` | `sparkline` (one row) or `bar` (columns `height` rows tall). |
| `height` | count | How many rows it takes: a chart's bars, a picture's or a sprite's height. |
| `role` | role | The semantic colour, or `{table:field}` to look one up. |

### `Stat`

A figure and its label, with a `delta` in its role when given.

- Arguments: text; text.
- Holds no other node.

| property | value | what |
|---|---|---|
| `delta` | text | A change beside the figure, such as `+12%`, in `role`. |
| `role` | role | The semantic colour, or `{table:field}` to look one up. |

### `Kbd`

A key, drawn as the workspace draws its own keys.

- Arguments: text.
- Holds no other node.

### `Grid`

Its children as tiles, in as many columns as fit their `min-width`, row by row.

- Holds: `Stack`, `Row`, `Text`, `Badge`, `Marks`, `Pips`, `Progress`, `List`, `Table`, `Tabs`, `Toggle`, `Button`, `Activity`, `Elapsed`, `Log`, `Rule`, `Details`, `Markdown`, `Code`, `Gauge`, `Chart`, `Stat`, `Kbd`, `Grid`, `Cells`, `Image`, `Sprite`, `Select`, `Field`, `Form`.

| property | value | what |
|---|---|---|
| `gap` | count | Rows (in a `Stack`) or columns (in a `Row`) between children. |

### `Cells`

A grid of characters, `rows` a list of rows of `{ch, role}` (or plain text), with `regions` (`{x, y, w, h, action, item}`) running a declared action when clicked. Colour comes from roles only.

- Holds no other node.

| property | value | what |
|---|---|---|
| `rows` | any value | A list of rows, each a list of `{ch, role}` cells or a plain string. |
| `regions` | any value | A list of `{x, y, w, h, action, item}`: a click inside runs `action` with `item`. |

### `Image`

A picture: a PNG, JPEG or GIF inside the extension, or `scratch:<name>` its program wrote, fitted to `height` rows; `alt` where the terminal draws none.

- Arguments: text.
- Holds no other node.

| property | value | what |
|---|---|---|
| `alt` | text | What stands in for the picture where the terminal draws none. |
| `height` | count | How many rows it takes: a chart's bars, a picture's or a sprite's height. |

### `Sprite`

A declared sprite playing `state` (or its fallback), on the host's clock.

- Arguments: name.
- Holds no other node.

| property | value | what |
|---|---|---|
| `state` | text | The state to play: one of the sprite's, or its fallback otherwise. |
| `height` | count | How many rows it takes: a chart's bars, a picture's or a sprite's height. |

### `Select`

One of its `Choice`s, held by the host as `ui.<id>`, stepped with its arrows or Enter; the first until one is chosen.

- Arguments: text, optional.
- Holds: `Choice`.

| property | value | what |
|---|---|---|
| `id` | name | The name the host keeps this node's state under, as `ui.<id>`. |

### `Choice`

One option of a `Select`: its label, and the `value` held when chosen.

- Arguments: text.
- Holds no other node.

| property | value | what |
|---|---|---|
| `value` | name | What `show` compares the choice with, and what a chosen one is held as. |

### `Field`

A line of text the host holds as `ui.<id>` while the operator types into it; `placeholder` stands in while it is empty.

- Arguments: text, optional.
- Holds no other node.

| property | value | what |
|---|---|---|
| `id` | name | The name the host keeps this node's state under, as `ui.<id>`. |
| `placeholder` | text | What the field says while nothing is typed. |

### `Form`

Its `Field`s and `Select`s, and a button (`submit`) running `action` with them as `form.<id>`, cleaned like any item.

- Holds: `Stack`, `Row`, `Text`, `Badge`, `Marks`, `Pips`, `Progress`, `List`, `Table`, `Tabs`, `Toggle`, `Button`, `Activity`, `Elapsed`, `Log`, `Rule`, `Details`, `Markdown`, `Code`, `Gauge`, `Chart`, `Stat`, `Kbd`, `Grid`, `Cells`, `Image`, `Sprite`, `Select`, `Field`, `Form`.

| property | value | what |
|---|---|---|
| `action` | name | The action a press records, by name. |
| `submit` | text | The form's button label; `Send` when it names none. |
| `item` | any value | What the action receives: a field, or `{.}` for the whole item. |

### Roles

| role | when |
|---|---|
| `default` | Body text. |
| `muted` | Supporting text a reader skims past. |
| `secondary` | Labels and headings of a group. |
| `bright` | What the eye should land on first. |
| `inactive` | Something switched off or not reachable. |
| `accent` | The extension's one point of emphasis. |
| `dim` | Quieter than muted, still readable. |
| `faint` | Hints, placeholders and empty states. |
| `info` | Neutral status: a version, a count, a note. |
| `success` | Done, passing, healthy. |
| `warning` | In progress, waiting, needs attention. |
| `danger` | Failed, blocked, destructive. |

