## Purpose

Lets the operator give one agent's launch extra harness flags and
environment variables from the workspace, without ever displacing what UZE
composes for that launch, and apply a set used before in one gesture.

## ADDED Requirements

### Requirement: A new agent can be launched with overrides
The new agent's popup SHALL let the operator add, before launching, extra
flags for the chosen harness and extra environment variables. A launch with
none SHALL be exactly the launch the workspace made before this capability,
and choosing none SHALL take no step beyond the directory and Enter.

#### Scenario: Launching with a flag and a variable
- **WHEN** the operator adds `--model opus` and `DEBUG=1` in the popup and launches
- **THEN** the harness starts with `--model opus` among its arguments and `DEBUG=1` in its environment, in the chosen directory

#### Scenario: Launching with no overrides
- **WHEN** the operator picks a directory and presses Enter without opening overrides
- **THEN** the agent starts with exactly the arguments and environment it would have had without this capability

#### Scenario: A harness with no UZE launcher
- **WHEN** the chosen harness is launched by its plain binary because UZE's launcher is not installed for it
- **THEN** the harness still receives the overrides

### Requirement: Overrides are composed after what UZE adds
The system SHALL place an override's flags after the arguments UZE
prepends and before the first turn UZE appends last, and SHALL set an
override's variables on top of the inherited environment and under
nothing UZE sets, so an override changes the harness and never the launch
UZE composed.

#### Scenario: A resumed agent with an override
- **WHEN** an agent launched with `--model opus` is relaunched and its conversation is resumed
- **THEN** the harness receives the resume arguments first and `--model opus` after them

#### Scenario: A variable the operator's shell already sets
- **WHEN** the operator's environment has `DEBUG=0` and the override says `DEBUG=1`
- **THEN** the harness sees `DEBUG=1`

### Requirement: Variables UZE owns cannot be overridden
The system SHALL refuse a variable UZE sets or depends on: every `UZE_`
name, `PATH`, `HOME`, the trace context variables, and every variable the
harness's integration declares it sets or reads to deliver and resume. The
refusal SHALL name the variable and say it is UZE's.

#### Scenario: Overriding UZE's home
- **WHEN** the operator types `UZE_HOME=/tmp/x` and presses Enter to add it
- **THEN** it is not added, it stays in the line marked as refused, and the popup says the variable is managed by UZE

#### Scenario: Overriding where a harness keeps its sessions
- **WHEN** the operator adds a variable the chosen harness's integration declares it reads to find the harness's configuration or sessions
- **THEN** it is refused with the same reason

### Requirement: Flags UZE owns cannot be overridden
The system SHALL refuse a flag the chosen harness's catalog marks as UZE's:
the session, resume, continue and first-prompt flags and every flag UZE
composes for a launch. A flag SHALL be matched in every spelling the
harness accepts: long, short, `--flag=value`, a short flag's attached
value and a short flag inside a cluster of switches.

#### Scenario: A resume flag
- **WHEN** the operator adds `--resume abc` for a harness whose continuity UZE manages
- **THEN** it is refused, saying UZE continues this agent's conversation itself

#### Scenario: The short spelling of an owned flag
- **WHEN** the operator adds the short spelling of a flag the catalog marks as UZE's
- **THEN** it is refused exactly as its long spelling is

#### Scenario: An owned flag inside a cluster of switches
- **WHEN** the operator adds a single-dash word the catalog does not spell, such as `-cd`, holding the short spelling of a flag the catalog marks as UZE's
- **THEN** it is refused with that flag's reason

#### Scenario: A configuration key UZE composes
- **WHEN** the operator adds a configuration override setting a key UZE composes on that harness's launch
- **THEN** it is refused, while a configuration override of a key UZE does not compose is accepted

### Requirement: Positional arguments are refused
The system SHALL refuse a bare argument that is not the value of the flag
before it, and a literal `--`, since the harness would read either as a
prompt or a subcommand.

#### Scenario: A stray word
- **WHEN** the operator types `--verbose fix-the-bug`
- **THEN** `--verbose` is accepted and `fix-the-bug` is refused as an argument the harness would read as a prompt

### Requirement: A flag outside the catalog is passed as typed
The system SHALL accept a flag the harness's catalog does not list, pass it
unchanged and mark it as unknown, so a release that adds a flag is usable
before the catalog learns it. A value SHALL belong to an unknown flag only
when written `--flag=value`.

