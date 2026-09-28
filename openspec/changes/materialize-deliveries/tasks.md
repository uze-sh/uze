## 1. Phase 1: physical, whole, and never into the Store

- [x] 1.1 `persistence::replace_dir`: stage beside the destination, swap with
  `RENAME_EXCHANGE` on Linux and two renames elsewhere; clean a failed or
  stale staging directory. Tests for success, failure and a stale stage.
- [x] 1.2 Every generated directory goes through it: `materialize_generated_package`,
  Antigravity's `materialize_generated_plugin`, the skill wrappers.
- [x] 1.3 Supporting files are copied through the envelope mirror; `link_extras`
  and the Claude shim's `SKILL.md` link are removed. No path under a harness
  root or the generated tier resolves into `store/` (test walks both).
- [x] 1.4 `ManagedArtifact::GeneratedTree { path, digest }`: attach, inspect,
  detach, fingerprint, `is_in_place`, `exposure_name`, `location`;
  `same_name_space`; maintenance and repair treat it as integration-attached.
- [x] 1.5 Loose skills on all four integrations are planned and attached as
  `GeneratedTree`, each in its harness's own global root (OpenCode moves to
  `~/.config/opencode/skills`).
- [x] 1.5a Remove the shared-root machinery: `shared_agent_skill_root`,
  `resolved_artifact_target`, the two-vendor wrapper, `verify_reused_wrapper`,
  shared-aware retirement.
- [x] 1.6 A receipt in an earlier shape is retired before the current shape is
  attached (inspect-before-detach); CLI test upgrading a symlinked skill.
- [x] 1.7 Lab: OpenCode `<skill_files>` non-empty for a delivered skill with
  references, as a contract check. `experiments/opencode/skill_files`: 2/2
  on opencode 2.0.18 (2026-09-28), run in the OpenCode vertical.
- [x] 1.8 Docs: integration READMEs and the web delivery page describe the
  materialized routes.
- [x] 1.9 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --workspace --no-fail-fast`,
  `openspec validate materialize-deliveries --strict`.

## 2. Phase 2: no harness reads the Store

- [x] 2.1 Explicit envelopes mirrored into the generated tier; the explicit
  catalogue rooted there; Codex's relative `source.path` rule kept. A
  Store-rooted `uze-local` from an earlier build is re-pointed: measured
  2026-09-28, Claude 2.1.283 takes a second `marketplace add` of the same
  name as the new source, Codex 0.158.0 refuses it until `marketplace remove`
  (which uninstalls its plugins); the attach does the second when the first
  fails, and the old receipts are retired by `package_receipt_serves`.
- [x] 2.2 Reference-driven pruning of the generated tier; report-only audit of
  `store/`.
- [x] 2.3 Measure clone/reflink on the Lab filesystem; decide whether a
  hardlink materializer from a read-only generated copy is worth offering.
  Measured 2026-09-28: WSL's ext4 (`cp --reflink=always`: not supported) and
  Docker's overlayfs have no clone, so a copy costs its bytes there. The
  bytes a copy adds are the supporting files of loose Skills only (see
  design); no hardlink materializer until a measurement shows those bytes
  matter.

## 3. Phase 3

- [x] 3.1 Moved to its own change, `harness-dialect-table`: the per-harness
  table, canonical `harness:` frontmatter, Agent Plugins 1.0 conformance and
  project `.agents/` are one design that waits on product decisions (the
  frontmatter shape, who owns a UZE-written file in a repository), not on
  this change's materialization.
