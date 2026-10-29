## Purpose

Lets the operator give one agent's launch extra harness flags and
environment variables from the workspace, without ever displacing what UZE
composes for that launch, and include again, one by one, the flags and
variables used before.

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
prepends, before a first turn a flag or `--` carries and after a first
turn the harness reads as a bare positional word, and SHALL set an
override's variables on top of the inherited environment and under
nothing UZE sets, so an override changes the harness and never the launch
UZE composed.

#### Scenario: A resumed agent with an override
- **WHEN** an agent launched with `--model opus` is relaunched and its conversation is resumed
- **THEN** the harness receives the resume arguments first and `--model opus` after them

#### Scenario: A first turn read as a bare positional word
- **WHEN** an agent of a harness that reads its first turn as a bare positional word is launched with an override flag that takes several values
- **THEN** the harness receives the first turn before the override, so the flag never takes the turn as a value

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
harness accepts, read in the grammar of the harness's own parser, which
its catalog declares: long, short, `--flag=value`, and, where that parser
reads them so, a short flag's attached value, a short flag inside a
cluster of switches, and a long flag written with one dash.

#### Scenario: A resume flag
- **WHEN** the operator adds `--resume abc` for a harness whose continuity UZE manages
- **THEN** it is refused, saying UZE continues this agent's conversation itself

#### Scenario: The short spelling of an owned flag
- **WHEN** the operator adds the short spelling of a flag the catalog marks as UZE's
- **THEN** it is refused exactly as its long spelling is

#### Scenario: An owned flag inside a cluster of switches
- **WHEN** the operator adds a single-dash word the catalog does not spell, such as `-cd`, holding the short spelling of a flag the catalog marks as UZE's, for a harness whose parser reads such a word as a cluster
- **THEN** it is refused with that flag's reason

#### Scenario: A long owned flag written with one dash
- **WHEN** the operator adds `-continue` or `-conversation=abc` for a harness whose parser reads one dash as two, and the catalog spells that flag `--continue` or `--conversation`
- **THEN** it is refused exactly as its long spelling is

#### Scenario: A long flag written with one dash, to a parser without clusters
- **WHEN** the operator adds `-add-dir /x` for a harness whose parser reads one dash as two and has no clusters
- **THEN** it gets the verdict `--add-dir /x` gets, whatever short flags its letters spell

#### Scenario: One dash before a long name, to a parser that reads short letters
- **WHEN** the operator adds `-model opus` for a harness whose parser reads `-model` as `-m` with the value `odel`
- **THEN** the entry is read as `-m odel`, and `opus` is refused as an argument the harness would read as a prompt

#### Scenario: A configuration key UZE composes
- **WHEN** the operator adds a configuration override setting a key UZE composes on that harness's launch
- **THEN** it is refused, while a configuration override of a key UZE does not compose is accepted

#### Scenario: A flag that sets a configuration key by name
- **WHEN** the operator adds a flag the harness documents as setting a configuration key, such as `--enable memories` for `features.memories`
- **THEN** it is judged as a configuration override of that key would be

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
Every harness integration SHALL catalog every flag its latest release's
interactive `--help` defines, each with its spellings, value, accepted
values, a short description and whether it is UZE's, the operator's, or
the operator's but weakening safety or switching off delivery. Only a flag
that ends the launch or acts only in print mode SHALL be left out, named
with its reason; a flag that collides with UZE SHALL be catalogued as UZE's.

#### Scenario: The catalog completes a flag and its value
- **WHEN** the operator types `--perm` in the overrides step of a Claude Code agent, completes it and types a space
- **THEN** the flag `--permission-mode` is completed and the list offers its accepted values

#### Scenario: A flag UZE owns is not offered
- **WHEN** the operator lists the catalog in the overrides step
- **THEN** no flag the catalog marks as UZE's is offered for completion

#### Scenario: A flag that collides with UZE is catalogued and refused
- **WHEN** the operator adds `--worktree` to a Claude Code agent's launch
- **THEN** the entry is refused, saying UZE starts the agent in its own checkout, rather than passed as unknown