#### Scenario: A flag the catalog does not know
- **WHEN** the operator adds `--brand-new-flag=x` and launches
- **THEN** the harness receives `--brand-new-flag=x` and the entry was shown as not in the catalog

### Requirement: Each harness declares a flag catalog
Every harness integration SHALL declare the flags of its harness worth
offering, each with its spellings, whether it takes a value, the values it
accepts when they are a fixed set, a short description, and whether it is
UZE's, the operator's, the operator's but weakening safety, or the
operator's but switching off something UZE delivers. The catalog SHALL
describe the harness's latest release.

#### Scenario: The catalog completes a flag and its value
- **WHEN** the operator types `--perm` in the overrides step of a Claude Code agent, completes it and types a space
- **THEN** the flag `--permission-mode` is completed and the list offers its accepted values

#### Scenario: A flag UZE owns is not offered
- **WHEN** the operator lists the catalog in the overrides step
- **THEN** no flag the catalog marks as UZE's is offered for completion

### Requirement: Every catalog is proven against its real binary
The system SHALL keep, per harness, the flags its current release accepts,
including on the form a resumed launch takes, and SHALL fail its checks
when a catalog lists a flag the harness no longer accepts.

#### Scenario: A vendor removes a flag
- **WHEN** a harness's new release no longer accepts a flag its catalog lists
- **THEN** that harness's check fails naming the flag

### Requirement: A flag that weakens safety or switches off delivery says so
The system SHALL mark, wherever an override is shown, a flag that weakens
the harness's safety or switches off something UZE delivers, naming what
is lost, and SHALL still accept it. A launch or relaunch carrying one SHALL
say so on the agent's tab.

#### Scenario: Skipping permissions
- **WHEN** the operator adds Claude Code's flag that skips permission prompts
- **THEN** the entry is marked as weakening safety, in the popup and in the history row that later holds it

#### Scenario: Dropping delivered MCP servers
- **WHEN** the operator adds a flag that makes the harness ignore the MCP servers UZE installed
- **THEN** the entry is accepted and marked, saying the installed MCP servers will not load

### Requirement: Recent overrides are offered again
The system SHALL remember each distinct set of overrides a launch carried,
per harness across projects, keeping a bounded number, and SHALL offer
them in the overrides step with none marked, so Enter still launches,
those last used in this project first. Taking one SHALL replace the
entries being edited.

#### Scenario: Applying a recent set
- **WHEN** the operator has `--verbose` entered, marks a remembered set and takes it
- **THEN** the set's flags and variables replace `--verbose` and can still be edited before launching

#### Scenario: The same set twice
- **WHEN** a set already remembered is launched again
- **THEN** the history holds it once, at the top

#### Scenario: A set from another project
- **WHEN** the operator opens the overrides step in a project where `--model opus` was never used, after using it in another project
- **THEN** `--model opus` is offered after this project's own sets, naming the project it was last used in

#### Scenario: Forgetting a set
- **WHEN** the operator removes a remembered set from the list
- **THEN** it is no longer offered for that harness in any project

### Requirement: Overrides stay private to the operator
The system SHALL keep recorded and remembered overrides readable by the
operator alone, SHALL NOT write a value to a log or a trace, and SHALL NOT
remember in the history a variable whose name reads as a secret.

#### Scenario: Tracing a launch with overrides
- **WHEN** a launch carrying `API_TOKEN=secret` is traced
- **THEN** the trace says how many overrides it carried and never the value

#### Scenario: A secret in a remembered set
- **WHEN** a launch carrying `API_TOKEN=secret` and `--model opus` is remembered
- **THEN** the history holds `--model opus` and says one variable was not kept

### Requirement: An agent keeps the overrides it was launched with
The system SHALL record an agent's overrides per harness, so a pane
respawned and a preserved agent resumed on the same harness start with the
same overrides, checked again at that launch against what UZE owns then.

#### Scenario: Resuming a preserved agent
- **WHEN** an agent launched with `--model opus` is put away and later resumed on the same harness
- **THEN** it starts again with `--model opus`

#### Scenario: A recorded override UZE now owns
- **WHEN** a recorded override names a flag a later catalog marks as UZE's
- **THEN** the agent starts without it, and its tab says the override was left out and why

#### Scenario: Launching with overrides while UZE is bypassed
- **WHEN** the launcher runs with UZE bypassed for an agent that has recorded overrides
- **THEN** the harness starts without them and the tab says they were left out
