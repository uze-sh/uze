## Purpose

Make package-defined agent profiles portable across UZE's supported harnesses
while preserving canonical source bytes and clearly reporting any adaptation.

## ADDED Requirements

### Requirement: Canonical package agent discovery

The system SHALL recognize every Markdown definition at
`agents/<name>.md` in an installed package as an Agent capability, preserve
the definition's bytes in the Store, and compose it independently from Skill,
MCP, instruction, and hook capabilities.

#### Scenario: Package contains an agent definition

- **WHEN** a package contains `agents/reviewer.md`
- **THEN** inspection reports one Agent capability named `reviewer` with the
  canonical file as its source

#### Scenario: Package has no agent directory

- **WHEN** a package contains no `agents/` directory
- **THEN** its existing capabilities and delivery behavior remain unchanged

### Requirement: Per-harness agent delivery

The system SHALL expose each canonical Agent independently to every supported
harness under the label `<plugin>:<agent>`, the same qualified label Skills
use (ADR-026). Where the harness's plugin mechanism namespaces agents, the
agent SHALL be delivered inside the package's plugin; where the harness reads
a global agent root, the delivered file SHALL be named with the qualified
label. Codex's representation is a UZE-generated standalone TOML custom-agent
file. Two packages declaring an agent of the same name SHALL both be
delivered, each under its own label.

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
- **THEN** UZE exposes a generated TOML custom-agent file without modifying
  Store bytes, and reports the route as adapted when frontmatter fields were
  not carried

### Requirement: Only the portable frontmatter subset is projected

The system SHALL project to each harness only the frontmatter fields that
harness accepts with the same meaning, and SHALL NOT deliver a field whose
value the harness would reject. A field that is not carried SHALL be named in
the route evidence and SHALL make the route Adapted for that harness. When
the harness accepts the whole definition unchanged, the Store bytes MAY be
exposed directly.

#### Scenario: Claude-style fields reach OpenCode

- **WHEN** an agent declares `model: haiku` and `tools: Read, Grep` and is
  delivered to OpenCode
- **THEN** OpenCode lists the agent
- **AND** the install report names `model` and `tools` as not carried for
  OpenCode

### Requirement: Agent definitions are checked before install

`uze agent plugin check` and install SHALL validate every `agents/*.md`:
parseable YAML frontmatter, a non-empty `description`, and a file stem that
is a valid package name. A failing agent SHALL be reported with its path and
the fault.

#### Scenario: Agent without description

- **WHEN** a plugin contains `agents/reviewer.md` with no frontmatter
- **THEN** `uze agent plugin check` reports `agents/reviewer.md` and the
  missing `description`

### Requirement: Agent lifecycle safety

The system SHALL record every UZE-managed agent artifact in a typed receipt,
inspect it before destructive detach, and block removal when the artifact or
receipt has drifted.

#### Scenario: Cleanly detached agent

- **WHEN** an attached Agent artifact still matches its receipt
- **THEN** removing its package removes that artifact and leaves Store bytes
  and unrelated harness artifacts intact

#### Scenario: Drifted agent artifact

- **WHEN** a managed Agent artifact differs from its receipt
- **THEN** removal is blocked without deleting the drifted artifact

### Requirement: Capability support reporting

The system SHALL show an Agents row in the TUI and generated README harness
matrix using the same Native, Adapted, Degraded, Unsupported, and roadmap
semantics used for other capabilities.

#### Scenario: Matrix generation

- **WHEN** the support matrix is generated after agent delivery is available
- **THEN** each harness's Agents cell reflects its proven route: Native only
  with a passing real-harness scenario, Adapted where fields are not carried

### Requirement: Real-harness conformance evidence

The conformance lab SHALL verify discoverability and isolation for each
harness's Agent delivery route without Internet access or provider tokens.

#### Scenario: Conformance execution

- **WHEN** the agent conformance scenarios execute for a supported harness
- **THEN** they prove the expected agent is discoverable only from the
UZE-managed fixture and report a nonzero failure on an unexpected route
