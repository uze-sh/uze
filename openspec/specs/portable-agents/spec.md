# portable-agents Specification

## Purpose
Make package-defined agent profiles portable across UZE's supported harnesses
while preserving canonical source bytes and clearly reporting any adaptation.

## Requirements

### Requirement: Canonical package agent discovery

The system SHALL recognize every Markdown file below an installed package's
`agents/` directory, subdirectories included, as an Agent capability,
preserve the definition's bytes in the Store, and compose it independently
from Skill, MCP, instruction, and hook capabilities. The agent's logical name
is its subdirectories below `agents/` joined with colons, then its
frontmatter `name`, or the file stem when there is none.

#### Scenario: Package contains an agent definition

- **WHEN** a package contains `agents/reviewer.md`
- **THEN** inspection reports one Agent capability named `reviewer` with the
  canonical file as its source

#### Scenario: Agent in a subdirectory

- **WHEN** a package contains `agents/review/security.md` with no frontmatter
  `name`
- **THEN** its logical name is `review:security`

#### Scenario: Package has no agent directory

- **WHEN** a package contains no `agents/` directory
- **THEN** its existing capabilities and delivery behavior remain unchanged

### Requirement: Per-harness agent delivery

The system SHALL expose each canonical Agent independently to every supported
harness under the label `<plugin>:<logical name>`, the same qualified label
Skills use (ADR-026). Where the harness's plugin mechanism namespaces agents,
the agent SHALL be delivered inside the package's plugin; where the harness
reads a global agent root, the delivered file SHALL be a regular file named
with the label. Two packages declaring an agent of
the same name SHALL both be delivered, each under its own label.

#### Scenario: Native harness receives an agent

- **WHEN** plugin `flow` with `agents/mr-guard.md` is attached to Claude Code,
  OpenCode, or Antigravity CLI
- **THEN** the harness discovers an agent selectable as `flow:mr-guard`
  through its documented native agent surface
- **AND** no agent named `mr-guard` alone is exposed

#### Scenario: Same agent name in two plugins

- **WHEN** plugins `flow` and `forge` both declare `agents/architect.md`
- **THEN** both install, exposed as `flow:architect` and `forge:architect`

#### Scenario: Codex receives an agent

- **WHEN** a package with an Agent capability is attached to Codex
- **THEN** UZE writes a generated standalone TOML custom-agent file in
  `~/.codex/agents/` carrying the label as `name`, the description, and the
  body as `developer_instructions`, without modifying Store bytes
- **AND** the route is Native when every authored root field was carried, and
  Degraded naming each root field that was not

### Requirement: Only the portable frontmatter subset is projected

The system SHALL treat `name` (replaced by the label) and `description` as the
portable frontmatter root, and the body as the prompt.
Authored root fields other than `name` and `description` SHALL reach only
Claude Code, since the canonical format is Claude's; on every other harness
they are not carried. A root field that is not carried SHALL be named in the route
evidence and SHALL make the route Degraded for that harness.

#### Scenario: Claude-style fields reach OpenCode

- **WHEN** an agent declares `model: haiku` and `tools: Read, Grep` at the
  root and is delivered to OpenCode
- **THEN** OpenCode lists the agent, its file carrying neither field
- **AND** the route is Degraded with evidence naming `model` and `tools` as
  not carried to this harness

### Requirement: A field a harness spells differently goes in its own block

A field a harness spells differently SHALL be written under a `harness:` block
keyed by the harness id or one of its aliases (`claude-code`, `codex`,
`opencode`, `antigravity`/`agy`); each integration renders only its own
block's fields into its delivered file and never delivers the block itself.

#### Scenario: Each harness receives its own block

- **WHEN** an agent's `harness:` block gives `claude-code`, `codex`,
  `opencode`, and `agy` each its own `model`
- **THEN** each harness's delivered file carries the model its own block
  names, and no delivered file carries the `harness:` block

### Requirement: A block field is delivered only in a shape the harness accepts

Each integration declares which block fields it knows and the shape each must
have. A known field of the wrong shape SHALL NOT be delivered, a field the
harness drops an agent over SHALL be left out, and an unknown field SHALL be
carried as written on a harness that tolerates unknown fields and left out on
one that refuses them (Codex).

#### Scenario: A known field of the wrong shape

- **WHEN** an agent's `opencode` block gives `tools` as a list rather than a
  map
- **THEN** the file delivered to OpenCode does not carry `tools`

#### Scenario: An unknown field on a harness that refuses them

- **WHEN** an agent's `codex` block carries a field the Codex integration does
  not know
- **THEN** the file delivered to Codex leaves that field out

### Requirement: Agent definitions are checked before install

`uze agent plugin check`, and `uze agent market check` for each plugin it
lists, SHALL validate every agent definition below `agents/`: UTF-8,
parseable YAML frontmatter, a non-empty `description`, every part of the
agent's label obeying the package name rule, and a `harness:` block that is a
mapping of mappings redefining no identity field. A failing agent SHALL be
reported with its path and the fault.

#### Scenario: Agent without frontmatter

