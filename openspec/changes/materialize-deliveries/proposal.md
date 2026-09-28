## Why

`deliver-the-whole-plugin` made the plugin UZE generates complete. What it
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

## What Changes

Phase 1, in this change and for beta.4:

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

Phase 2, in this change too:

- **Phase 2, the harness never reads the Store.** The explicit catalogue and
  its envelopes move out of `store/` into the generated tier; reference-driven
  pruning of the generated tier; a hardlink materializer offered only as a
  measured optimization on top of a copy, never from the Store.
- **Phase 3 moved to `harness-dialect-table`**: the per-harness dialect
  table, canonical `harness:` frontmatter and project `.agents/` wait on
  product decisions this change does not need.

## Capabilities

### New Capabilities

<!-- none -->

### Modified Capabilities

- `transparent-harness-attachment`: an attachment is a UZE-owned artifact
  materialized from the Store, not a reference into it; rendered and carried
  files are physical; a generated directory is replaced atomically.

## Impact

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
