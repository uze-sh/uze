## Purpose

Lets an extension that needs state, raw input or per-frame drawing keep its
Rhai script alive for as long as its surface is open, reaching nothing but
the host functions its grants register, on every platform.

## ADDED Requirements

### Requirement: A resident surface keeps its script alive
A surface with `feed="resident"` SHALL keep its extension's packaged script
loaded while the surface is open, calling its `start`, `event`, `input`,
`render` and `stop` functions with a scope that survives between calls. The
script SHALL be the one whose digest was consented to; UZE SHALL NOT build
anything at install.

#### Scenario: The consented bytes are what runs
- **WHEN** a script in the Store differs from the digest recorded at consent
- **THEN** the surface does not start and the Extensions screen says the script changed since it was accepted

#### Scenario: State survives between frames
- **WHEN** a resident game's `input` moves the snake and the host then calls `render`
- **THEN** `render` sees the moved snake, kept in the script's scope between the two calls

### Requirement: A script reaches only the functions its grants register
A script SHALL be given no filesystem, network, environment, process or
module loading of its own; it SHALL reach the machine only through host
functions, each registered only under its grant (`fetch`: `net`;
`read_checkout`: `read:checkout`; `agent_prompt`: `agent:prompt`;
`run_program`: its program's grants). `eval` SHALL be unavailable.

#### Scenario: An ungranted function does not exist
- **WHEN** a script calls `fetch` and the extension's `wants` has no `net`
- **THEN** the check refuses the script naming the call and the grant it needs, and at runtime the call fails as an unknown function

#### Scenario: A script cannot load another file
- **WHEN** a script writes `import "../../secrets"`
- **THEN** the import fails, since the engine has no module resolver

#### Scenario: Confinement holds where the OS offers none
- **WHEN** a script that calls no `run_program` runs on a platform without Landlock or Seatbelt
- **THEN** it still reaches only the functions its grants register, and the Extensions screen reports it as enforced

#### Scenario: Programs it runs are reported as they are
- **WHEN** a script calls `run_program` on a platform without Landlock or Seatbelt
- **THEN** the Extensions screen reports the script as enforced and the programs it runs as not enforced

#### Scenario: A checkout read cannot leave the checkout
- **WHEN** a script calls `read_checkout` on `docs/key` and `docs` is a link to `~/.ssh`
- **THEN** the read is refused, since no link is followed and no path resolves outside the checkout

#### Scenario: The script cannot prompt on its own
- **WHEN** a script calls `agent_prompt` from a timer and the extension holds `agent:prompt` but not `agent:prompt:auto`
- **THEN** the host raises its confirmation with the exact text, and nothing is written unless the operator accepts

### Requirement: A script is bounded in operations, size and frames
Each call into a script SHALL be bounded in operations, call depth and the
size of its strings, arrays and maps, and stopped past its deadline; the
host SHALL ask it to render at most once per frame and 30 times a second. A
script exceeding a bound SHALL be stopped, its surface naming the bound,
while every other surface keeps working.

#### Scenario: A runaway loop is stopped
- **WHEN** a script's `input` handler loops forever
- **THEN** the call is stopped at its operation bound, the surface says the extension exceeded its budget, and keys keep working everywhere else

#### Scenario: A late frame keeps the last one
- **WHEN** a script's render misses the frame it was asked for
- **THEN** the surface keeps drawing its previous frame and the workspace does not wait for it

### Requirement: A script answers the catalog, never terminal bytes
A script's render SHALL answer a catalog tree or a cell grid of characters
and semantic roles, sanitized like any other extension output. Its `print`
and `debug` output SHALL go to the inspector, never to a terminal.

#### Scenario: An escape in a cell is inert
- **WHEN** a script answers a cell whose character is ESC
- **THEN** the cell is drawn blank and nothing reaches the terminal but the host's own output
