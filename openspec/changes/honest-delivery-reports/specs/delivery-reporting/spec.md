## ADDED Requirements

### Requirement: The install report names every delivery

After installing or updating a package, the system SHALL report, for each
detected harness, the delivery route taken (the package's own envelope, a
generated envelope, or capability by capability), the reason for that route,
and every capability with the name the harness exposes it under and its
status: native, adapted (with what was not carried), shadowed (with what
shadows it), or unsupported (with the reason). The human and JSON outputs
SHALL carry the same facts.

#### Scenario: Generated envelope with agents

- **WHEN** plugin `flow` with two skills and one agent is installed and
  Claude Code is detected
- **THEN** the report names Claude Code, the generated envelope and why it was
  chosen, and lists `flow:<skill>` twice and `flow:<agent>` once as native

#### Scenario: A field the target cannot read

- **WHEN** an agent carries a frontmatter field the target harness does not
  accept
- **THEN** the report marks that agent adapted for that harness and names the
  field that was not carried

### Requirement: The effective view needs no session

The system SHALL answer, for an installed package and a harness, the list of
capabilities that harness will load and the name each is exposed under, from
local state only, without launching the harness or reaching the network.

#### Scenario: Inspecting before publishing

- **WHEN** an author runs `uze inspect flow --harness claude`
- **THEN** the output lists every skill, agent, MCP server and hook of `flow`
  with its final name, route and status for Claude Code
- **AND** the command completes without starting Claude Code

### Requirement: A failed install leaves nothing installed

When installing a package attaches it to no harness, the system SHALL remove
the package from the Store and every artifact and receipt the attempt
created, and report the failure. When the package attaches to some harnesses
and fails on others, the system SHALL keep the package, record it as
partially delivered with each failing harness and its error, and every
command that lists the package SHALL show that state until a later install or
update delivers it fully.

#### Scenario: The only harness refuses

- **WHEN** a package's install fails on the only detected harness
- **THEN** `uze status -m` does not list the package
- **AND** `uze doctor` reports no receipt for it

#### Scenario: One of two harnesses refuses

- **WHEN** a package attaches to OpenCode and fails on Claude Code
- **THEN** `uze status -m` lists the package as partially delivered, naming
  Claude Code and the error

### Requirement: Update reports what changed in the bytes

The system SHALL decide whether an update changed a package by comparing the
package's content before and after, for every source kind, and SHALL NOT
report a package as current when its Store content changed.

#### Scenario: Linked marketplace edit

- **WHEN** a skill is edited in the working tree of a linked marketplace and
  `uze update <plugin> -m` runs
- **THEN** the report says the package was updated

### Requirement: Doctor checks delivery against intent

For each installed package and detected harness, the system SHALL compare
the capabilities the delivery plan expects with what is present and readable
by the harness, and SHALL report each missing capability, each name that
differs from the expected one, and each artifact the harness would not load,
with the harness and the capability named.

#### Scenario: Empty plugin cache

- **WHEN** Claude Code's cached copy of a UZE-delivered plugin has no skills
  while the package has skills
- **THEN** `uze doctor` reports the package, Claude Code and the missing
  skills
