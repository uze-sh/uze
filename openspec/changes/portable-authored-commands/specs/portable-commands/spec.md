## Purpose

Lets a project declare its `setup` steps and `gate` as plain lines that run
with the same meaning on every platform UZE supports, in one shell built
into UZE, and says plainly when a line cannot be portable.

## ADDED Requirements

### Requirement: A plain setup or gate line runs in the portable shell on every platform

A plain `setup` step or `gate` line that the portable shell can read SHALL
run in that shell on Linux, macOS and Windows, never in the platform's own
shell. The portable shell SHALL support `&&`, `||`, `;`, pipes, input and
output redirection, leading `NAME=value` assignments, `$NAME`, `$(...)`,
globs and `~`.

#### Scenario: A gate written once

- **WHEN** `agents.yaml` declares `gate: cargo test --workspace`
- **THEN** Linux, macOS and Windows run `cargo test` with the same words

#### Scenario: A conditional setup step

- **WHEN** a setup step is `test -f .env || cp .env.example .env`
- **THEN** on every platform `.env` is copied only when it is missing

#### Scenario: A pipe and a redirection

- **WHEN** a setup step is `git log --oneline | head -n 5 > recent.txt`
- **THEN** on every platform `recent.txt` holds the five latest commits,
  and the step's status is that of the last command in the pipeline

### Requirement: Builtins behave the same on every platform

The portable shell SHALL provide `cat`, `cd`, `cp`, `echo`, `exit`,
`export`, `false`, `head`, `mkdir`, `mv`, `pwd`, `rm`, `set`, `sleep`,
`test`, `true`, `unset` and `xargs` as builtins with the same behavior on
every platform. A builtin SHALL take precedence over a program of the same
name. A line giving a builtin a flag it does not implement SHALL NOT be
portable. A flag that reaches a builtin through expansion and is not
implemented SHALL fail the command with a message naming the flag.

#### Scenario: A builtin on a machine without the tool

- **WHEN** a setup step is `mkdir -p build && cp -r assets build` on a
  Windows machine with no `mkdir` or `cp` executable
- **THEN** the step succeeds with the same result as on Linux

#### Scenario: An unsupported flag keeps the system tool

- **WHEN** a setup step is `head -1 VERSION > .version`
- **THEN** it runs in `sh` on Linux and macOS with the system's `head`
- **AND** it is reported as not portable, naming `-1`

### Requirement: Programs are found the way the platform finds them

A command that is not a builtin SHALL run the program the platform resolves
from `PATH`, including, on Windows, `PATHEXT` extensions and batch
launchers. A program not found SHALL fail that command with status 127,
naming it.

#### Scenario: A batch launcher on Windows

- **WHEN** a setup step is `pnpm install` and `pnpm` is `pnpm.cmd` on Windows
- **THEN** `pnpm install` runs with that word, as on Linux

#### Scenario: A missing program

- **WHEN** a gate is `just test` on a machine without `just`
- **THEN** the gate fails with status 127, naming `just`

### Requirement: A line the portable shell cannot read keeps its POSIX-only meaning

A plain line the portable shell cannot read, or would run differently from
POSIX sh (`${NAME}`, `echo -n`, `[`), SHALL NOT be refused. It SHALL run in the
POSIX shell on Linux and macOS as written. On Windows it SHALL be treated as
a command with no spelling for that platform.

#### Scenario: A loop in a setup step

- **WHEN** a setup step is `for f in *.example; do cp "$f" "${f%.example}"; done`
- **THEN** it runs in `sh` on Linux and macOS
- **AND** on Windows it is reported as a step with no Windows spelling

#### Scenario: A construct the portable shell would not expand

- **WHEN** a setup step is `cp .env.example ${ENV_FILE}`
- **THEN** it runs in `sh` on Linux and macOS, expanding `ENV_FILE`
- **AND** it is reported as not portable because of `${`, with `$ENV_FILE`
  as the portable spelling

### Requirement: The reason a line is not portable is reported

`uze status` SHALL name, on every platform, each plain line the portable
shell cannot read, the construct it stopped at and its position, and the
`posix`/`windows` pair as the way to cover each platform. Where `sh` runs
the line it is a note; where nothing does, it is a command with no
spelling, and the placement and delivery reports SHALL name it too.

#### Scenario: Status explains a loop

- **WHEN** `uze status` runs in a project whose setup step uses `for`
- **THEN** it names the step as not portable because of `for` at that
  position, and names the pair as the alternative

#### Scenario: Status notes a line only sh runs

- **WHEN** `uze status` runs on Linux in a project whose setup step is
  `head -1 VERSION > .version`
- **THEN** it notes that the step runs only where `sh` does, naming `-1`
- **AND** it asks for nothing: the step runs here

### Requirement: A portable line's processes are controlled as a whole

A setup step or gate run in the portable shell SHALL keep the bounds the
platform's shell has: its timeout SHALL end it and every process it
started, and its captured output SHALL stay bounded.

#### Scenario: A gate that outlives its timeout

- **WHEN** a gate is `pnpm test && pnpm e2e` and `pnpm e2e` runs past the
  gate's timeout
- **THEN** the gate is reported timed out, and no process it started is
  left running, on every platform

### Requirement: Trust identifies a command by what its author wrote

Trust and approval SHALL identify a plain line by its text as written, and
SHALL show a portable line on every platform.

#### Scenario: A portable gate on Windows asks for approval

- **WHEN** a project's `gate: make test` is first seen on Windows
- **THEN** approval shows `make test` and asks before it runs
