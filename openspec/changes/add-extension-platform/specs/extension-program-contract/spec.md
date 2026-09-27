## Purpose

Defines the one contract a program behind an extension answers, so that an
extension can be written in any language that reads standard input and
writes standard output, with no SDK and no resident process.

## ADDED Requirements

### Requirement: A program is an argv the host runs once per question
A source, an action or a command declared by an extension SHALL be an argv
array. The host SHALL run it once per question, without a shell, with the
checkout the surface is open on as its working directory, and SHALL NOT keep
it running after it answers. A declaration that spells a program as a single
string to be split or interpreted by a shell SHALL be refused by the check.

> **Unresolved review finding (plan frozen 2026-09-27; see design.md):** M8 ("no process remains" cannot be tested; `setsid` escapes the process-group kill, so it narrows to the group), S6 (the working directory must be a Git worktree root, never `$HOME`, `/` or an ancestor of `$UZE_HOME`), and S12 (the check's dry run executes untrusted programs).

#### Scenario: A source is run on demand and exits
- **WHEN** a surface needs the data of a source and no fresh answer is cached
- **THEN** the host runs the source's argv once, reads its answer, and no process of that extension remains afterwards

#### Scenario: A shell string is refused
- **WHEN** a manifest declares `run: "openspec list --json | jq ."`
- **THEN** `uze agent extension check` fails naming the field and saying a program is an argv array

### Requirement: Input is one JSON document on stdin
The host SHALL write exactly one JSON object to the program's standard input
and close it. The object SHALL carry `protocol` (the contract version), a
`context` object (`checkout`, `branch`, `work` when the checkout has a named
work, `surface`, `extension`), and an `input` object holding only the fields
the declaration named in its `input` list, taken from the selected item or
the surface's state. Nothing the declaration did not name SHALL be sent.

> **Unresolved review finding (plan frozen 2026-09-27; see design.md):** S8 (a substituted value starting with `-` is an argument-injection vector wherever inputs reach argv).

#### Scenario: Only declared inputs reach the program
- **WHEN** a source declares `input: [change, mode]` and the operator has selected change `support-macos` in mode `tasks`
- **THEN** the program's stdin carries `input: {"change": "support-macos", "mode": "tasks"}` and no other field of the selection

### Requirement: Output is one JSON document on stdout
A program SHALL answer with one JSON value on standard output: a list of
objects for a list-shaped source, an object for a document-shaped source,
and for an action either nothing or an object whose optional `toast` and
`refresh` fields the host applies. Standard error SHALL be kept only as the
reason of a refusal or failure, bounded, and never parsed as an answer.

> **Unresolved review finding (plan frozen 2026-09-27; see design.md):** UX (`sh` authors cannot produce correct JSON without `jq`; an `output: lines` or JSONL mode is proposed).

#### Scenario: A list source feeds a navigator
- **WHEN** a list source prints `[{"name": "support-macos", "status": "in-progress"}]`
- **THEN** the navigator shows one item named `support-macos` grouped under `in-progress`

#### Scenario: Output that is not JSON is a failure, drawn
- **WHEN** a source prints `hello` and exits 0
- **THEN** the surface shows that the source answered with something that is not JSON, naming the source, and the rest of the workspace is unaffected

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

> **Unresolved review finding (plan frozen 2026-09-27; see design.md):** S13 (time and output bounds do not bound PIDs, CPU, disk or descendants; a global cap on concurrent runs and a debounce floor are missing).

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

> **Unresolved review finding (plan frozen 2026-09-27; see design.md):** A1 (job ordering, one read at a time, panic fallbacks and answers orphaned by a closed surface are orchestrator policy the contract does not yet express) and M4 (building views from large answers must not happen on the render thread).

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

> **Unresolved review finding (plan frozen 2026-09-27; see design.md):** M2 (a disk cache is not allowed from `uze-extensions` and cannot be invalidated across restarts; an in-memory cache per session is proposed, which changes this requirement's "rebuildable" wording).

#### Scenario: Reopening a surface costs no run
- **WHEN** the operator closes the board and opens it again with nothing changed
- **THEN** the board is drawn from the remembered answers without running any program

#### Scenario: An event refreshes what it names
- **WHEN** the checkout changes and the manifest maps `checkout.changed` to `refresh: [changes]`
- **THEN** the `changes` source runs again and no other source does
