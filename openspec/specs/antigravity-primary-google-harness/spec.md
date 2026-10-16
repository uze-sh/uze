# antigravity-primary-google-harness Specification

## Purpose
How UZE supports Antigravity CLI as its Google-family harness: a peer
integration that delivers packages through `agy`'s own plugin, MCP and
commands-to-Skills mechanisms with receipt-proven ownership, reads `AGENTS.md`
natively, and replaces Gemini CLI.

## Requirements

### Requirement: Antigravity CLI is a first-class peer integration

UZE SHALL implement Antigravity CLI (`agy`) as an independent
`IntegrationPort` implementation — no inheritance from, or delegation to,
any other integration — exposed under the stable id `antigravity` with the
aliases `agy` and `antigravity-cli`, and ordered as a primary v0 harness
alongside Claude Code, Codex, and OpenCode (Gemini CLI remaining available
as legacy/compatibility).

#### Scenario: Integration construction and detection

- **WHEN** a `UzeApplication` is composed from the environment
- **THEN** the integration list contains Claude Code, Codex, OpenCode,
  Antigravity CLI, and Gemini CLI (in that order)
- **AND** `detect()` reports presence from `agy --version` with the bare
  version token as the version
- **AND** internal invocations resolve the real `agy` past `$UZE_HOME/shims`
  so an internal call can never re-enter a UZE runtime shim.

#### Scenario: No Core or IntegrationPort change

- **WHEN** the Antigravity integration is implemented
- **THEN** `uze-core` production code gains no vendor-specific types,
  paths, or branches
- **AND** the `IntegrationPort` trait is unchanged.

### Requirement: Antigravity native plugin delivery with exact coverage

UZE SHALL deliver a stored canonical package through `agy plugin install`
using the canonical `plugin.json` as the vendor manifest, and SHALL compute
`provided_resource_identities` as an exact intersection (`discovered ∩
declared`) over the structural surfaces: `skills/`, `commands/` (converted
to Skills by the CLI), `mcp_config.json`-declared MCP servers, and every
portable hook group the harness can run, delivered in the generated
plugin's own root `hooks.json`.

#### Scenario: Canonical package without MCP

- **WHEN** a canonical package has a valid `plugin.json` name
  (`^[a-zA-Z0-9-_]+$`) and no canonical `mcp.json` servers
- **THEN** the package exposure plan is Native and covers its conventional
  `skills/` and `commands/` resources
- **AND** the package is installed directly from the Store path, with no
  synthesized envelope.

#### Scenario: Canonical package with MCP

- **WHEN** a canonical package declares MCP servers in `mcp.json`
- **THEN** UZE materializes a deterministic generated plugin into a
  UZE-owned derived directory (never the Store) carrying
  `plugin.json`, symlinked `skills/`/`commands/`, and a translated
  `mcp_config.json` whose remote servers carry the `url` key (an
  `httpUrl` key rewritten to `url`, a `url` key kept as written)
- **AND** installs that directory
- **AND** exactly the canonical `mcp.json`-declared MCP resources plus the
  conventional Skill/Command resources are marked provided.

#### Scenario: Canonical package with hooks

- **WHEN** a canonical package declares portable hook groups Antigravity
  can run
- **THEN** the generated plugin carries them in its own `hooks.json`, one
  named hook per group running the generated wrapper
- **AND** they are marked provided, so no entry for them is written into
  the shared `~/.gemini/config/hooks.json`
- **AND** an entry an earlier build wrote there for one of them is retired
  by the next install.

#### Scenario: Partial coverage falls back, never disappears

- **WHEN** a package contains a resource outside the covered surfaces
- **THEN** that resource is not marked provided
- **AND** it still resolves to a non-`Unsupported` capability-level plan.

#### Scenario: Foreign same-name import is refused

- **WHEN** Antigravity already holds an installed plugin with the target
  name that UZE has no receipt for, whether it was imported from a
  directory or installed from the vendor's marketplace
- **THEN** UZE refuses to install rather than overwriting the foreign
  plugin.

#### Scenario: The census cannot be read

