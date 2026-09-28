## Why

A migration of a real marketplace (4 plugins, 18 skills, 14 agents, one stdio
MCP server, one hook) from native Claude Code plugins to UZE, on 1.0.0-beta.3
with Claude Code 2.1.283, found that what reaches Claude is not the plugin the
author wrote:

- **The generated plugin carries `skills/` and nothing else.** `phases/`,
  `ui/`, `hooks/`, a README — anything at the package root — stays in the
  Store, invisible from the plugin root. A skill that reads a file next to it
  in the plugin works only when its author happened to hard-code a fallback
  path to their own checkout.
- **What it does carry is symlinks.** Claude copies a directory-marketplace
  plugin into `~/.claude/plugins/cache`, and the copy of a symlinked skill
  directory comes out empty. It worked in the tester's session only because
  the marketplace source was still on disk; the cache itself is hollow.
- **Agents never enter the plugin**, so Claude never namespaces them: the
  author's `flow:mr-guard` is delivered as a loose `~/.claude/agents/mr-guard.md`
  and `subagent_type: flow:mr-guard` answers "not found" (0/14). A skill that
  dispatches its agent by the qualified name works only when the model
  improvises the bare one.
- **A package that ships its own `.claude-plugin/` is delivered twice.** The
  explicit envelope is used, but UZE counts it as covering a skill only when
  the manifest lists it in a `skills` array, and an MCP server only when
  `mcpServers` is inline — Claude itself discovers `skills/`, `agents/` and
  `.mcp.json` by default. Everything "uncovered" is also delivered loose: 14
  agents appear twice, and the MCP server goes through `claude mcp add`.
- **The loose MCP label is invalid for Claude.** `<plugin>@<market>-<server>`
  is refused ("Names can only contain letters, numbers, hyphens, and
  underscores"), which fails the whole install. It is reached whenever an MCP
  server is delivered loose — through the coverage gap above, or when the
  derived marketplace could not be published. That is what made the route
  look like it depended on which harnesses were detected: it depends on the
  presence of `.claude-plugin/` and on publication, and nothing says so.
- **`${PLUGIN_ROOT}` stops at `hooks.json` and `mcp.json`.** A skill body
  that names `${PLUGIN_ROOT}/phases/x.md` receives the literal text, and the
  root it would name does not contain the file anyway.

The Native > Generated Native precedence is sound; what fails is fidelity.
The generated plugin must be the plugin, and a route that cannot keep a
capability's meaning must not be the one silently taken.

## What Changes

- **The generated plugin is the whole package.** Its root holds every file
  the Store holds for the package (regular files, not links into the Store),
  plus the generated manifest and the rewritten `SKILL.md`s. Claude's copy
  into its cache is therefore complete, and `${CLAUDE_PLUGIN_ROOT}` and
  `${PLUGIN_ROOT}` name a directory that has the files.
- **Agents are delivered inside the plugin** on every Claude route that has a
  plugin, so Claude exposes them as `<plugin>:<agent>`, exactly as it does for
  a native plugin. A package whose only Claude-deliverable content is agents
  is still generated as a plugin.
- **Explicit envelope coverage follows Claude's own discovery.** An author's
  `.claude-plugin/plugin.json` covers what Claude would load from it: the
  default `skills/`, `agents/` and `hooks/hooks.json` directories when the
  manifest does not override them, and `mcpServers` as an inline object, a
  path, or the default `.mcp.json`. Nothing the envelope covers is delivered a
  second time. A canonical `hooks.json` beside an envelope that declares its
  own hooks is reported, not run twice.
- **MCP labels are valid in the harness that registers them.** An
  integration that registers servers by name through a vendor CLI derives a
  label from the vendor's own character set, injective over
  `(plugin, marketplace, server)`. Claude's loose label becomes
  `<plugin>_<marketplace>__<server>`; package and marketplace names are
  kebab-case, so the separators cannot be produced by either.
- **`${PLUGIN_ROOT}` is resolved in skill and agent text** that UZE delivers,
  to the delivered package root, the same one resolver `mcp.json` already
  uses. It is documented as one placeholder with one meaning in every file
  UZE reads.
- **A capability-level fallback is never silent.** When a package that could
  have been delivered as a plugin is delivered loose — the marketplace could
  not be published, or a prior receipt pins the loose form — the install says
  which, per harness.

## Capabilities

### New Capabilities

- `package-delivery-fidelity`: what a delivered plugin contains, how its
  capabilities are named in the harness, when a capability is delivered once
  and only once, and which labels a vendor registry is given.

### Modified Capabilities

<!-- none: `add-portable-agent-capability` (open) owns agent naming across
harnesses and is updated in place to match. -->

## Impact

- `crates/uze-integrations/src/claude/{generate,plugin}.rs`,
  `shared/marketplace.rs` (`package_plan`, `generatable`, coverage),
  `shared/skill.rs` (copy instead of link for generated envelopes),
  `shared/mcp.rs` (label per harness, root resolver reused for text).
- `crates/uze-core/src/delivery/integration.rs` (exposure name candidates for
  MCP become harness-owned); `attach.rs` (loose fallback evidence).
- `runtime/attachments/claude/generated/` grows from links to copies: the
  generated tier, reproducible, same cost to delete.
- Tests pinning the `@` label (`tests/integrations/contract.rs`,
  `tests/projection/{naming,invocation}.rs`) and the symlinked envelope
  (`claude/generate.rs`) change; the fake `claude` in `tests/cli/machine.rs`
  learns Claude's MCP name rule.
- Conformance (Claude vertical): the plugin cache is non-empty; an agent is
  selectable as `<plugin>:<agent>`; a file outside `skills/` is readable from
  the plugin root; an explicit envelope yields no duplicate.
