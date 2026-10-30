# Writing a declared extension

An extension is a surface a plugin adds to the workspace: a full-frame
`view` opened from the bar under the agent area, a `section` of the
sidebar, a drawer, a line in the bar. It never draws: it declares what it
shows, and the workspace draws it. What it shows and the logic behind it
live in **components**, `.uze` files holding a `<view>` (KDL) and a
`<script>` (Rhai). Its data comes from **sources**, a fixture, a
component's script or a program; what a person can do on it are
**actions**; what its scripts may reach are **grants**, which the operator
consents to before anything runs.

The plugin itself (its marketplace, scaffold, install and update loop) is
`SKILL.md`'s subject, beside this file. When there is no plugin yet,
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

surface "board" slot="view" view="board.uze" source="items" title="Notes"
surface "count" slot="section" view="count.uze" source="items" title="Notes"

source "items" {
    script "board.uze"
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

- **`surface "<id>" slot=… view=… source=… title=…`**: `view` is a
  component whose `<view>` holds catalog nodes bound to the source's data
  with `{field}` (the
  node list is in *What a view can draw*, below, and in the language
  server's hover). `slot` is where it is drawn:

  | slot | where |
  |---|---|
  | `view` | the full frame, opened from its door in the bar under the agent area |
  | `section` | a block of the sidebar |
  | `drawer` | a panel on the `side="right"` (the default), `"left"`, `"bottom"` or `"top"` of the agent area, opened from its door in the bar. Beside the pane, which gives up that room, or with `float=#true` over it, so the agent's terminal keeps its size; a floating one with `size="fit"` is a card as big as what it holds. Float what is glanced at and closed (a map, a timer), dock what is read while the agent works |
  | `status` | one row of the sidebar's header, folded to it |
  | `band` | `rows=1`…`6` along the `place="top"` (or `"bottom"`) of each agent's pane, taken before the pane is sized, so no answer resizes the agent |
  | `overlay` | a card over the frame, opened by an `open` action, closed by Esc |
  | `bar` | one line in the bar under the agent area, `align="left"` (the default) or `"right"`, folded to at most 32 cells; a click opens the view or drawer `opens="<surface>"` names |

  The bar is where everything a plugin adds stands, apart from UZE's own
  tab strip: the doors to its views and drawers on the left, then its
  items, in the order extensions were found. What is clicked is drawn
  lowercase, as UZE's own controls are (`Button`, `Tab`, `Toggle`, a form's
  submit, a door), whatever the manifest or view writes, and a test's
  `click` finds it in either case. Everything else keeps your case.
- **`source "<id>" { … }`**: exactly one of `script "x.uze"` (its
  `<script>`'s `answer` answers) or `fixture "fixtures/x.json"`. `input name="<template>" …` is what the
  script is asked with, text only. `every "<n>s|m|h"` asks again on a
  clock, at least `5s`, and only while a surface showing the source is in
  view. `refresh-on "<event>" <paths…>`, one line per event, asks again when
  `paths.changed` (a file beneath one of the paths, relative to the
  checkout) or `checkout.changed` happens; events arriving together are
  asked about once.
- **`action "<id>" label=… on="<surface>" key=… confirm=… { <does> }`**:
  the block holds exactly one of `call "<fn>"` (a function of a
  component's script: of the `on` surface's file, or `in="x.uze"`),
  `refresh "<source>" …`,
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
  `confirm`, and one that reaches an agent (`agent-prompt`,
  `agent-restart`, `focus-agent`), needs a modifier in its key, so a
  stray keystroke never reaches its question or an agent. A copy longer than one line or 256 bytes asks
  first even without `confirm`.
- **`wants`** is written once, with every grant as an argument.
- **`applies-when`**: a surface (`applies-when=".planning"`) or a source
  (`applies-when ".planning/STATE.md"` inside its block) for the
  checkouts holding that path only. A surface that does not apply is not
  offered or drawn there; a source that does not apply is never asked, and
  says so on its surfaces. Checked a while after the checkout changes, so
  a path an agent creates shows up on its own.

### Components

A `.uze` file holds a `<view>` block, a `<script>` block, or both, each at
most once. A tag stands alone on its line, unindented; outside the blocks
only blank lines and `//` comments stand. Errors point at the line in the
`.uze` file.

```
// board.uze
<view>
Stack gap=1 {
    Text "{summary}" strong=#true
    List each="items" { Text "{text}"; }
    Button "Clear done" action="clear"
}
</view>

<script>
fn answer(input, context) {
    let open = lines(read_checkout(input.file)).filter(|line| line.starts_with("- [ ] "));
    #{ data: #{ items: open.map(|line| #{ text: line.sub_string(6) }), summary: `${open.len()} open` } }
}

fn clear(input, context) {
    #{ data: (), refresh: ["items"] }
}
</script>
```

- A source's `script "board.uze"` runs its `answer(input, context)`.
- An action's `call "<fn>"` runs `<fn>(input, context)` of the `<script>`
  in the file of the surface the action is `on`; `call "<fn>" in="x.uze"`
  names another component, such as the one holding a source two surfaces
  share. A call needs consent as a script does, adds no grant, and may not
  name `answer` nor a resident's entries.
- Logic with no view of its own (a source two surfaces read) is a `.uze`
  holding only `<script>`; a view with no logic, one holding only
  `<view>`. A resident game's file holds its placeholder `<view>` and its
  `<script>`.
- The check compiles every script an extension reaches, so a syntax error
  or a function a `call` names that does not exist fails it before any
  run, and warns about a `<script>` nothing runs.

### Input in a view

```kdl
Select "Sort" id="sort" { Choice "Newest" value="new"; Choice "Oldest" value="old"; }
Field "Filter" id="query" placeholder="name"
List each="items" filter="ui.query" { Text "{text}"; }
Form action="comment" submit="Comment" item="{.}" {
    Field "Comment" id="comment"
}
```

- `Select` holds one of its `Choice`s as `ui.<id>` (its first until one is
  chosen). The host draws it as a form's choice: its options side by side,
  a click on one choosing it, or the chosen one in a box Enter or a click
  steps on when they do not fit. `show="ui.sort=old"` names a `value` it
  declares.
- `Field` is a line of text held as `ui.<id>`: a click (or Enter on it)
  types into it, Esc stops typing, Tab moves to the next input. A list's
  `filter="ui.<id>"` keeps the rows whose fields hold what was typed.
- `Form` draws its fields and a button; Enter in one of its fields or a
  click on the button runs `action` with `form.<id>` of each field, cleaned
  like any item. ←→ move focus between a view's inputs.
- An action reads what it names in `with`, and nothing else:
  `with "ui.sort" "form.*"` (or `"form.comment"`) puts `ui` and `form` in a
  script's `input` and a program's stdin, and `{form.comment}` in its
  templates. A form sent to an action whose `with` names none of its
  fields runs nothing and says so.

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

### Cells, images and sprites

Beyond text and the shapes it composes, a view may draw a grid, a picture
and a sprite. The workspace decodes, fits and writes each; an extension
never sends a colour or a byte for the terminal.

```kdl
// extension.kdl: a sheet of equal frames packaged with the extension,
// and the states that play them.
sprite "pet" sheet="pet.png" frame="16x16" columns=4 fallback="ready" {
    state "ready" frames="0-1" fps=2
    state "working" frames="2-5" fps=8
    state "done" frames="6-7" fps=2 loop=#false
}
```

```kdl
// A view.
Row gap=1 each="." {
    Sprite "pet" state="{state}" height=2
    Text "{label}"
}
Image "scratch:shot.png" alt="The device's screen" height=8
Cells rows="{board}" regions="{targets}"
```

- **`Cells`**: `rows` is a list of rows, each a string or a list of
  `{ch, role}` (one character, one of the roles); a role it does not know,
  a colour value among them, draws as the default. `regions` is a list of
  `{x, y, w, h, action, item}`: a click inside one runs that declared
  action on its item, as a `Button` does. At most 200 rows of 400 cells.
- **`Image "<name>"`**: a PNG, JPEG or GIF inside the extension
  (`"shots/home.png"`), or `"scratch:<file>"`, one its program wrote at the
  top of its `TMPDIR`, kept when the run ends and read again after each of
  the source's answers. Nothing outside the extension or that scratch is
  read; a file over 8 MiB, or past the 32 MiB of pixels an extension's
  pictures may hold, is refused and the image's first row says which.
  `height` is in rows (8 when left out); `alt` is said while it is read and
  where the terminal draws no images.
- **`Sprite "<id>"`**: plays the state `state` names, or the fallback, on
  the workspace's clock, with no run behind it. The check measures the
  sheet: its frame size divides it, `columns` fit across it, and every
  state's frames (`0-3,6`) exist. `fps` is 1 to 30; `loop=#false` holds
  the last frame.

### The agents

With `agent:observe`, an extension sees the workspace's agents:

```kdl
wants "agent:observe" "agent:prompt"

source "crew" { agents; refresh-on "agent.turn.finished"; }

action "review" label="Ask to review the plan" on="board" {
    agent-prompt "Review PLAN.md and say what is missing" target="self"
}
action "next" label="Run phase {number}" on="band" {
    agent-prompt skill="gsd:execute-phase" "{number}" target="self"
}
action "go" label="Go to agent" on="crew-view" { focus-agent target="{id}"; }
action "jump" label="Jump" on="crew-view" key="alt-j" { open "picker"; }
```

- `source … { agents; }` is answered by the workspace: every agent of the
  open spaces as `{id, harness, label, space, checkout, state, reports}`,
  `state` one of `working`, `done`, `ready`.
- `refresh-on "agent.<event>"`: `session.started`, `turn.started`,
  `turn.finished`, `tool.started`, `tool.finished`, `approval.asked`,
  `approval.resolved`, from any agent; `input.event` adds `agent`,
  `harness`, and what the harness reported (`tool`, a `path` inside the
  agent's checkout, `call`, `outcome`). A harness that reports nothing
  still has its turns, read from its pane.
- `agent-prompt` types text (or, with `skill="<plugin>:<skill>"`, the
  harness's own way of invoking that plugin's skill; a harness's built-in
  commands are not skills and cannot be reached) into the agent `target` names: `"self"` for the
  agent in front or the band's, `"{id}"` for one from the `agents` source.
  The workspace shows the exact text first, types it only into an agent
  that is ready, and refuses it otherwise: nothing is queued. One a turn,
  one a minute per extension and agent, three a minute per agent. A
  prompt may not begin with `!`, `/`, `#`, `@` or `$` unless it is a skill.
- `when="agent.turn.finished"` sends a fixed prompt on that event, with
  `agent:prompt:auto`: its text may hold no `{field}`, the operator allows
  it once per agent for a day, and F8 pauses every extension's writes.
- **Two steps**: a `run` or `call` may say what its answer is for with
  `then { agent-prompt "Review {answer.file}" target="self"; }`. The
  prompt reads the answer as `answer.*` (and the item, `form.*`), needs
  `agent:prompt`, is sent only on the gesture that ran the action, and is
  shown first like any prompt; `when=` is refused there, since its text is
  not fixed.
- `focus-agent` puts the agent in front. An `open` action naming an
  overlay with a `key` opens it from anywhere, over the agent's pane too,
  so that key must be a function key or carry ctrl or alt.

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

## A resident script

A game, or anything that keeps state between frames and reads keys as they
come, is a **resident** source: its script keeps running while a surface
showing it is in view, and is stopped when none is.

```kdl
wants "resident" "store"

surface "game" slot="view" view="game.uze" source="board" title="Counter"

source "board" { resident "game.uze"; }
```

```rhai
// The `<script>` of game.uze: up and down count, every second a tick blinks a dot, and
// the highest count is kept in the store.
fn start(context) {
    this.count = 0;
    this.best = store().best ?? 0;
    this.on = false;
    timer(1000);
    true
}

fn event(event) {
    this.on = !this.on;
    timer(1000);
    true
}

fn input(input) {
    switch input.key {
        "up" => { this.count += 1; }
        "down" => { this.count -= 1; }
        _ => { return false; }
    }
    if this.count > this.best {
        this.best = this.count;
        return #{ redraw: true, store: #{ best: this.best } };
    }
    true
}

fn render(size) {
    #{ type: "Row", props: #{ gap: 2 }, children: [
        #{ type: "Text", args: [`${this.count}`], props: #{ strong: true } },
        #{ type: "Text", args: [`best ${this.best}`], props: #{ role: "muted" } },
        #{ type: "Text", args: [if this.on { "●" } else { "○" }], props: #{ role: "accent" } },
    ] }
}
```

- What it keeps, it keeps on **`this`**, one map every call shares; a
  function sees nothing outside itself, so a constant is a function
  (`fn width() { 20 }`).
- **`render(size)`** (`#{ width, height }`) is the one it must define: it
  answers a tree of catalog nodes, as a composed answer does, and the
  surface's view file draws until the first. It is asked only after a call
  answered a redraw, at most once a drawn frame and 30 times a second; a
  render still running keeps the last frame.
- **`start(context)`** (`slot`, `size`, `locale`, `checkout`),
  **`event(event)`**, **`input(input)`** and **`stop()`** are optional.
  `start`, `event` and `input` answer `true` or `false` (redraw or not), or
  a map with `redraw` beside the envelope's `store`, `toast`, `notice`,
  `notify` and `refresh`.
- **`timer(ms)`** asks for one `event(#{ name: "timer" })`, `ms` later and
  never sooner than 34 ms; ask again from `event` to keep ticking.
- While the surface is focused, **`input`** gets every key as
  `#{ key, mods }`, spelled as the keymap spells it (`"up"`, `"a"`,
  `"space"`, `mods: ["ctrl"]`), and a press as `#{ x, y, button }`,
  relative to the surface. **Esc** lets go of it (the next Esc closes it)
  and a press takes it back; F1 and F8 stay the workspace's. None of the
  three ever reaches the script.
- `store()` is the store as the session started; what a call keeps is
  written at once. Each call has the script bounds of its own; the first
  that fails, a bound or a thrown error, stops the session, and the
  surface says why.
- A test runs no script and keeps no session: give the source a frame
  (`given "board" fixture="fixtures/frame.json"`, the tree `render`
  answers) and test what it draws.

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
| `agent:observe` | the `agents` source, `agent.*` events and `focus-agent` |
| `agent:prompt` | `agent-prompt` on a gesture, shown first |
| `agent:prompt:auto` | `agent-prompt … when=`, fixed text, allowed per agent for a day |
| `agent:restart` | `agent-restart`, always asked; not available yet |
| `resident` | a `resident` source: its script kept running while shown, `timer`, and raw keys and pointer while focused |
| `net:<host>` | `fetch` to that one host, over HTTPS, and `secret` preferences sent to it |

### Reaching a host

```kdl
wants "net:api.github.com"
preferences {
    secret "token" label="GitHub token"
}
```

```rhai
let runs = fetch(`https://api.github.com/repos/${repo}/actions/runs?branch=${branch}`,
    #{ secret: "token", header: "Authorization", scheme: "Bearer" });
if runs.status != 200 { return #{ notice: `GitHub said ${runs.status}` }; }
#{ data: runs.json.workflow_runs }
```

- `fetch(url, options)` exists only with a `net:` grant, reaches only
  `https://` on a granted host, follows no redirect and answers
  `#{ status, body, json }` (`json` when the body is JSON). Options:
  `method` (`GET` or `POST`), `headers`, `body`, and `secret`.
- A `secret` preference is typed once on the Extensions screen into a
  masked field and kept in the system's keychain. Nothing reads it back:
  `{pref.*}` and `input.preferences` never hold it, and a script names it
  only in `fetch`'s `secret` option, which sends it as a header. Its value
  is replaced with `[secret]` in every answer, and a run that sent one keeps
  no store and sends no notification.

### Calling an MCP server

When the logic already lives in an MCP server a plugin delivers to its
agents, call its tools rather than writing it again:

```kdl
wants "mcp:rtc/rtc_timesheet"
mcp "rtc" plugin="std@cardinal" timeout="5m"
```

```rhai
let month = mcp("rtc", "rtc_timesheet", #{ month: "2026-10" });
#{ data: month }
```

- One grant per tool, `mcp:<server>/<tool>`: the operator consents to each
  by name, and a tool no grant names is refused before the server hears
  of it. Never grant a server's writing tools to read with.
- The server is the extension's own plugin's, or the one an `mcp` node
  names with `plugin="<plugin>@<marketplace>"`. It is started from that
  plugin's `mcp.json` exactly as a harness starts it (a stdio server with
  no `env` or `cwd` of its own), unconfined, kept alive between calls and
  ended after five idle minutes.
- `mcp(server, tool, args)` answers the tool's structured content, or its
  text read as JSON when it is JSON, else the text. A tool's error is
  thrown; catch it with `try { … } catch (error) { … }`.
- A call may take 30 s, or what the `mcp` node's `timeout` says, up to
  `10m`. A check starts no server: give the source a fixture to test with.

`edit:checkout` is refused: no extension writes the checkout.
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
`--update-snapshots` are in *Test an extension*, below.
`uze agent plugin check --run` executes the scripts in the engine against
a checkout, and the programs too where this machine confines them, their
`exec:` names resolved on the spot; elsewhere their recorded answers
stand in. Then `uze update <plugin> -m` puts the change in the
workspace (`SKILL.md`, *Install and iterate*).

## What a view can draw

A view is built from the catalog's nodes, PascalCase, each documented in
the language server's hover:

- layout: `Box`, `Stack`, `Row`, `Grid`, `Tabs`/`Tab`, `Rule`
- text: `Text`, `Markdown`, `Code`, `Log`, `Kbd`
- data: `List`, `Table`, `Details`/`Entry`
- measures: `Progress`/`Segment` (`style="line"` for a thin line), `Gauge`, `Chart`, `Pips`, `Stat`, `Digits` (a figure in large block digits)
- status: `Badge`, `Marks`/`Mark`, `Activity`, `Elapsed` (`since` counts up; `until` counts down as a clock, `24:58`, as does a `Digits`)
- input: `Toggle`, `Button` (runs the declared action its `action` names), `Select`/`Choice`, `Field`, `Form` (values held as `ui.<id>`, a form sending `form.<id>`)
- media: `Cells`, `Image`, `Sprite` (a grid in roles, a picture, a declared sprite)
- icons: `Icon` (one of the host's icons by name, or one the manifest declares)

Colour is always a role (`muted`, `warning`, `{tone:state}`), never a
value. A state role says what kind a thing is; to say how much of it there
is, use the heat ramp, `heat-0` (none) to `heat-4` (the most), five steps of
one hue the theme keeps apart in light and dark: a contribution map, a load
chart. `ui.` and `context.` are scopes: a fixture field named `context`
is not reachable as `{context.…}`.

### A free-form area: `Box`

When the fixed nodes do not draw what you want (a card, a calendar day, a
framed panel), compose it with `Box`, a flex container the host lays out:

```kdl
lookup "ground" default="" {
    full "info"
    partial "info"
}

Box direction="row" wrap=#true gap=1 {
    Box each="days" width="19%" height=3 padding-x=1 ground="{ground:state}" {
        Row { Text "{day}" strong=#true grow=#true; Text "{hours}" strong=#true; }
        Text "{items}" truncate=#true
    }
}
```

- `direction` (`column`, the default, or `row`), `wrap` in a row, `gap`
  between children and between wrapped lines, `justify` (`start`,
  `center`, `end`, `between`).
- `width` is cells (`12`) or a share of what holds it (`"20%"`); `height`
  fixes its rows; `padding`, `padding-x`, `padding-y` keep cells free
  inside its edge; `grow` shares what is left of a row.
- `ground` fills it with a role's hue (`tone="soft"` lightens it toward the
  surface); an empty ground (`""`, a lookup's default) is none. Text on a
  ground is drawn in an ink the host picks to read on it, whatever its role.
- `border` (`line`, `round`, `heavy`) is drawn by the host inside the box,
  in `border-role` (`faint` when it names none).
- No colour value is ever accepted: `ground="#5b5bf7"` is refused. The hue
  of a role is the theme's; an operator who wants another sets it there.

### Icons: `Icon`, never a glyph

An icon is named, never typed: the operator chose a glyph set (Unicode, a
Nerd Font or ASCII) and the host draws the icon in it. A view or a lookup
table that writes an icon glyph loose (`⏱`, `🚀`, a Nerd Font codepoint)
is refused by the check; arrows, box drawing, `● ▶ ━` and accented text are
text and pass.

```kdl
Row gap=1 {
    Icon "clock" role="info"
    Text "{total}" strong=#true
}
```

The host's icons are listed in `docs/extensions/reference.md` (`clock`,
`timer`, `calendar`, `check`, `cross`, `warning`, `git-branch`,
`pull-request`, `rocket`, `bug`…). For an idea they do not hold, declare
it in `extension.kdl` with all three spellings:

```kdl
icon "kennel" nerd="\u{f0a43}" unicode="⌂" ascii="K"
```

`nerd` is one character of a Nerd Font's private use area, `unicode` one
character drawn as text (not an emoji), `ascii` one or two printable
characters. A declared name may not be one of the host's. `Icon
"{state}"` may name an icon by data: a name nothing holds draws nothing.

## Test an extension

Every `tests/*.test.kdl` runs in the check, which fails when one does. A
test opens a surface in a world, then takes the steps an operator could:

```kdl
test "the notes tab shows the notes" {
    surface "board"                       // by its id in extension.kdl
    given "board" fixture="fixtures/empty.json"  // or json=#"{ … }"#
    size 100 30                           // default: the slot's size
    theme "light"                         // default: dark
    locale "pt"                           // default: en
    now 1761570125000                     // ms since the epoch an Elapsed counts to; default: 0
    click "Notes"                         // a tab, a row: what draws this text
    press "down"                          // a key, as the keymap spells it
    press-button "refresh"
    expect-text "A fixture is a sample"
    expect-no-text "Nothing to do"
    expect-absent "List" because="ui.tab=tasks"  // or a field a `when` missed
    expect-action "refresh"               // with key=value of its item
    snapshot "board"                      // the cells, as text and colour roles
}
```

A failing step is said at its line with what was drawn. A snapshot lives
at `tests/snapshots/<name>.<slot>.<width>x<height>.<theme>.snap`; after a
change you meant, `uze agent plugin check <plugin> --update-snapshots`
writes it again, and the diff is yours to review. Tests run nothing the
plugin ships, so they pass the same on any machine.

## Point your editor at the language server

`uze agent plugin lsp` serves completion, hover and the check's own
diagnostics for `extension.kdl`, components and tests, as you type;
inside a component's `<view>` it completes and hovers as in a view, and
inside its `<script>` it answers nothing yet. Editors know no `.uze`
language, so name one:

- **Helix** (`languages.toml`): `[language-server.uze] command = "uze"`,
  `args = ["agent", "plugin", "lsp"]`, `language-servers = ["uze"]` on the
  `kdl` language, and a `[[language]] name = "uze"`, `file-types =
  ["uze"]`, `language-servers = ["uze"]`.
- **Neovim**: `vim.filetype.add({ extension = { uze = "uze" } })`, then
  `vim.lsp.start({ name = "uze", cmd = { "uze", "agent", "plugin", "lsp"
  }, root_dir = vim.fs.root(0, "extension.kdl") })` in an autocommand for
  `*.kdl` and `*.uze`.
- **VS Code**: `"files.associations": { "*.uze": "uze" }`, and any generic
  LSP client extension with the command `uze agent plugin lsp` for the
  `kdl` and `uze` languages.

## Consent and enabling

A fixture draws with no question asked. An extension that wants a grant,
has a script source, or an action that runs a script runs nothing until
the operator has **consented** to its grants and **enabled** it in the
project; until then its surfaces say `needs consent` or `needs enabling
in this project` where the data would be. Consent is recorded per
extension, by `plugin@marketplace/id`, with the grants and the digest of
the files consented to. The workspace's offer enables in the project, or
in every project when the operator chooses so; the Extensions screen
enables and disables for every project. One wanting an `agent:*` grant is
only ever enabled project by project.

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
