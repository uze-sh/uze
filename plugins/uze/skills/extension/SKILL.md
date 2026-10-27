---
name: extension
# Who finds this skill: the moment someone wants a surface of their own in
# the workspace, not the moment they want a plugin (that is `uze:author`).
description: Writing a declared extension a plugin carries — the `extension.kdl` manifest (surfaces, sources, actions, grants), a Rhai script or a program that answers a source, `exec:` grants and how a program is pinned and confined, the extension's store, the check loop with `.test.kdl` tests, and the consent and per-project enabling code needs before it runs. Use when adding an extension to a plugin, writing or debugging an extension's script or program, a source, an action or a key, when `uze agent plugin check` refuses an extension, or when an extension draws "needs consent", "needs enabling in this project" or "`<program>` not found".
invoke:
  model: true
  user: true
---

An extension is a surface a plugin adds to the workspace: a full-frame
`view` opened from the tab strip, or a `section` of the sidebar. It never
draws: it declares what it shows, and the workspace draws it. Its data
comes from **sources**, a fixture, a Rhai script or a program; what a person can do
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

- **`surface "<id>" slot=… view=… source=… title=…`**: `view` is a view
  file of catalog nodes bound to the source's data with `{field}` (the
  node list is in `uze:author`, *What a view can draw*, and in the language
  server's hover). `slot` is where it is drawn:

  | slot | where |
  |---|---|
  | `view` | the full frame, opened from the tab strip |
  | `section` | a block of the sidebar |
  | `drawer` | a panel beside the agent's pane |
  | `status` | one row of the sidebar's header, folded to it |
  | `band` | `rows=1`…`6` along the `place="top"` (or `"bottom"`) of each agent's pane, taken before the pane is sized, so no answer resizes the agent |
  | `overlay` | a card over the frame, opened by an `open` action, closed by Esc |
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

### Preferences

```kdl
preferences {
    number "warn-at" label="Warn at load (%)" default=70 min=1 max=100
    toggle "detail" label="Show each phase's name" default=#true
    choice "unit" "MiB" "GiB" label="Memory unit" default="MiB"
    text "title" label="Title" default="GPU"
}
```

The operator sets them on the Extensions screen. A view reads
`{pref.warn-at}`; a source that writes `preferences` inside its block is
asked with `input.preferences`. A value the declaration may not hold reads
as its default.

### Events and composed answers

- `deliver "latest"` (the default) or `"each"` inside a source says how
  the events of its `refresh-on` reach it: `input.event`
  (`{name, paths, checkout}`) holds the last of a burst, or each in turn,
  16 at most per burst, the next one saying how many were `dropped`.
- `source "log" { events 50; }` is answered by the workspace itself: the
  last events the extension saw, newest first.
- `output "view"` makes a source **composed**: its `data` is a tree of
  catalog nodes, `{"type": "Stack", "args": [...], "props": {...},
  "children": [...]}`, which the workspace validates and draws in place of
  the surface's view file (that file draws until the source answers). A
  `Button` names a declared `action` and carries its own `item`. Nothing
  in a composed tree is a reference: `{x}` there is text.

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

- The envelope may also ask for `notify: #{ title, body }` and
  `sound: "done" | "attention" | "error"`, under the `notify` grant: the
  workspace notifies through the desktop or the terminal, attributed to the
  extension and at most once every 10 seconds.
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

## A program

When a script cannot answer (a GPU, a CLI that knows what a file does
not), a source or an action runs a program: an argv, never a shell
string, its first element a file inside the extension (`./bin/meter`) or
the name of a program an `exec:` grant covers.

```kdl
wants "exec:git"

source "recent" {
    run "git" "log" "--format=%h%x09%s" "-n" "5" timeout="5s"
    output "lines"
    fields "hash" "subject"
    every "60s"
}

action "show" label="Show" on="board" {
    run "./bin/show" "--" "{hash}"
}
```

- The host runs the declaration the operator accepted, by its id, once
  per question, in the checkout's root. A `{field}` becomes one whole
  argument; one that begins an argument goes after `--`, or the check
  refuses it, so a value can never be read as a flag.
