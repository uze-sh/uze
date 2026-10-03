## ADDED Requirements

### Requirement: Scaffolds refuse a name outside the rule
The system SHALL refuse `uze agent market create <name>` and
`uze agent plugin create <name>` when the name is not lowercase kebab-case
of at most 64 characters (see the `plugin` capability), before writing
anything, and the refusal SHALL name the corrected name when one can be
derived.

#### Scenario: A plugin named in another style
- **WHEN** the agent runs `uze agent plugin create My_Plugin --market tools`
- **THEN** the command fails, says `try \`my-plugin\``, and writes no file

## MODIFIED Requirements

### Requirement: Check validates authored plugins and marketplaces offline
The system SHALL, for `uze agent plugin check <path>` and `uze agent market check <path>`, validate the authored artifact without touching the Store, any harness or the network, and report every finding: manifest parse errors, a plugin or marketplace name outside the name rule (with the corrected name), absolute or `..`-shaped path references, `SKILL.md` files missing required frontmatter, carrying a `name` that is missing, outside the name rule or different from the skill's directory, or carrying a malformed invocation policy, a `hooks.json` violating the handler contract (timeout bounds, deny exit code semantics), a malformed `mcp.json`, and — for marketplace checks — `plugins[]` entries whose `name` is outside the name rule, whose `source` does not resolve inside the marketplace or whose plugin fails its own check. A clean artifact SHALL exit zero; any finding SHALL exit non-zero with the reasons on the answer.

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
