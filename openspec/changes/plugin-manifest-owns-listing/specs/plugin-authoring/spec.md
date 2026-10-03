## MODIFIED Requirements

### Requirement: Plugin scaffold produces an installable plugin inside a marketplace
The system SHALL, for `uze agent plugin create <name> --market <name> [--description <text>] [--category <word>] [--hook] [--mcp] [--instructions]`, create `plugins/<plugin-name>/` inside the marketplace's checkout with: a `plugin.json` (name, description), a `skills/<skill-name>/SKILL.md` whose frontmatter carries the canonical `invoke: {model, user}` policy block, the optional capability files for each flag (`hooks.json`, `mcp.json`, an instruction resource), and the matching `plugins[]` entry added to the marketplace's `marketplace.json`, carrying the plugin's `name`, `source` and, when given, `category`. The description SHALL be written to `plugin.json` only. Every scaffolded file SHALL carry commented documentation explaining its fields, and the scaffold SHALL refuse to overwrite an existing plugin of the same name.

#### Scenario: Default scaffold is a skill plugin
- **WHEN** an agent runs `uze agent plugin create greet --market tools --description "Says hello"`
- **THEN** `plugins/greet/` exists with `plugin.json` and `skills/greet/SKILL.md`, the marketplace manifest names `greet → ./plugins/greet`, and the plugin is installable

#### Scenario: The description has one home
- **WHEN** an agent runs `uze agent plugin create greet --market tools --description "Says hello" --category productivity`
- **THEN** `plugins/greet/plugin.json` carries `"description": "Says hello"`, and the `greet` entry in `marketplace.json` carries `category` but no `description`

#### Scenario: Capability flags extend the scaffold
- **WHEN** the agent passes `--hook --mcp`
- **THEN** the scaffolded plugin additionally carries a `hooks.json` with one handler stub obeying the `HOOK_*`/exit-code contract and an `mcp.json` with one server stub, and `check` passes

#### Scenario: Scaffold never overwrites
- **WHEN** `plugins/<name>/` already exists in the marketplace
- **THEN** the command fails naming the existing plugin and writes nothing into it

### Requirement: Check validates authored plugins and marketplaces offline
The system SHALL, for `uze agent plugin check <path>` and `uze agent market check <path>`, validate the authored artifact without touching the Store, any harness or the network, and report every finding: manifest parse errors, a plugin or marketplace name outside the name rule (with the corrected name), absolute or `..`-shaped path references, `SKILL.md` files missing required frontmatter, carrying a `name` that is missing, outside the name rule or different from the skill's directory, or carrying a malformed invocation policy, a `hooks.json` violating the handler contract (timeout bounds, deny exit code semantics), a malformed `mcp.json`, and — for marketplace checks — `plugins[]` entries whose `name` is outside the name rule, whose `source` does not resolve inside the marketplace or whose plugin fails its own check. A clean artifact SHALL exit zero; any finding SHALL exit non-zero with the reasons on the answer.

A marketplace check SHALL also warn, without failing, on each `plugins[]` entry that carries `description` or `keywords`, naming the entry, the field and the one action that resolves it: move it to the plugin's `plugin.json` when that file lacks the field, delete it when `plugin.json` already carries the same value, or keep one of the two in `plugin.json` when the values differ.

#### Scenario: A plugin that would fail at install fails check first
- **WHEN** an authored plugin's `plugin.json` names it `-starts-with-dash` and the agent runs `uze agent plugin check` on its directory
- **THEN** the command exits non-zero and names the invalid package name, before any install was attempted

#### Scenario: A clean authored plugin passes
- **WHEN** the agent scaffolds a plugin with `--hook --mcp --instructions` and runs `uze agent plugin check` on it
- **THEN** the command exits zero and reports what the plugin would deliver (one skill, one hook group, one MCP server)

#### Scenario: A marketplace check covers its plugins
- **WHEN** the agent runs `uze agent market check` on a scaffolded marketplace whose one plugin has an invalid `hooks.json`
- **THEN** the command exits non-zero and the answer locates the finding in that plugin

#### Scenario: A skill a harness would refuse by name
- **WHEN** a plugin's `skills/greet/SKILL.md` carries `name: hello`, `name: Greet_Skill`, or no `name`
- **THEN** `uze agent plugin check` exits non-zero and the finding says which, with the corrected name or the directory the name must equal

#### Scenario: A describing field left on an entry is moved
- **WHEN** the `greet` entry carries `"description": "Says hello"` and `plugins/greet/plugin.json` carries no `description`
- **THEN** `uze agent market check` exits zero and warns that `greet`'s `description` is not read and belongs in `plugins/greet/plugin.json`

#### Scenario: A duplicated describing field is deleted
- **WHEN** the `greet` entry and `plugins/greet/plugin.json` carry the same `description`
- **THEN** the warning says to delete it from the entry, since `plugin.json` already says the same

#### Scenario: Two different values are reconciled by hand
- **WHEN** the `greet` entry's `keywords` differ from those in `plugins/greet/plugin.json`
- **THEN** the warning names both, says the `plugin.json` value is the one shown, and asks to keep one of the two there and delete the entry's
