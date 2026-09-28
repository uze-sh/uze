## 1. The whole package in the envelope

- [x] 1.1 Generated envelopes are a mirror of the Store package plus the
  generated files; no link into the Store. One mirror (`shared::tree`),
  shared with the Codex envelope, which had its own; links escaping the
  package are refused by name.
- [x] 1.2 Receipts for generated envelopes fingerprint content; inspect
  reports drift on an edited copy. Decided otherwise, and recorded in
  section 8 below: a generated envelope lives in the generated
  tier, which is rebuilt, never edited, so there is no drift to report
  there; it is replaced whole (`persistence::replace_dir`) on every
  publication. Content identity is carried where an operator can edit:
  a delivered Skill directory (`GeneratedTree`, `tree_sha256`) and a
  delivered agent file (`GeneratedFile`).
- [x] 1.3 Test: a package with `phases/`, `hooks/<script>` has them under the
  envelope root; Claude-only component paths are left out.

## 2. Agents inside the plugin

- [x] 2.1 `generatable` includes `agents/` where the dialect carries agents
  (Claude); generated plan provides every Agent resource; envelope carries
  `agents/`.
- [x] 2.2 Agents covered by a plan are not delivered to `~/.claude/agents`;
  an install whose plugin was already attached retires the loose receipts
  the plan now covers (inspect-before-detach).
- [x] 2.3 Test: package with agents only is generated as a plugin.
- [x] 2.4 Fields Claude ignores on a plugin agent (`permissionMode`, `hooks`,
  `mcpServers`, `initialPrompt`) are reported as a Degraded shortfall.

## 3. One route per capability

- [x] 3.1 `claude_exact_coverage` follows Claude's discovery: `skills/`
  always, `agents/` unless listed, `.mcp.json` merged with `mcpServers`
  (object, path or list), hooks shadowed by the envelope's own. Tests per
  default; the tests that fixed the old assumption were rewritten.
- [x] 3.2 Canonical `hooks.json` beside an envelope with hooks: covered by
  the envelope, not delivered a second time.
- [x] 3.3 Test: explicit envelope with no `skills` field and `.mcp.json`
  covers every skill and the server.

## 4. Valid registry labels

- [x] 4.1 Claude's MCP label is `<plugin>-<server>`, then
  `<plugin>-<marketplace>-<server>`, within `[A-Za-z0-9_-]`.
- [x] 4.2 Fake `claude` refuses names outside Claude's rule; contract test
  updated.
- [x] 4.3 Codex/OpenCode/Antigravity keep the qualified form their
  configuration accepts (Lab verticals pass MCP on all three).

## 5. `${PLUGIN_ROOT}` in text

- [x] 5.1 Resolved to the Store package root in every delivered `SKILL.md`
  (Claude envelope and shim, Codex envelope and wrapper, OpenCode and
  Antigravity wrappers) and agent definition (all four), through one
  resolver (`shared::package_root`).
- [x] 5.2 Documented in the web plugin-format page and the `uze:author` skill.

## 6. Loose fallback evidence

- [x] 6.1 The install report names each harness's route and why
  (`honest-delivery-reports`).
- [x] 6.2 Test: publication failure yields loose delivery with the reason.

## 7. Evidence

- [ ] 7.1 `conformance/experiments/claude/parity.py`: 15/16 on the build
  before SessionStart (the one gap), native vs UZE-format and native vs the
  author's own manifest (`dual`). Promote into the Claude vertical after 3
  clean runs (ADR-035).
- [x] 7.1a Parity fixture gains the `dual` form (native plugin plus UZE's
  manifests, installed through UZE): the report's first round.
- [x] 7.1b Claude-only component paths are excluded from the envelope.
- [x] 7.2 Integration READMEs corrected (agents routed; routes and coverage).
- [x] 7.3 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --workspace --no-fail-fast`.

## 8. Materialization, part 1: physical, whole, and never into the Store

- [x] 8.1 `persistence::replace_dir`: stage beside the destination, swap with
  `RENAME_EXCHANGE` on Linux and two renames elsewhere; clean a failed or
  stale staging directory. Tests for success, failure and a stale stage.
- [x] 8.2 Every generated directory goes through it: `materialize_generated_package`,
  Antigravity's `materialize_generated_plugin`, the skill wrappers.
- [x] 8.3 Supporting files are copied through the envelope mirror; `link_extras`
  and the Claude shim's `SKILL.md` link are removed. No path under a harness
  root or the generated tier resolves into `store/` (test walks both).
- [x] 8.4 `ManagedArtifact::GeneratedTree { path, digest }`: attach, inspect,
  detach, fingerprint, `is_in_place`, `exposure_name`, `location`;
  `same_name_space`; maintenance and repair treat it as integration-attached.
- [x] 8.5 Loose skills on all four integrations are planned and attached as
  `GeneratedTree`, each in its harness's own global root (OpenCode moves to
  `~/.config/opencode/skills`).
- [x] 8.5a Remove the shared-root machinery: `shared_agent_skill_root`,
  `resolved_artifact_target`, the two-vendor wrapper, `verify_reused_wrapper`,
  shared-aware retirement.
- [x] 8.6 A receipt in an earlier shape is retired before the current shape is
  attached (inspect-before-detach); CLI test upgrading a symlinked skill.
- [x] 8.7 Lab: OpenCode `<skill_files>` non-empty for a delivered skill with
  references, as a contract check. `experiments/opencode/skill_files`: 2/2
  on opencode 2.0.18 (2026-09-28), run in the OpenCode vertical.
- [x] 8.8 Docs: integration READMEs and the web delivery page describe the
  materialized routes.
- [x] 8.9 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --workspace --no-fail-fast`,
  `openspec validate deliver-the-whole-plugin --strict`.

## 9. Materialization, part 2: no harness reads the Store

- [x] 9.1 Explicit envelopes mirrored into the generated tier; the explicit
  catalogue rooted there; Codex's relative `source.path` rule kept. A
  Store-rooted `uze-local` from an earlier build is re-pointed: measured
  2026-09-28, Claude 2.1.283 takes a second `marketplace add` of the same
  name as the new source, Codex 0.158.0 refuses it until `marketplace remove`
  (which uninstalls its plugins); the attach does the second when the first
  fails, and the old receipts are retired by `package_receipt_serves`.
- [x] 9.2 Reference-driven pruning of the generated tier; report-only audit of
  `store/`.
- [x] 9.3 Measure clone/reflink on the Lab filesystem; decide whether a
  hardlink materializer from a read-only generated copy is worth offering.
  Measured 2026-09-28: WSL's ext4 (`cp --reflink=always`: not supported) and
  Docker's overlayfs have no clone, so a copy costs its bytes there. The
  bytes a copy adds are the supporting files of loose Skills only (see
  design); no hardlink materializer until a measurement shows those bytes
  matter.

## 10. Materialization, part 3

- [x] 10.1 Moved to its own change, `harness-dialect-table`: the per-harness
  table, canonical `harness:` frontmatter, Agent Plugins 1.0 conformance and
  project `.agents/` are one design that waits on product decisions (the
  frontmatter shape, who owns a UZE-written file in a repository), not on
  this change's materialization.
