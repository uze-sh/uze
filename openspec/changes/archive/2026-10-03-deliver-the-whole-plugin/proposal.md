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
  own hooks is covered by the envelope, not run twice.
- **MCP labels are valid in the harness that registers them.** An
  integration that registers servers by name through a vendor CLI derives a
  its labels from the vendor's own character set, and never reuses a label
  another server holds. Claude's loose label becomes
  `<plugin>-<server>`, then `<plugin>-<marketplace>-<server>` when that is
  taken, with any character outside `[A-Za-z0-9_-]` mapped to `-`.
- **`${PLUGIN_ROOT}` is resolved in skill and agent text** that UZE delivers,
  to the delivered package root, the same one resolver `mcp.json` already
  uses. It is documented as one placeholder with one meaning in every file
  UZE reads.
- **A capability-level fallback is never silent.** When a package that could
  have been delivered as a plugin is delivered loose — the marketplace could
  not be published, or a prior receipt pins the loose form — the install says
  which, per harness.

### How the bytes reach a harness

The first half of this change made the plugin UZE generates complete. What it
left alone is *how* bytes reach a harness on the other routes, and a study of
the four harnesses (Lab image `beta4-final`, 2026-09-28: Claude Code 2.1.284,
Codex 0.158.0, OpenCode 2.0.18, Antigravity 1.2.12; every mechanic measured
with no UZE in the loop) shows that the answer is decided per file kind, not
per harness, and that UZE currently takes two of the answers the harnesses
punish:

- **A loose Skill is a directory symlink into the generated tier, whose
  extras are symlinks into the Store.** OpenCode's skill walker does not
  descend a symlinked skill root: the body loads, but `<skill_files>` (the
  list of supporting files the model is shown) comes out empty. Codex
  ignores a `SKILL.md` that is itself a link. Every harness that keeps a
  plugin cache drops linked files when it copies. A link is safe for exactly
  one of these readers and silently lossy for the others.
- **Generated directories are rebuilt in place.** `recreate_dir` removes the
  directory and writes it again. Claude reads a directory-marketplace plugin
  *live* from its source, so a session open during `uze install` can read a
  plugin that is half there.
- **A harness reads the Store.** Loose skills reach Store bytes through their
  extras links, and the explicit catalogue for Claude and Codex is rooted in
  the Store itself. The Store is the one tier whose loss costs the packages;
  it is also the tier a harness should never be able to reach.

Disk is not free either: 140 KB of plugin source becomes ~1.35 MB across
~300 files on a four-harness machine, 4.8x of it in the generated tier. Most
of that is the harnesses' own caches, which UZE cannot avoid; what UZE can
avoid is making the choice of mechanism on the wrong axis.

The rule the measurements support is short: **what UZE renders is a real
file; what UZE carries unchanged is a real copy; nothing a harness reads
points into the Store; and a generated directory is replaced whole, never
edited in place.**

Materialization, first part:

- **A loose Skill is a real directory in the harness's discovery root**,
  owned through a new receipt `ManagedArtifact::GeneratedTree { path,
  digest }`: the rendered `SKILL.md`, the policy sidecar a harness needs, and
  every supporting file copied from the Store. Inspection is content
  identity (`tree_sha256` of the directory against the receipt), so an edit
  is Drifted and a removal is Missing, exactly as for a generated file.
- **Global delivery goes to each harness's own root.** Codex keeps
  `~/.agents/skills`, the user root it documents; OpenCode moves to
  `~/.config/opencode/skills`. No directory has two owners, and the
  shared-root machinery (the two-vendor wrapper, reuse of another
  integration's artifact, shared-aware retirement) is removed.
- **Supporting files are copied, never linked.** `link_extras` and the Claude
  shim's `SKILL.md` link are replaced by the envelope mirror
  `deliver-the-whole-plugin` introduced, with the same containment rule (a
  link that escapes the package is refused by name). `std::fs::copy` already
  clones on filesystems that support it (btrfs, XFS, APFS, ReFS), so copying
  costs bytes only where the filesystem cannot share them.
- **Every generated directory is replaced atomically**: built beside its
  destination and swapped in with one rename (`renameat2(RENAME_EXCHANGE)`
  where available, two renames otherwise). A reader sees the old tree or the
  new one, never a mix.
- **A receipt an earlier build made in another shape is retired** before the
  current shape is attached for the same capability, by the same
  inspect-before-detach rule as a renamed receipt: a matched symlink goes, a
  changed one is left and the capability is held back with the reason.

Materialization, second part:

- **Part 2, the harness never reads the Store.** The explicit catalogue and
  its envelopes move out of `store/` into the generated tier; reference-driven
  pruning of the generated tier; a hardlink materializer offered only as a
  measured optimization on top of a copy, never from the Store.
- **Part 3 moved out**: the per-harness dialect table is
  `harness-dialect-table`'s, and the canonical `harness:` frontmatter is
  `add-portable-agent-capability`'s; neither is needed by this change.
  Project `.agents/` (read and mounted, never written) is not part of either.

## Capabilities

### New Capabilities

- `package-delivery-fidelity`: what a delivered plugin contains, how its
  capabilities are named in the harness, when a capability is delivered once
  and only once, and which labels a vendor registry is given.

### Modified Capabilities

- `transparent-harness-attachment`: an attachment is a UZE-owned artifact
  materialized from the Store, not a reference into it; rendered and carried
  files are physical; a generated directory is replaced atomically.
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

Materialization:

- `crates/uze-core/src/delivery/exposure.rs`: `GeneratedTree` variant with
  attach / inspect / detach / fingerprint / `is_in_place`; ledger comment for
  shape 3 (unreleased, so no new rung).
- `crates/uze-core/src/persistence.rs`: `replace_dir` (stage and swap).
- `crates/uze-integrations/src/shared/{skill,tree,marketplace}.rs`: copy
  instead of `link_extras`; every `recreate_dir` becomes a staged swap.
- `crates/uze-integrations/src/{claude,codex,opencode,antigravity}/skills.rs`:
  loose skills planned as `GeneratedTree`; OpenCode's global skill root.
- Removed: `shared_agent_skill_root`, `resolved_artifact_target`,
  `write_superset_skill_wrapper`, `verify_reused_wrapper` and their callers.
- `crates/uze-application/src/application/lifecycle/attach.rs`: retirement of
  receipts in an earlier shape; `same_name_space` covers the new variant.
- Lab: `conformance/experiments/*/study_{mechanics,discovery}*.py` are the
  evidence this change rests on; OpenCode's `<skill_files>` becomes a contract
  check.
- No CLI surface change. The per-harness route report is unchanged; what
  changes is what is on disk behind it.