- It gets one JSON document on stdin, `{protocol, context, input}`, and
  each flat input as `UZE_IN_<NAME>` too. Its environment is the locale,
  the terminal, `PATH`, `HOME`, `TMPDIR` (its own scratch folder) and a
  Git told to run no hook, fsmonitor or helper; no token of the operator's
  reaches it.
- It answers on stdout: `json` (the default), one value or an envelope
  (`data`, `refresh`, `toast`, `notice`, `store`); or `lines`, one item a
  line, its fields named by `fields`, split at a tab or at
  `separator=","`. Exit `0` answers, `3` refuses with the reason on
  stderr (an action's notice; a source's failure), anything else fails.
- Bounds: 10 s (`timeout` lowers it), 1 MiB of stdout, 64 KiB of stderr;
  past one, the run and every process it started are ended. A program
  source is asked at most every 30 s.
- A packaged script runs as its `#!` interpreter followed by the script,
  so the interpreter needs its own grant: `#!/usr/bin/env python3` wants
  `exec:python3`. `env -S` is refused.
- From a script, `run_program(name, [args…])` runs a program its grants
  cover, the same way, and returns `#{ code, stdout, stderr }`; its
  arguments are taken as written, and the time it waits does not count
  against the script's 100 ms.

**Pinned and confined.** When the operator allows an extension, each
`exec:` name is resolved once on the machine's search path to its real
path and recorded (`nvidia-smi → /usr/lib/wsl/lib/nvidia-smi`); a run
never searches `PATH` again, and a name found only inside the project,
UZE's home or a temporary directory is refused. A name not found leaves
its source saying "`gh` not found" until the operator presses **Resolve
again** on the Extensions screen.

Where the machine confines (Linux with Landlock, macOS), a program reads
the checkout, its Git directory, its extension's files and the system,
writes only its scratch folder, opens no socket, and cannot leave its
process group. A device or a driver file it needs is declared beside its
grant, and the operator sees it:

```kdl
exec "nvidia-smi" {
    reads "/dev/dxg" "/dev/nvidiactl" "/proc/driver/nvidia"
}
```

A path under `/dev/` is spoken to (read, write, `ioctl`); any other is
read. Where the machine cannot confine, the program runs as the operator,
and allowing it says so in those words; `uze doctor` says which this
machine is.

## The store

An extension that wants `store` keeps one JSON object per project
(`store:global`: one for the machine, asked about on its own), at most
1 MiB, removed with its plugin. Any answer's envelope may carry `store`,
an object whose keys replace the store's (`null` removes one), applied in
the order answers arrive. A script reads it with `store()`; a program
source that declares `store` gets it on its input.

```rhai
let kept = store();
let history = if "history" in kept { kept.history } else { [] };
history.push(load);
#{ data: #{ history: history }, store: #{ history: history } }
```

## Grants

| grant | reaches |
|---|---|
| `read:checkout` | `read_checkout(path)` (a file's text, at most 1 MiB) and `list_checkout(path)` (`#{ name, kind }` with `kind` one of `file`, `dir`, `link`), beneath the checkout root; `..` and a link leading out are refused |
| `clipboard` | a `clipboard` action |
| `open-path` | an `open-path` action |
| `exec:<program>` | running that program, pinned and confined as above; `sh`, `python3`, `node`, `git` and the like are named to the operator as able to run anything |
| `store` · `store:global` | the extension's store, per project or for the machine |
| `notify` | a notification and a sound an answer asks for, rate-limited |

Later grants are refused by the check naming the change that brings them:
`agent:*`, `resident`, and `net`, `secret`, `edit:checkout`.
There is no `write:checkout`, and `exec:uze` and a harness's own program
are never granted. Ask for the fewest grants that do the job: each one is
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
a checkout, and the programs too where this machine confines them, their
`exec:` names resolved on the spot; elsewhere their recorded answers
stand in. Then `uze update <plugin> -m` puts the change in the
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
- Programs are asked about on a step of their own, after the other
  grants, each with where it was pinned and whether it runs confined. One
  allowed to run as the operator is asked again on every update, and one
  allowed confined is asked again if the machine stops confining.
- Both answers are the operator's: they are given on the workspace's
  Extensions screen, which shows the grants, and asked by `uze install`
  on a terminal. From an agent's pane both are refused. An agent writing
  an extension tells the person what to consent to and where, and does
  not try to answer for them.