- **WHEN** neither the import manifest nor `agy plugin list` can be read
  in a shape UZE knows
- **THEN** inspection reports the plugin's state as unknown and install
  and detach are refused, never treated as a free name.

### Requirement: Staged plugins are derived artifacts with ownership proof

The staged tree at `~/.gemini/config/plugins/<name>/` SHALL be treated as a
Derived Artifact rebuilt from the Store, proven at inspection by content
fingerprint plus registration, and removed through the official `agy
plugin uninstall` verb; destructive detach SHALL be blocked on Drifted or
Conflict.

#### Scenario: Attach → inspect Matched → detach → Missing

- **WHEN** a package is attached
- **THEN** inspection reports Matched while staged content equals the
  receipt fingerprint and Antigravity's census contains the registration
- **AND** after detach the staged tree and registration are removed.

#### Scenario: Drift blocks destructive removal

- **WHEN** staged plugin content no longer matches the receipt fingerprint
- **THEN** inspection reports Drifted
- **AND** detach leaves the foreign content untouched.

#### Scenario: Store bytes unchanged

- **WHEN** a package is attached or detached
- **THEN** the Store package tree is never written to by the integration.

### Requirement: Registration is read from one census of installed plugins

Registration SHALL be read once per operation from one census of
Antigravity's installed plugins, and the same census SHALL answer both
whether the name is free and whether the registration is UZE's. Detach
SHALL be refused when the census holds more than one plugin of the name,
so the vendor's removal by name can never reach a plugin UZE did not
install.

#### Scenario: A foreign plugin of the same name beside UZE's

- **WHEN** the census holds UZE's imported plugin and another plugin of
  the same name installed from the vendor's marketplace
- **THEN** inspection reports Conflict and detach removes nothing.

### Requirement: Antigravity context is native

UZE SHALL treat Antigravity CLI as reading `AGENTS.md` directly, and SHALL
not generate any bridge file for it.

#### Scenario: Context reconcile writes nothing for Antigravity

- **WHEN** `uze agent context reconcile` runs
- **THEN** no Antigravity-specific file is written into the project.

### Requirement: Commands are adapted, not native

UZE SHALL deliver canonical Commands on Antigravity through the vendor's
official commands→Skills conversion, and SHALL declare the route Adapted —
not Native — because Skills are model-discoverable and no explicit-only
mechanism is documented or observable.

#### Scenario: Command route classification

- **WHEN** a canonical Command is planned on Antigravity
- **THEN** the exposure plan route is `Adaptable`
- **AND** the delivered artifact preserves the stable namespaced label,
  canonical description and body.

#### Scenario: The missing explicit-only mechanism is not faked

- **WHEN** a generated Command artifact is inspected
- **THEN** it carries no policy file that does not exist in Antigravity
  (no `agents/openai.yaml` or equivalent is invented).

### Requirement: MCP falls back through `agy mcp add`

For MCP resources outside plugin coverage, UZE SHALL register the server
through `agy mcp add <name> <command> [args…]` writing
`~/.gemini/config/mcp_config.json`, and SHALL inspect by reading that JSON
file directly.

#### Scenario: Registration and inspection

- **WHEN** an MCP server is attached
- **THEN** `agy mcp add` succeeds and the entry exists in
  `~/.gemini/config/mcp_config.json`
- **AND** inspection compares command/args and reports Drifted on mismatch,
  treating a user `disabled` as a preference rather than an ownership
  signal.

### Requirement: Gemini CLI is replaced, not kept as legacy

The integration that preceded Antigravity as the Google-family harness
SHALL be removed from the codebase — module, tests, fixtures, e2e spec,
composition and bridge entry — with its historical record preserved only in
the ADRs and the migration-audit document.

#### Scenario: The codebase carries no removed-integration residue

- **WHEN** the repository is searched for the removed harness name as an
  identifier or path (excluding `~/.gemini` vendor config paths,
  historical ADRs/OpenSpecs and the migration-audit record)
- **THEN** no active code, test, fixture or current documentation entry
  references it
- **AND** the remaining harness set is Claude Code, Codex, OpenCode and
  Antigravity CLI.
