## 1. Canonical capability and resource model

- [x] 1.1 Discover every Markdown file below `agents/` (subdirectories
  included) from package and project inputs, compose one
  `CapabilityKind::Agent` resource per definition, and preserve bytes.
- [x] 1.2 Define Agent resource identity, display name, inspection payload,
  and capability routing tests without adding vendor knowledge to Core.
- [x] 1.3 Extend route/identity/conformance fixtures for native, degraded
  (fields not carried), per-harness block, label, and byte-preservation
  cases.

## 2. Native delivery

- [x] 2.1 Implement Claude Code native Agent exposure, receipts, inspection,
  safe detach, drift behavior, and focused integration tests.
- [x] 2.2 Implement OpenCode native Agent exposure, receipts, inspection,
  safe detach, drift behavior, and focused integration tests.
- [x] 2.3 Implement Antigravity CLI native Agent exposure, receipts,
  inspection, safe detach, drift behavior, and focused integration tests.
- [x] 2.4 Implement Codex's generated native TOML Agent projection, receipts,
  inspection, safe detach, and focused integration tests.
- [x] 2.5 Reuse qualified naming and detect cross-harness physical-root
  conflicts before mutating any Agent artifact: agents go through
  `resolve_exposure_name` like Skills (it used to return early for them), label
  `<plugin>:<subdir>…:<name>` with `name` from frontmatter, else the stem.
- [x] 2.6 Claude: agents delivered inside the plugin
  (`deliver-the-whole-plugin` §2); no loose `~/.claude/agents` file when a
  plugin carries them.
- [x] 2.7 OpenCode: generated agent file named `<plugin>:<agent>.md`
  carrying `description`, `mode: subagent` and the checked fields of its
  `harness.opencode` block, and no other root field; report each root field
  not carried as Degraded.
- [x] 2.8 Antigravity and Codex: Lab experiment on the qualified label. Both
  accept it (frontmatter `name` / TOML `name`), so no `<plugin>-<agent>`
  fallback exists; root fields not carried are reported as Degraded.

## 3. Product reporting and documentation

- [x] 3.4 `plugin check` validates `agents/*.md` (YAML, `description`, stem).
- [x] 3.5 `agent.md.template` carries `name`/`description` frontmatter with
  the commented field documentation the other templates have.
- [x] 3.6 `uze:author` skill and `plugins/uze/README.md` document agents:
  format, portable fields, qualified label, what each harness keeps.
- [x] 3.7 Correct the per-integration READMEs (agents are routed — done) and the
  generated harness matrix to the proven routes (not "Native" everywhere). The
  matrix is generated from the integrations (`uze-harness-matrix --check`
  passes); every harness now has a native agent surface (Claude plugin
  agents, Codex TOML, OpenCode `agents/`, Antigravity's agents directory),
  and a field a harness drops is reported per agent as Degraded at install.

- [x] 3.1 Add the Agents capability to TUI compatibility rows and verify
  native/adapted status rendering.
- [x] 3.2 Update the harness-matrix generator and regenerate its output
  (`web/content/docs/reference/harnesses.mdx`, `web/lib/harness-matrix.json`)
  so all four harnesses are Native for Agents.
- [x] 3.3 Confirm `docs/adr/031-adopt-a-canonical-portable-agent-capability.md`
  remains present and linked from the ADR index.

## 4. Conformance and verification

- [x] 4.1 Add a small canonical Agent fixture and deterministic integration /
  acceptance coverage for composed multi-capability packages.
- [x] 4.2 Add the agent contract (`conformance/contract/agent.py`) to each
  Claude, Codex, OpenCode, and Antigravity conformance vertical: each fixture
  agent (flat, nested, renamed, and one Claude-style agent with
  `model: haiku`, `tools: Read, Grep` and a `harness:` block) is offered to
  the model under its qualified label, its body arrives when dispatched by
  that label, and the vendor-fields agent runs on the model its block names.
- [x] 4.3 Run each conformance vertical against its real harness and fix all
  failures before promoting matrix evidence. 2026-09-28, image built from
  this branch: Claude Code 2.1.283 (0 failures), codex-cli 0.158.0 (68/68),
  OpenCode 2.0.18 (55/55), agy 1.2.12 (62/62, 0 ADAPTED).
- [x] 4.4 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --no-fail-fast`, and `openspec validate add-portable-agent-capability --strict`.