#### Scenario: Only meta and print-only flags are left out
- **WHEN** a harness's snapshot defines `--version` and `--output-format`, which only works with `--print`
- **THEN** both are left out of its catalog, each named with its reason, and every other flag the snapshot defines is in the catalog

#### Scenario: A release adds a flag
- **WHEN** the snapshot is refreshed and defines a flag neither catalogued nor left out
- **THEN** that harness's check fails naming the flag until it is classified

### Requirement: Every catalog is proven against its real binary
The system SHALL keep, per harness, the flags its current release accepts,
including on the form a resumed launch takes, and SHALL fail its checks
when a catalog lists a flag the harness no longer accepts.

#### Scenario: A vendor removes a flag
- **WHEN** a harness's new release no longer accepts a flag its catalog lists
- **THEN** that harness's check fails naming the flag

### Requirement: A flag that weakens safety or switches off delivery says so
The system SHALL mark, wherever an override is shown, a flag, or a flag
with a value, that weakens the harness's safety or switches off something
UZE delivers, naming what is lost, and SHALL still accept it. A launch or relaunch carrying one SHALL
say so on the agent's tab.

#### Scenario: Skipping permissions
- **WHEN** the operator adds Claude Code's flag that skips permission prompts
- **THEN** the entry is marked as weakening safety, in the popup and in the history row that later holds it

#### Scenario: A value that gives up safety
- **WHEN** the operator adds a flag whose value gives up what a skip-permissions flag would, such as Codex's `--sandbox danger-full-access`
- **THEN** the entry is marked as weakening safety, as that flag would be

#### Scenario: A configuration setting that gives up safety
- **WHEN** the operator adds a configuration override setting what such a flag stands for, such as Codex's `-c sandbox_mode="danger-full-access"`, quoted or not
- **THEN** the entry is marked as weakening safety, as the flag would be

#### Scenario: Dropping delivered MCP servers
- **WHEN** the operator adds a flag that makes the harness ignore the MCP servers UZE installed
- **THEN** the entry is accepted and marked, saying the installed MCP servers will not load

### Requirement: Recent overrides are offered again
The system SHALL remember each flag, with its value, and each variable a
launch carried as an entry of its own, per harness across projects,
keeping a bounded number, and SHALL offer each in the overrides step as a
choice the launch includes or leaves out, none marked, so Enter still
launches, those last used in this project first.

#### Scenario: A recent entry is offered per harness, this project's first
- **WHEN** the operator opens the overrides step after launches on that harness here and in another project
- **THEN** each flag and variable used is one row, left out, this project's first, another project's naming it

#### Scenario: Including and leaving out an entry
- **WHEN** the operator checks `--model opus` and leaves `--verbose` unchecked, then launches
- **THEN** the agent starts with `--model opus` and without `--verbose`

#### Scenario: The same entry twice
- **WHEN** an entry already remembered is launched again
- **THEN** the history holds it once, at the top

#### Scenario: Forgetting an entry
- **WHEN** the operator removes a remembered entry from the list
- **THEN** it is no longer offered for that harness in any project, and this launch leaves it out

#### Scenario: An unchecked entry stays listed
- **WHEN** the operator adds `--verbose` in the step and then unchecks it
- **THEN** the launch leaves it out and the list still holds it, until its close mark takes it off

#### Scenario: Space switches the marked row
- **WHEN** the operator marks a row with the arrows and presses space
- **THEN** that entry is included or left out, and with no row marked space is typed into the line

### Requirement: Overrides stay private to the operator
The system SHALL keep recorded and remembered overrides readable by the
operator alone, SHALL NOT write a value to a log or a trace, and SHALL NOT
remember in the history a variable whose name reads as a secret or whose
value holds a URL with credentials before its host.

#### Scenario: Tracing a launch with overrides
- **WHEN** a launch carrying `API_TOKEN=secret` is traced
- **THEN** the trace says how many overrides it carried and never the value

#### Scenario: A secret among remembered entries
- **WHEN** a launch carrying `API_TOKEN=secret` and `--model opus` is remembered
- **THEN** the history holds `--model opus` and not the variable

#### Scenario: Credentials in a variable's value
- **WHEN** a launch carrying `DATABASE_URL=postgres://u:pw@h/db` is remembered
- **THEN** the history does not hold the variable, whatever its name

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
