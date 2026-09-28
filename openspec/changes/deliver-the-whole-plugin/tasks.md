## 1. The whole package in the envelope

- [x] 1.1 Generated envelopes are a mirror of the Store package plus the
  generated files; no link into the Store. One mirror (`shared::tree`),
  shared with the Codex envelope, which had its own; links escaping the
  package are refused by name.
- [x] 1.2 Receipts for generated envelopes fingerprint content; inspect
  reports drift on an edited copy. Decided otherwise, and recorded in
  `materialize-deliveries`: a generated envelope lives in the generated
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
