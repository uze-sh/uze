## MODIFIED Requirements

### Requirement: Authoring verbs are deterministic, agent-facing, and unaliased for people
The system SHALL expose plugin authoring only as deterministic verbs under `uze agent …`: each verb takes its full input on the command line (or its defaults), performs exactly one artifact-producing step, prints a machine-readable answer, and never prompts interactively. The authoring verbs SHALL be hidden from `uze --help` like the rest of the `agent` grammar and SHALL NOT gain person-facing aliases, since a person's CLI surface carries only what a person performs by hand.

#### Scenario: An agent drives the full authoring loop without a person's input
- **WHEN** an agent runs `uze agent market create`, then `uze agent plugin create`, then `uze agent plugin check`, providing all arguments up front
- **THEN** each command performs its one step and exits zero with a summary an agent can parse, and no command ever waits for interactive input

#### Scenario: Authoring verbs stay off the person's help surface
- **WHEN** a person runs `uze --help` or `uze agent --help`
- **THEN** the authoring verbs are not listed; they are reachable by following the `uze:author` Skill, and no person-facing alias of them exists — browsing what a marketplace offers is `uze market`'s answer, not a second authoring spelling

### Requirement: The authoring verbs are documented for every agent
The authoring verbs SHALL be documented by the `uze:author` Skill, which the
official plugin delivers natively to every harness, so an agent reaches them
whether or not the workspace launched it. UZE SHALL NOT document them in a
project's files. None of the authoring verbs SHALL require an agent the
workspace launched.

#### Scenario: A project without a workspace policy documents authoring
- **WHEN** a project that declares no workspace policy reconciles its context
- **THEN** its `AGENTS.md` carries no text about the authoring verbs

#### Scenario: An agent started by hand authors a plugin
- **WHEN** an agent a person started by hand is asked to create a plugin and follows the `uze:author` Skill
- **THEN** every verb it names runs without asking for a launched agent

### Requirement: Plugin scaffold produces an installable plugin inside a marketplace
The system SHALL, for `uze agent plugin create <name> --market <name> [--description <text>] [--category <word>] [--hook] [--mcp]`, create `plugins/<plugin-name>/` inside the marketplace's checkout with: a `plugin.json` (name, description), a `skills/<skill-name>/SKILL.md` whose frontmatter carries the canonical `invoke: {model, user}` policy block, the optional capability file for each flag, and the matching `plugins[]` entry added to the marketplace's `marketplace.json`.

#### Scenario: Default scaffold is a skill plugin
- **WHEN** an agent runs `uze agent plugin create greet --market tools --description "Says hello"`
- **THEN** `plugins/greet/` exists with `plugin.json` and `skills/greet/SKILL.md`, the marketplace manifest names `greet → ./plugins/greet`, and the plugin is installable

#### Scenario: The entry the scaffold adds
- **WHEN** an agent runs `uze agent plugin create greet --market tools --category productivity`
- **THEN** the `greet` entry added to `marketplace.json` carries the plugin's `name`, `source` and `category`; without `--category` it carries `name` and `source` only

#### Scenario: Capability flags extend the scaffold
- **WHEN** the agent passes `--hook --mcp`
- **THEN** the scaffolded plugin additionally carries a `hooks.json` with one handler stub obeying the `HOOK_*`/exit-code contract and an `mcp.json` with one server stub, and `check` passes

#### Scenario: The instructions flag adds an instruction resource
- **WHEN** the agent passes `--instructions`
- **THEN** the command fails and names `--rules` with an `always_on` rule as the way a plugin speaks to every agent of a declaring project

### Requirement: Check validates authored plugins and marketplaces offline
The system SHALL, for `uze agent plugin check <path>` and `uze agent market check <path>`, validate the authored artifact without touching the Store, any harness or the network, and report every finding, including manifest parse errors, a plugin or marketplace name outside the name rule (with the corrected name), and absolute or `..`-shaped path references. A clean artifact SHALL exit zero; any finding SHALL exit non-zero with the reasons on the answer.

#### Scenario: A plugin that would fail at install fails check first
- **WHEN** an authored plugin's `plugin.json` names it `-starts-with-dash` and the agent runs `uze agent plugin check` on its directory
- **THEN** the command exits non-zero and names the invalid package name, before any install was attempted

#### Scenario: A clean authored plugin passes
- **WHEN** the agent scaffolds a plugin with `--hook --mcp` and runs `uze agent plugin check` on it
- **THEN** the command exits zero and reports what the plugin would deliver (one skill, one hook group, one MCP server)

#### Scenario: A plugin still shipping AGENTS.md
- **WHEN** a plugin carries an `AGENTS.md` at its root and the agent runs `uze agent plugin check` on it
- **THEN** the command reports that the file is not delivered and names an `always_on` rule as its replacement
