# package-delivery-fidelity Specification

## Purpose
TBD - created by archiving change deliver-the-whole-plugin. Update Purpose after archive.
## Requirements
### Requirement: A generated plugin carries the whole package

When the system generates a harness-native plugin from a package, the
plugin's root SHALL contain every file the Store holds for that package, at
the same relative path, as files the harness can copy without following links
into the Store. Generated or rewritten files (the harness manifest, a
`SKILL.md` carrying a translated invocation policy) replace their canonical
counterparts at the same path. A path the harness would load as a
component the canonical format does not define (for Claude Code:
`commands/`, `bin/`, `output-styles/`, `workflows/`, `themes/`,
`monitors/`, `settings.json`, `.lsp.json`, and its own `hooks/hooks.json`
and `.mcp.json`) SHALL NOT be carried, and `uze inspect` SHALL name each
such path the package contains as not delivered to that harness. Store bytes
SHALL remain unchanged.

#### Scenario: A file outside skills/ is reachable from the plugin root

- **WHEN** a package containing `skills/run/SKILL.md` and `phases/plan.md` is
  delivered to Claude Code as a generated plugin
- **THEN** `phases/plan.md` exists under the generated plugin's root
- **AND** a skill resolving `${PLUGIN_ROOT}/phases/plan.md` reads that file

#### Scenario: A harness-only component is not delivered

- **WHEN** a package contains `bin/tool` and `commands/legacy.md`
- **THEN** neither is present in the generated Claude Code plugin
- **AND** both remain in the Store package
- **AND** `uze inspect` names `bin` and `commands` as not delivered to Claude
  Code

#### Scenario: The harness cache copy is complete

- **WHEN** Claude Code installs the generated plugin and copies it into its
  plugin cache
- **THEN** the cached copy contains every skill directory with its files
- **AND** no entry in the generated plugin is a symbolic link into the Store

### Requirement: Agents keep their plugin-qualified name

On a harness whose plugin mechanism namespaces agents, the system SHALL
deliver a package's agents inside the package's plugin, so the harness
exposes each as `<plugin>:<agent>`. A package whose only deliverable content
for that harness is agents SHALL still be delivered as a plugin. An agent
delivered through a plugin SHALL NOT also be delivered to the harness's
global agent directory.

#### Scenario: Qualified agent dispatch works

- **WHEN** plugin `flow` with `agents/mr-guard.md` is installed and a session
  dispatches a subagent named `flow:mr-guard`
- **THEN** Claude Code runs that agent
- **AND** no agent named `mr-guard` is exposed from `~/.claude/agents`

### Requirement: A capability is delivered once per harness

The system SHALL deliver each capability of a package to a harness through
exactly one route. When a package ships its own harness envelope, the
capabilities the harness would load from that envelope — including those it
discovers by default without the manifest naming them — SHALL be treated as
covered and SHALL NOT be delivered again through another route. A canonical
hook declaration beside an envelope that declares its own hooks SHALL be
treated as covered by the envelope, not delivered twice.

#### Scenario: Envelope without a skills list

- **WHEN** a package has `.claude-plugin/plugin.json` with no `skills` field
  and a `skills/review/SKILL.md`
- **THEN** the skill is delivered by the envelope only
- **AND** nothing is linked under `~/.claude/skills` for it

#### Scenario: Envelope with a .mcp.json file

- **WHEN** a package's envelope declares no inline `mcpServers` and the
  package has `.mcp.json` at its root
- **THEN** the MCP server is delivered by the envelope only
- **AND** no vendor CLI registration is attempted for it

### Requirement: Registry labels are valid for the harness

A label the system gives a vendor registry (an MCP server name registered
through a harness CLI or written into its configuration) SHALL satisfy that
harness's documented name rule; a label a harness would reject SHALL never be
attempted. A label another resource already holds SHALL NOT be reused: the
next candidate the integration offers is taken, and when every candidate is
held the delivery is reported as a conflict before anything is attached.

#### Scenario: Loose MCP registration on Claude Code

- **WHEN** server `rtc` of plugin `std` from marketplace `cardinal` is
  registered with Claude Code outside a plugin
- **THEN** the registered name contains only letters, digits, `-` and `_`
- **AND** installing the plugin succeeds

### Requirement: The plugin root placeholder means one thing everywhere

The system SHALL replace `${PLUGIN_ROOT}` with the delivered package root in
every text file it delivers that the author may use it in — `mcp.json`,
`hooks.json`, `SKILL.md` and agent definitions — and in no other file. The
delivered package root SHALL contain the whole package.

#### Scenario: A skill names a file by the placeholder

- **WHEN** a skill body contains `${PLUGIN_ROOT}/phases/plan.md`
- **THEN** the body a harness loads names an absolute path that exists
- **AND** the canonical `SKILL.md` in the Store still contains the placeholder

### Requirement: A loose fallback is reported

When a package that a harness could receive as a plugin is delivered
capability by capability instead, the system SHALL say so in the install and
update report for that harness, with the reason (the derived marketplace could
not be published, or an existing receipt holds the loose form).

#### Scenario: Publication failed

- **WHEN** the derived marketplace for Claude Code cannot be published during
  an install
- **THEN** the report names Claude Code, the loose route, and the publication
  error

