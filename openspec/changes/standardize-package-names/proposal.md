## Why

Every harness UZE delivers to converges on one spelling for a name:
lowercase kebab-case. The Agent Skills specification (read by Claude Code,
Codex, OpenCode and Gemini) allows a skill `name` of 1 to 64 characters from
`a-z`, `0-9` and `-`, with no leading, trailing or doubled hyphen, equal to
its directory; Claude Code and Codex hold plugin names to kebab-case and put
casing for display in a separate `displayName`; Gemini CLI, Antigravity's
base, holds extension names to `^[a-z0-9][a-z0-9-]*[a-z0-9]$` and cannot
enable an uppercase one with `-e`; OpenCode holds skills to
`^[a-z0-9]+(-[a-z0-9]+)*$`.

UZE is the only link in the chain that is more permissive. It accepts
`[A-Za-z0-9_-]` with no length bound, so it installs names a harness later
rejects, and it treats names case-sensitively: `Git@AI` and `git@ai` are two
packages in the Store, the lock and the receipts, and on a case-insensitive
filesystem they share one directory. A marketplace's own name is not
checked at all when it is registered, only later, when a plugin from it is
installed.

## What Changes

- **One rule** for plugin names, marketplace names and install aliases:
  `^[a-z0-9]+(-[a-z0-9]+)*$`, 1 to 64 characters. It subsumes the leading
  `-` rule (ADR-036), whose reason (a vendor CLI parsing an id as a flag)
  still holds.
- **Marketplace registration checks the name** it reads from
  `marketplace.json` before anything is recorded or mirrored.
- **A refusal names the corrected name**: lowercased, `_`, spaces and `.`
  as `-`, hyphens collapsed and trimmed.
- **What a person types is forgiven its case**: the install spec, the
  `remove`, `update` and `inspect` argument, the `market` verbs' name,
  `--alias` and `--market` are lowercased before anything resolves.
  Resolution against the record stays exact.
- **Authoring holds the same rule**: `uze agent plugin create` and
  `uze agent market create` refuse a name outside it, and both checks
  report one, including a `SKILL.md` whose `name` is missing, outside the
  rule, or different from its directory.
- **No migration.** UZE is pre-release and every name on record
  (`uze-official`, `ai`) already holds; nothing is carried across.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `plugin`: the name rule, and the case a person types being forgiven.
- `marketplace`: registration refuses a name outside the rule.
- `plugin-authoring`: scaffolds refuse a name outside the rule; checks
  report plugin, marketplace, entry and skill names outside it.

## Impact

- `uze-core`: the name rule, its suggestion, the marketplace manifest
  reader, the marketplace registry, the authoring check.
- `uze` CLI: argument parsing lowercases typed names.
- `plugins/uze`: the `author` Skill states the rule; its own `SKILL.md`
  gains the `name` it lacked.
- Follow-up, out of scope: showing a plugin's `displayName` in the TUI when
  it declares one.
