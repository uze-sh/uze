---
name: extension
# Who finds this skill: the moment someone wants a surface of their own in
# the workspace, not the moment they want a plugin (that is `uze:author`).
description: Writing a declared extension a plugin carries — the `extension.kdl` manifest (surfaces, sources, actions, grants), a Rhai script that answers a source, the grants this version offers, the check loop with `.test.kdl` tests, and the consent and per-project enabling a script needs before it runs. Use when adding an extension to a plugin, writing or debugging an extension's script, a source, an action or a key, when `uze agent plugin check` refuses an extension, or when an extension draws "needs consent" or "needs enabling in this project".
invoke:
  model: true
  user: true
---

An extension is a surface a plugin adds to the workspace: a full-frame
`view` opened from the tab strip, or a `section` of the sidebar. It never
draws: it declares what it shows, and the workspace draws it. Its data
comes from **sources**, a fixture or a Rhai script; what a person can do
on it are **actions**; what its scripts may reach are **grants**, which
the operator consents to before anything runs.

The plugin itself (its marketplace, scaffold, install and update loop) is
the `uze:author` Skill's subject. Start there when there is no plugin yet;
`uze agent plugin create <name> --market <market> --extension <id>`
scaffolds an extension with a fixture source and a script source that
already pass the check.

## The manifest

`extensions/<id>/extension.kdl`, every word kebab-case, every path
relative to the extension's directory and inside it:

```kdl
protocol 1
id "notes"
name "Notes"
description "The open items of the checkout's NOTES.md"

wants "read:checkout" "clipboard"

surface "board" slot="view" view="board.kdl" source="items" title="Notes"
surface "count" slot="section" view="count.kdl" source="items" title="Notes"

source "items" {
    script "items.rhai"
    input file="NOTES.md"
    every "30s"
    refresh-on "paths.changed" "NOTES.md"
}

action "refresh" label="Refresh" on="board" key="ctrl-r" {
    refresh "items"
}

action "copy" label="Copy" on="board" key="ctrl-y" confirm="Copy {text}?" {
    clipboard "{text}"
}
```

- **`surface "<id>" slot=… view=… source=… title=…`**: `slot` is `view`
  or `section`; `view` is a view file of catalog nodes bound to the
  source's data with `{field}` (the node list is in `uze:author`, *What a
  view can draw*, and in the language server's hover).
- **`source "<id>" { … }`**: exactly one of `script "x.rhai"` or
  `fixture "fixtures/x.json"`. `input name="<template>" …` is what the
  script is asked with, text only. `every "<n>s|m|h"` asks again on a
  clock, at least `5s`, and only while a surface showing the source is in
  view. `refresh-on "<event>" <paths…>`, one line per event, asks again when
  `paths.changed` (a file beneath one of the paths, relative to the
  checkout) or `checkout.changed` happens; events arriving together are
  asked about once.
- **`action "<id>" label=… on="<surface>" key=… confirm=… { <does> }`**:
  the block holds exactly one of `script "x.rhai"`, `refresh "<source>" …`,
  `clipboard "<template>"`, `open-path "<template>"` (a path inside the
  checkout) or `open "<surface>"`. A `Button` whose `action` is the
  action's id runs it wherever it is drawn; `key` runs it while its `on`
  surface is in front, on the row selected in that surface's first `List`.
- **Templates** (`input`, `confirm`, `clipboard`, `open-path`) read the
  item acted on: `{text}` and `{item.text}` are the same field, beside
  `ui.*` and `context.{checkout,slot,locale}`.
- **`key`** spells modifiers joined by `-`: `ctrl-r`, `ctrl-shift-r`,
  `f5`. It is a suggestion: a key the workspace or the operator already
  holds keeps theirs, and the operator can rebind it. An action with
  `confirm` needs a modifier in its key, so a stray keystroke never
  reaches its question. A copy longer than one line or 256 bytes asks
  first even without `confirm`.
- **`wants`** is written once, with every grant as an argument.

## A script

A source's script is a Rhai file defining `fn answer(input, context)`. It
returns the **envelope**: a map with `data`, what the surface binds, and
optionally `notice` (a line said on the surface), `toast`, and `refresh`
(a list of sources to ask again).