- **WHEN** a plugin contains `agents/reviewer.md` with no frontmatter
- **THEN** `uze agent plugin check` reports `agents/reviewer.md` and the
  missing frontmatter

### Requirement: Each integration checks its own harness block

Each integration's check SHALL report a block field of the wrong shape as an
error and a field it leaves out as a warning.

#### Scenario: A block field of the wrong shape fails the check

- **WHEN** an agent's `opencode` block gives `tools` as a list
- **THEN** `uze agent plugin check` reports an error naming the agent's path
  and `tools`

#### Scenario: A block field the harness leaves out warns

- **WHEN** an agent's block carries a field its integration leaves out
- **THEN** `uze agent plugin check` reports a warning naming the field and
  still passes

### Requirement: Agent lifecycle safety

The system SHALL record every UZE-managed agent artifact in a typed receipt
that carries its content, inspect it before destructive detach, and block
removal when the artifact or receipt has drifted.

#### Scenario: Cleanly detached agent

- **WHEN** an attached Agent artifact still matches its receipt
- **THEN** removing its package removes that artifact and leaves Store bytes
  and unrelated harness artifacts intact

#### Scenario: Drifted agent artifact

- **WHEN** a managed Agent artifact differs from its receipt
- **THEN** removal is blocked without deleting the drifted artifact

### Requirement: Capability support reporting

The system SHALL show an Agents row in the TUI and in the generated harness
matrix (`web/content/docs/reference/harnesses.mdx` and
`web/lib/harness-matrix.json`, written by `uze-harness-matrix`) using the same
Native, Adapted, Degraded, Unsupported, and roadmap semantics used for other
capabilities. The matrix cell is the route the integration declares for the
capability; a field one agent loses is reported on that agent's route at
install, not in the matrix.

#### Scenario: Matrix generation

- **WHEN** the support matrix is generated after agent delivery is available
- **THEN** each harness's Agents cell is derived from its integration's
  declared Agent route
- **AND** `uze-harness-matrix --check` fails when the committed matrix
  differs from the generated one

### Requirement: Real-harness conformance evidence

The conformance lab SHALL verify, without Internet access or provider tokens,
that each harness offers every fixture agent to the model under its label and
runs it when dispatched by that label.

#### Scenario: Conformance execution

- **WHEN** the agent conformance scenarios execute for a supported harness
- **THEN** each fixture agent's label appears in a request the harness sent
  to the model, and dispatching that label puts the agent's body in a model
  request
- **AND** a missing label or body is a failed check, unless the harness
  declares that part unsupported with a reason

### Requirement: A project's own agents reach the harness, in that project alone
Every Markdown file in a project's `.agents/agents` SHALL be offered to
each supported harness as a subagent named by its frontmatter `name` or
file stem, in sessions of that project only, read by the harness itself or
projected by UZE with a package agent's portable subset, and never by
writing into the checkout.

#### Scenario: Offered in its own project
- **WHEN** a project holds `.agents/agents/reviewer.md` and a session starts in that project on any supported harness
- **THEN** the harness offers `reviewer` to the model

#### Scenario: Dispatched on OpenCode
- **WHEN** an OpenCode session dispatches a project agent
- **THEN** the subagent's own request carries the file's body as its system prompt

#### Scenario: A linked checkout reads its own branch
- **WHEN** an OpenCode session starts in a linked checkout whose branch changed `.agents/agents`
- **THEN** that checkout's agents are offered, not the main checkout's

#### Scenario: Two projects on one OpenCode service
- **WHEN** projects `a` and `b` each hold their own agent and OpenCode serves both from its shared background service
- **THEN** a session in `a` is offered `a`'s agent and not `b`'s, and a session in `b` the reverse

#### Scenario: A session below the checkout root
- **WHEN** an OpenCode session starts in a subdirectory of a checkout whose root holds `.agents/agents`
- **THEN** the root's agents are offered

#### Scenario: An agent written while OpenCode runs
- **WHEN** a file is added to `.agents/agents` while OpenCode's service is serving that project
- **THEN** the next session turn in that project is offered it without a restart

#### Scenario: A built-in agent's name
- **WHEN** a project agent's name is one of OpenCode's built-in agents
- **THEN** the built-in agent is kept and the project's is not registered

#### Scenario: A configured agent's name
- **WHEN** a project agent's name is one OpenCode's own configuration also defines
- **THEN** the configured fields apply over the project's, as OpenCode's precedence orders them

#### Scenario: OpenCode's route
- **WHEN** support is reported for project agents on OpenCode
- **THEN** the harness matrix reads Adapted, and the workspace and `uze doctor` read delivered while UZE's runtime plugin is in place

#### Scenario: The runtime plugin is not in place
- **WHEN** UZE's runtime plugin is missing from OpenCode's plugin directory
- **THEN** the workspace and `uze doctor` say to run `uze setup` for project agents, not that they are unsupported

#### Scenario: A foreign file holds the plugin's path
- **WHEN** a file UZE did not generate sits where the runtime plugin goes
- **THEN** project agents are reported blocked, naming that file, and setup does not write over it
