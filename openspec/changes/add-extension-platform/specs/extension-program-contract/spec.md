## Purpose

Defines the one contract a program behind an extension answers, so that an
extension can be written in any language that reads standard input and
writes standard output, with no SDK and no resident process.

## ADDED Requirements

### Requirement: A declaration is answered by a component or a program
A source or action SHALL be answered by a WebAssembly component called once
per question (`wasm`), by a program (`run`), or, in development and tests
only, by a fixture. A component and a program SHALL receive the same input
and answer the same output and envelope, so changing which one answers SHALL
NOT change the view or the tests.

#### Scenario: A script becomes a component without touching the view
- **WHEN** an author replaces `run "./bin/board.sh"` with `wasm "bin/board.wasm"` and the component answers the same JSON
- **THEN** the surface draws the same and the extension's tests pass unchanged

#### Scenario: A component needs no exec grant
- **WHEN** an extension's sources are all components that import only `read-checkout`
- **THEN** consent asks only for `read:checkout`, and the extension is confined on every platform

#### Scenario: A component wrapping a CLI is as confined as the CLI
- **WHEN** a component imports `run-program` to run `nvidia-smi`
- **THEN** enabling asks for `exec:nvidia-smi` apart, and the Extensions screen reports the program's confinement as the platform gives it

### Requirement: A program is an argv the host runs once per question
A source, an action or a command declared by an extension SHALL be an argv
array. The host SHALL run it once per question, without a shell, with the
checkout the surface is open on as its working directory, and SHALL NOT keep
it running after it answers. A declaration that spells a program as a single
string to be split or interpreted by a shell SHALL be refused by the check.

> **Unresolved review findings** (design.md): M8, S6, S12.

#### Scenario: A source is run on demand and exits
- **WHEN** a surface needs the data of a source and no fresh answer is cached
- **THEN** the host runs the source's argv once, reads its answer, and no process of that extension remains afterwards

#### Scenario: A shell string is refused
- **WHEN** a manifest declares `run "openspec list --json | jq ."`
- **THEN** `uze agent extension check` fails naming the field and saying a program is an argv array

### Requirement: Input is one JSON document on stdin
The host SHALL write exactly one JSON object to the program's standard input
and close it. The object SHALL carry `protocol` (the contract version), a
`context` object naming where the question comes from, and an `input` object holding only the fields
the declaration named in its `input` list, taken from the selected item or
the surface's state. Nothing the declaration did not name SHALL be sent.

> **Unresolved review findings** (design.md): S8.

#### Scenario: Only declared inputs reach the program
- **WHEN** a source declares `input "change" "mode"` and the operator has selected change `support-macos` in mode `tasks`
- **THEN** the program's stdin carries `input: {"change": "support-macos", "mode": "tasks"}` and no other field of the selection

#### Scenario: The context names where the question comes from
- **WHEN** the host runs a program
- **THEN** the `context` object carries `checkout`, `branch`, `work` when the checkout has a named work, `surface`, `extension`, `slot`, the slot's `size` in cells, and `agent` when the surface sits on an agent's pane

### Requirement: Declared fields can reach a program without JSON
A declaration MAY substitute declared flat fields into its argv, each as one
whole argument, or set them as `UZE_IN_<NAME>` environment variables, so a
shell program needs no JSON tool to read its input. A substituted value
beginning with `-` SHALL be refused unless the argv puts `--` before it.

#### Scenario: A shell script reads its input from argv
- **WHEN** a source declares `run "./bin/calls.sh" "--" "{event.tool}"` and a tool event names `Bash`
- **THEN** the program receives `Bash` as its argument after `--`, with the full input still on stdin

#### Scenario: A dash value cannot become a flag
- **WHEN** a declaration substitutes `{name}` without a preceding `--` and the value is `--force`
- **THEN** the run is refused naming the field and the rule

### Requirement: Output is one JSON document on stdout
In `json` mode a program SHALL answer one JSON value on stdout: a list of
objects for a list source, an object for a document source, and for an
action nothing or an object whose `toast`, `refresh` and `store` fields the
host applies. Stderr SHALL be only a refusal's or failure's reason, bounded,
never parsed.

> **Unresolved review findings** (design.md): `output: lines` (adopted, D4).

#### Scenario: A list source feeds a navigator
- **WHEN** a list source prints `[{"name": "support-macos", "status": "in-progress"}]`
- **THEN** the navigator shows one item named `support-macos` grouped under `in-progress`

#### Scenario: Output that is not JSON is a failure, drawn
- **WHEN** a source prints `hello` and exits 0
- **THEN** the surface shows that the source answered with something that is not JSON, naming the source, and the rest of the workspace is unaffected

