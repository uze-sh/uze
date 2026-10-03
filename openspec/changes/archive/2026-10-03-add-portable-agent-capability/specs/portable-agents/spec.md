## Purpose

Make package-defined agent profiles portable across UZE's supported harnesses
while preserving canonical source bytes and clearly reporting any adaptation.

## ADDED Requirements

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
with the label. Codex's representation is a UZE-generated standalone TOML
custom-agent file in `~/.codex/agents/`. Two packages declaring an agent of
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
- **THEN** UZE writes a generated TOML custom-agent file carrying the label as
  `name`, the description, and the body as `developer_instructions`, without
  modifying Store bytes
- **AND** the route is Native when every authored root field was carried, and
  Degraded naming each root field that was not

### Requirement: Only the portable frontmatter subset is projected

The system SHALL treat `name` (replaced by the label) and `description` as the
portable frontmatter root, and the body as the prompt. A field a harness
spells differently SHALL be written under a `harness:` block keyed by the
harness id or one of its aliases (`claude-code`, `codex`, `opencode`,
`antigravity`/`agy`); each integration renders only its own block's fields
into its delivered file and never delivers the block itself. Each integration
declares which block fields it knows and the shape each must have: a known
field of the wrong shape is not delivered, a field the harness drops an agent
over is left out, and an unknown field is carried as written on a harness
that tolerates unknown fields and left out on one that refuses them (Codex).
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

#### Scenario: Each harness receives its own block

- **WHEN** an agent's `harness:` block gives `claude-code`, `codex`,
  `opencode`, and `agy` each its own `model`
- **THEN** each harness's delivered file carries the model its own block
  names, and no delivered file carries the `harness:` block

### Requirement: Agent definitions are checked before install

`uze agent plugin check`, and `uze agent market check` for each plugin it
lists, SHALL validate every agent definition below `agents/`: UTF-8,
parseable YAML frontmatter, a non-empty `description`, every part of the
agent's label obeying the package name rule, and a `harness:` block that is a
mapping of mappings redefining no identity field. Each integration's check
SHALL report a block field of the wrong shape as an error and a field it
leaves out as a warning. A failing agent SHALL be reported with its path and
the fault.

#### Scenario: Agent without frontmatter

- **WHEN** a plugin contains `agents/reviewer.md` with no frontmatter
- **THEN** `uze agent plugin check` reports `agents/reviewer.md` and the
  missing frontmatter

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
