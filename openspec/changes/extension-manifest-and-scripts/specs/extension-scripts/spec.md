## ADDED Requirements

### Requirement: A source is answered by a script or a fixture
A `source` SHALL declare exactly one of `script "…"`, `run …` or
`fixture "…"`, plus an optional `input`. A script SHALL be a Rhai file in
the extension's directory whose `answer(input, context)` returns the
envelope. The check SHALL refuse `run` until programs arrive, naming the
change that brings them.

#### Scenario: A board answered by a script
- **WHEN** a source declares `script "board.rhai"` and the extension is enabled in the project
- **THEN** the surface is drawn from the `data` that script's `answer` returned

#### Scenario: A program is refused for now
- **WHEN** a source declares `run "./bin/meter"`
- **THEN** the check fails saying programs arrive in change 5

### Requirement: A script runs bounded, and reaches only what it was granted
A script SHALL run in an engine with no `eval`, no modules and no file,
network or process access of its own, bounded in operations, call depth,
expression depth, string, array and map sizes, and wall time. It SHALL
reach the checkout only through `read_checkout` and `list_checkout`, and
only with `read:checkout`.

#### Scenario: A runaway script is stopped
- **WHEN** a script loops without end
- **THEN** it is stopped at its bound and the surface shows the source failed, naming the bound

#### Scenario: An ungranted read is refused by the check
- **WHEN** a script calls `read_checkout` and the manifest does not want `read:checkout`
- **THEN** the check fails naming the function and the grant it needs

### Requirement: A checkout read cannot leave the checkout
`read_checkout` and `list_checkout` SHALL resolve paths beneath the
checkout root, SHALL refuse `..` and absolute paths, and SHALL not follow
a link out of the root.

#### Scenario: A link out is not followed
- **WHEN** a script reads `notes` and `notes` is a link to `~/.ssh/id_ed25519`
- **THEN** the read fails and nothing of the target is returned

### Requirement: The answer envelope is minimal
A script's answer SHALL be a map with `data` and optionally `refresh` (a
list of source ids), `toast` and `notice`. Every string drawn from it
SHALL pass through `Clean`. An unknown key SHALL fail the answer, said on
the surface.

#### Scenario: An action refreshes another source
- **WHEN** an action's script answers `refresh: ["board"]`
- **THEN** the `board` source is asked again and its surface redrawn

### Requirement: Actions run on a gesture, confirmed when they say so
An `action` SHALL name a label, where it is offered (`on`) and one of
`script`, `refresh`, `clipboard`, `open-path` or `open`. An action with
`confirm` SHALL ask first, showing its resolved arguments. `clipboard`
and `open-path` SHALL run only from a gesture, and `open-path` only
inside the checkout.

#### Scenario: A confirmed action shows what it will do
- **WHEN** the operator presses an action with `confirm "Archive {item.name}?"` on the row `auth`
- **THEN** a host dialog asks "Archive auth?", attributed to the extension, and nothing runs until accepted

#### Scenario: A copy shows what was copied
- **WHEN** an action copies `{item.hash}`
- **THEN** the toast shows the copied text, and a text over one line or 256 bytes asks first

#### Scenario: A key pressed on no row
- **WHEN** an action that copies or opens `{item.path}`, or asks `Archive {item.name}?`, is pressed by its key with no row selected
- **THEN** nothing runs and a toast says to select a row first, and a copy whose text resolves empty never reaches the clipboard

### Requirement: Events refresh sources, bounded by the host
A source MAY declare `every` (at least 5 seconds) and `refresh-on` with
`paths.changed` (paths beneath the checkout) or `checkout.changed`. The
host SHALL run `every` only while a surface showing the source is visible,
SHALL coalesce events in 250 ms and SHALL drop an answer to a question
that is no longer the latest.

#### Scenario: A plan file changes
- **WHEN** a source declares `refresh-on "paths.changed" ".planning"` and a file under `.planning` is saved
- **THEN** the source is asked again within about a second, once for a burst of saves

#### Scenario: A hidden surface costs nothing
- **WHEN** a source declares `every "10s"` and no surface showing it is visible
- **THEN** its script does not run

### Requirement: Answers are shared for the session, never written
The runner SHALL remember a source's last answer per (identity, source,
checkout, input digest) for the session, feed every surface that asked
the same question from one run, and write no answer to disk.

#### Scenario: Two surfaces, one run
- **WHEN** a `view` and a `section` show the same source on one checkout
- **THEN** its script runs once and both are drawn from that answer

### Requirement: A third-party surface carries its attribution
Every `view` and `section` a declared extension draws SHALL carry the
host's strip `id · plugin@marketplace`, drawn by the host from the
identity, never from the extension's name, and never covered by an
answer.

#### Scenario: A look-alike is named
- **WHEN** an extension named "Code" draws a view
- **THEN** the view's strip says `code-look · acme@github`, not "Code"

### Requirement: Rhai is highlighted in UZE's own editor
The code surface's editor and preview, and fenced `rhai` blocks in
Markdown, SHALL highlight Rhai with the same syntax theme as every other
language.

#### Scenario: A script is coloured while it is edited
- **WHEN** the operator opens an extension's `board.rhai` in the code surface
- **THEN** keywords, strings, the code inside a back-tick string's `${…}`, object maps, numbers and comments are coloured, as a `.rs` file would be
