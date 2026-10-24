## Purpose

Lets an extension's author prove, without a terminal and without running
anything the plugin ships, that its surfaces draw and answer gestures as
intended, and keep proving it after every edit.

## ADDED Requirements

### Requirement: An extension's tests are KDL files the check runs
An extension MAY carry `tests/*.test.kdl`, each holding named `test`
blocks. `uze agent plugin check` SHALL decode them with the same bounded
parse and the same errors as views, run every test headlessly, and fail
when a test fails, naming the test, the file and the line of the step that
failed.

#### Scenario: A failing test fails the check
- **WHEN** a test expects the text `Phases` and the view draws `Fases`
- **THEN** the check fails naming the test, its file and the `expect` line, and showing the rows that were drawn

#### Scenario: A misspelled step is refused like a view's node
- **WHEN** a test writes `expect-txt "Phases"`
- **THEN** the check fails at that line naming the steps a test may use and suggesting `expect-text`

### Requirement: A test states the world it draws in
A test SHALL name the surface it opens and MAY give each source's data as
a fixture file or inline JSON, the size, the theme (`dark` or `light`) and
the locale. What a test leaves unsaid SHALL take a stated default: the
source's own fixture, the slot's default size, `dark` and `en`.

#### Scenario: A test overrides a fixture
- **WHEN** a test gives the source `milestone` the file `fixtures/empty.json`
- **THEN** the surface is drawn from that file instead of the source's declared fixture

#### Scenario: A test draws the operator's language
- **WHEN** a test sets `locale "pt"` on a view whose labels come from a lookup with a `pt` table
- **THEN** the drawn text uses the `pt` labels

### Requirement: A test drives a surface by its gestures
A test SHALL drive a surface only through what the operator can do: press
a key the surface answers, switch a tab by its label, click a node by the
text it draws, and press a button by its label. A gesture whose target is
not drawn SHALL fail the test naming what was drawn instead.

#### Scenario: Switching a tab is a gesture
- **WHEN** a test clicks the tab `Activity` and then expects the text of a log line
- **THEN** the test passes only if that line is drawn under the `Activity` tab

#### Scenario: A gesture at nothing fails
- **WHEN** a test presses the button `Deploy` and no such button is drawn
- **THEN** the test fails naming the buttons that are drawn

### Requirement: A test expects what is drawn, what is absent and what was asked
A test SHALL be able to expect a text drawn or not drawn, a node absent
for a stated reason (a `when` field missing, a `show` not matching), and
an action recorded with the arguments it would receive. Since actions run
nothing in this version, recording is the only outcome a test can expect.

#### Scenario: An absent node is expected with its reason
- **WHEN** a test expects the button `next` absent because the field `next` is missing
- **THEN** the test passes only if the inspector's trace gives that reason for that button

#### Scenario: A button records its action
- **WHEN** a test presses `▶ /gsd-discuss-phase 43` and expects the action `next` with the item's `command`
- **THEN** the test passes only if that action was recorded with that argument, and nothing ran

### Requirement: A snapshot is the cells a surface drew
A `snapshot` step SHALL compare the cells drawn, with their text and the
role of each cell, against a file beside the test, keyed by slot, size and
theme. A mismatch SHALL fail showing the rows that differ; with
`--update-snapshots` the check SHALL write the snapshot instead and say
which it wrote. A missing snapshot SHALL fail unless updating.

#### Scenario: A moved column fails its snapshot
- **WHEN** a view's edit moves a table column and its snapshot was recorded before
- **THEN** the check fails showing the differing rows, before and after

#### Scenario: Snapshots are rewritten on request
- **WHEN** the author runs `uze agent plugin check --update-snapshots`
- **THEN** every snapshot that differs or is missing is written, each named in the report, and the check passes

### Requirement: Tests run nothing a plugin ships
Running tests SHALL execute no script or program from the plugin and reach
no network, so a plugin's tests pass on any machine with UZE and give the
same cells on every platform.

#### Scenario: Tests pass on a bare machine
- **WHEN** an extension's tests run on a machine without any of the tools its future scripts would call
- **THEN** the check runs every test and gives the same result as on the author's machine