### Requirement: A declaration names its output mode
A source SHALL declare its output mode: `json` (the default), `lines` (one
item per line, tab-separated fields the declaration names), `view` (a
catalog tree), or `cells`, `image` or `sprite`. The host SHALL check the
answer against its mode and bound its size before decoding it, and an answer
that does not match SHALL be a failure drawn on the surface that asked.

#### Scenario: A shell author lists without jq
- **WHEN** a source declares `output "lines"` with `fields "name" "status"` and prints `build\tok`
- **THEN** the navigator shows one item named `build` with status `ok`

#### Scenario: A view answer outside the catalog is refused
- **WHEN** a `view` source answers a node of type `iframe`
- **THEN** the surface shows that the source answered a node the catalog does not have, naming it, and draws nothing of that answer

#### Scenario: An oversized image is refused before decoding
- **WHEN** an `image` source answers more bytes than the image ceiling
- **THEN** the run is a failure naming the bound, and no decoder ran on it

### Requirement: Any answer may carry requests in an envelope
A program MAY wrap its answer as `{data, store, refresh, toast, notice,
notify, sound}`, where `data` is the answer of its mode. The host SHALL
apply each request only under the extension's grants, attributed to it, and
SHALL treat a bare answer as `data` alone.

#### Scenario: A source patches the store and asks for a cue
- **WHEN** a source answers `{"data": [...], "store": {"history": [3, 5]}, "sound": "done"}` and the extension holds `notify`
- **THEN** the surface draws `data`, the store's `history` becomes `[3, 5]`, and the `done` cue is emitted once

#### Scenario: A request without its grant is dropped and named
- **WHEN** an answer requests `notify` and the extension does not hold `notify`
- **THEN** no notification is emitted, the rest of the answer applies, and the surface's notice names the missing grant

### Requirement: Exit codes follow the portable hook contract
Exit code `0` SHALL mean the program answered. Exit code `3` SHALL mean it
refused, with the reason on standard error, which the host shows as the
surface's notice. Any other exit, a signal, or a timeout SHALL be a failure,
shown on the surface that asked with the source's name and the tail of its
standard error.

#### Scenario: A refusal is shown as a notice
- **WHEN** an action exits 3 with `change has unchecked tasks` on stderr
- **THEN** the surface's notice reads that reason and nothing is refreshed

#### Scenario: A crash is contained
- **WHEN** a source exits 139
- **THEN** only that surface shows a failure naming the source, and every other surface and pane keeps working

### Requirement: Every run is bounded
Every program run SHALL be bounded by a time limit and by a maximum size of
standard output and of standard error. A program that exceeds either SHALL be
stopped together with every process it started, and the run SHALL count as a
failure naming the bound that was hit. A manifest MAY lower the bounds for
one program and SHALL NOT raise them above the host's ceiling.

> **Unresolved review findings** (design.md): S13.

#### Scenario: A program that hangs is stopped
- **WHEN** a source is still running when its time limit passes
- **THEN** the source and its children are stopped and the surface shows that the source timed out

#### Scenario: A flood of output is cut off
- **WHEN** a source writes more than the output ceiling
- **THEN** the run is stopped and counted as a failure, and the host never holds more than the ceiling in memory

### Requirement: Nothing the workspace draws waits on a program
Every program run SHALL happen off the thread that draws and reads input, and
its answer SHALL carry the question it answers, so an answer that arrives
after the viewer moved on is dropped rather than drawn. While a run is in
flight the surface SHALL keep drawing its last answer.

> **Unresolved review findings** (design.md): A1, M4.

#### Scenario: A slow source does not freeze the client
- **WHEN** a source takes five seconds to answer
- **THEN** keys and pointer gestures keep working in every pane during those five seconds

#### Scenario: A stale answer is dropped
- **WHEN** the operator selects change A, then change B before A's document arrives
- **THEN** A's document is never drawn and B's is

### Requirement: Answers are remembered until something says they changed
The host SHALL remember a source's last answer per checkout and input, and
SHALL run the source again only when a declared event, an action's `refresh`,
or an explicit refresh names it. A remembered answer SHALL be rebuildable:
deleting it costs nothing but a run.

> **Unresolved review findings** (design.md): M2.

#### Scenario: Reopening a surface costs no run
- **WHEN** the operator closes the board and opens it again with nothing changed
- **THEN** the board is drawn from the remembered answers without running any program

#### Scenario: An event refreshes what it names
- **WHEN** the checkout changes and the manifest maps `checkout.changed` to `refresh: [changes]`
- **THEN** the `changes` source runs again and no other source does