```rhai
// The `items` source: every line of the notes file that opens with `- [ ] `.
fn answer(input, context) {
    let open = [];
    for line in lines(read_checkout(input.file)) {
        if line.starts_with("- [ ] ") {
            open.push(#{ text: line.sub_string(6) });
        }
    }
    #{
        data: #{ items: open, count: open.len() },
        notice: `${open.len()} open in ${input.file}`,
    }
}
```

- `input` is the source's declared `input`, templates resolved; an
  action's script is asked with `#{ item: <the item acted on> }`, or a
  null item when none was.
- `context` holds `checkout` (the root of the checkout the surface is
  about), `extension`, and `source` or `action`.
- Every script has `lines`, `glob`, `glob_match`, `regex`, `regex_match`,
  `json_parse`, `json_stringify`, `front_matter` and `clock()`
  (milliseconds since the epoch). Everything else of the machine is a host
  function its grant registers; a script naming one without the grant
  fails the check.
- No `import`, `export` or `eval`, and bounded: one million operations,
  100 ms, 32 call levels, strings of 1 MiB, arrays and maps of 10,000. A
  script past a bound is stopped and the surface says which bound.
  `print` and `debug` go to the trace the inspector shows.
- What a source's script answered is remembered for the session by its
  input and checkout; `every`, `refresh-on` and a `refresh` action ask
  past it. An action's script runs on every gesture.

## Grants

This version offers three:

| grant | reaches |
|---|---|
| `read:checkout` | `read_checkout(path)` (a file's text, at most 1 MiB) and `list_checkout(path)` (`#{ name, kind }` with `kind` one of `file`, `dir`, `link`), beneath the checkout root; `..` and a link leading out are refused |
| `clipboard` | a `clipboard` action |
| `open-path` | an `open-path` action |

Later grants are refused by the check naming the change that brings them:
`exec:*` and `store` (programs and an extension's own store), `agent:*`
and `notify`, `resident`, and `net`, `secret`, `edit:checkout`. There is
no `write:checkout`. A `run` source is refused the same way until
programs arrive. Ask for the fewest grants that do the job: each one is
what the operator is asked about.

## The check loop

```bash
uze agent plugin check <the plugin's directory>
```

It compiles every extension with the rules the workspace hosts it with,
scans every script for a host function its grants do not register, binds
each surface's view to its fixture, and runs every `tests/*.test.kdl`.
Each finding is `file:line:column: message (suggestion)`; fix it and check
again. A test runs nothing the plugin ships, so a surface fed by a script
is given its data from a fixture:

```kdl
test "the board lists the open items" {
    surface "board"
    given "items" fixture="fixtures/items.json"
    expect-text "Write the script"
}

test "refresh is the action its key presses" {
    surface "board"
    given "items" fixture="fixtures/items.json"
    press "ctrl+r"
    expect-action "refresh"
}
```

`press` spells a key the way the keymap does (`ctrl+r`), not the way the
manifest suggests one. The rest of the test vocabulary, snapshots and
`--update-snapshots` are in `uze:author`, *Test an extension*.
`uze agent plugin check --run` executes the scripts in the engine against
a checkout. Then `uze update <plugin> -m` puts the change in the
workspace (`uze:author`, *Install and iterate*).

## Consent and enabling

A fixture draws with no question asked. An extension that wants a grant,
has a script source, or an action that runs a script runs nothing until
the operator has **consented** to its grants and **enabled** it in the
project; until then its surfaces say `needs consent` or `needs enabling
in this project` where the data would be. Consent is recorded per
extension, by `plugin@marketplace/id`, with the grants and the digest of
the files consented to; enabling is per project, and so is disabling.

- An update that wants the same grants or fewer is applied without
  asking. One that wants a new grant is **held** (`held: its update wants
  …`) and runs nothing until the operator consents again.
- Both answers are the operator's: they are given on the workspace's
  Extensions screen, which shows the grants, and asked by `uze install`
  on a terminal. From an agent's pane both are refused. An agent writing
  an extension tells the person what to consent to and where, and does
  not try to answer for them.
