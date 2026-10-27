## Purpose

How a declared extension's source or action is answered by a program: what
the manifest may declare, what the host runs, what the program receives and
answers, and the bounds every run is held to.

## ADDED Requirements

### Requirement: A program is an argv the manifest declares
A `run` source or action SHALL declare an argv array whose first element is a
path relative to the extension, or a bare name its `exec:` grant covers. The
check SHALL refuse a single string holding a shell pipeline, a relative path
resolving outside the extension, a bare name without its grant, `exec:uze`,
and a harness's own name.

#### Scenario: A shell string is refused
- **WHEN** a manifest declares `run "openspec list --json | jq ."`
- **THEN** the check fails naming the source and saying a program is an argv array

#### Scenario: A bare name needs its grant
- **WHEN** a source declares `run "nvidia-smi" "--query-gpu=name"` and `wants` has no `exec:nvidia-smi`
- **THEN** the check fails naming `exec:nvidia-smi` as the grant it needs

#### Scenario: A path cannot escape the package
- **WHEN** a manifest declares `run "../../bin/tool"`, or a link inside the extension points outside it
- **THEN** the check and the host refuse the program as resolving outside the extension

### Requirement: A packaged script runs under its interpreter's grant
A packaged program whose first line is a shebang SHALL need the `exec:` grant
of the interpreter it names, and SHALL run as that interpreter's pinned path
followed by the script, so `env` never runs.

#### Scenario: A shebang names its interpreter's grant
- **WHEN** a packaged script begins with `#!/usr/bin/env python3`
- **THEN** the check requires `exec:python3`, and a run executes the pinned `python3` with the script as its argument

### Requirement: Only the accepted declaration runs, its values whole
The host SHALL run only a declaration of the manifest whose digest was
accepted, named by its id, with the argv rebuilt from it and each `{field}`
substituted as one whole argument, never re-parsed by a shell. A substituted
value beginning with `-` SHALL be refused unless `--` precedes it in the argv.

#### Scenario: A value cannot inject arguments or commands
- **WHEN** a selected item is named `x; rm -rf ~` and an action runs `["./bin/archive", "--", "{name}"]`
- **THEN** the program receives the single argument `x; rm -rf ~`

#### Scenario: A dash value cannot become a flag
- **WHEN** a declaration substitutes `{name}` with no `--` before it and the value is `--force`
- **THEN** the run is refused naming the field and the rule

### Requirement: Input is one JSON document on stdin
The host SHALL write one JSON object to the program's stdin and close it:
`protocol`, a `context` naming the checkout, the extension and the
declaration, `input` with only the fields the declaration named, and `store`
only when the declaration names it. Declared flat fields SHALL also be set as
`UZE_IN_<NAME>` variables.

#### Scenario: Only declared inputs reach the program
- **WHEN** a source declares `input file="NOTES.md"` and the store exists but the source does not name it
- **THEN** stdin carries `input: {"file": "NOTES.md"}`, no `store`, and the environment has `UZE_IN_FILE=NOTES.md`

### Requirement: Output is JSON or lines, named by the declaration
A program SHALL answer on stdout in the mode its declaration names: `json`,
the default, one JSON value or an envelope; or `lines`, one item per line,
its tab-separated fields named by `fields`. An answer that does not match its
mode SHALL be a failure drawn on the surface that asked.

#### Scenario: A shell author lists without jq
- **WHEN** a source declares `output "lines"` with `fields "name" "status"` and prints `build\tok`
- **THEN** the surface's data is `[{"name": "build", "status": "ok"}]`

#### Scenario: Output that is not JSON is a failure, drawn
- **WHEN** a `json` source prints `hello` and exits 0
- **THEN** the surface says the source answered something that is not JSON, naming it, and every other surface keeps working

### Requirement: Exit codes follow the portable hook contract
Exit `0` SHALL mean the program answered. Exit `3` SHALL be a refusal whose
reason, read from stderr, is the surface's notice. Any other exit, a signal or
a timeout SHALL be a failure that names the source and the tail of stderr.

#### Scenario: A refusal is shown as a notice
- **WHEN** an action exits 3 with `change has unchecked tasks` on stderr
- **THEN** the surface's notice reads that reason and nothing is refreshed

#### Scenario: A crash is contained
- **WHEN** a source exits 139
- **THEN** only that surface shows a failure naming the source

### Requirement: Every run is bounded and leaves nothing behind
A run SHALL be bounded at 10 seconds, 1 MiB of stdout and 64 KiB of stderr.
A manifest MAY lower the time limit and SHALL NOT raise it. A run past a bound
SHALL be stopped with every process it started, and no process of the run
SHALL remain once it answered.

#### Scenario: A program that hangs is stopped
- **WHEN** a source is still running when its time limit passes
- **THEN** the source and its children are stopped and the surface says the source timed out

#### Scenario: A flood of output is cut off
- **WHEN** a source writes more than the output ceiling
- **THEN** the run is a failure naming the bound, and the host never holds more than the ceiling

### Requirement: A program inherits no secret from the operator's environment
A program SHALL start with an environment built from an allowlist (locale,
terminal, `PATH`, `HOME`, the contract's variables, and `TMPDIR` set to its
scratch directory) and the Git variables that disarm a hostile checkout's
configuration. Nothing else from UZE's environment SHALL reach it.

#### Scenario: A token in the environment does not leak
- **WHEN** the operator's environment has `GITHUB_TOKEN` set and a source prints its environment
- **THEN** `GITHUB_TOKEN` is absent from what the source saw

### Requirement: A script may run a program it was granted
A script SHALL be able to call `run_program(name, args)` for a program its
extension holds `exec:<name>` for, run with the same pinning, environment,
bounds and confinement as a `run` source. It returns its exit code, stdout
and stderr. The script's own deadline SHALL not count the time spent in that
call.

#### Scenario: A script reads a program's output
- **WHEN** a script holding `exec:nvidia-smi` calls `run_program("nvidia-smi", ["--query-gpu=name", "--format=csv"])`
- **THEN** it receives the program's exit code and output, and the call is refused for a name the extension does not hold
