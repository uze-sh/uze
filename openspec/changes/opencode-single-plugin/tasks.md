## 1. Measurements first

- [x] 1.1 Project agents through a global plugin, shared service, OpenCode 2.1.2 (`conformance/experiments/opencode/project-agents-plugin.py`, 14/14)
- [x] 1.2 Cross-package hooks today (`experiments/opencode/hooks-cross-package`, 2.1.2): plugins run in file-name order; a `shell` observer after the denier is skipped; a matcher-less observer after it is handed `shell (refused by a hook: …)` with empty `HOOK_TOOL`/`HOOK_COMMAND`
- [x] 1.3a Plugin runtime: Bun 1.4.2, `Bun.YAML.parse` available with no import (sandbox probe, OpenCode 2.1.2)
- [x] 1.3 Name collision (D5b), `experiments/opencode/project-agents-precedence` (2.1.2, 10/10): a project `build` is skipped; an `.opencode/agents` file of the same id overlays the project's, `system` included; `opencode.json` `permissions` reach a project agent
- [x] 1.3b A linked checkout (`git worktree add`) offers its branch's `.agents/agents`, not the main checkout's (same experiment)
- [x] 1.4 A plugin file written while the service runs is set up in every location, open ones included; a `.json` written into a watched directory reaches existing setups through `fs.watch` (same experiment)

## 2. Runtime plugin

- [x] 2.1 One template for `<config root>/plugins/uze.ts` with sections, each wrapped so a throw ends only that section (D1); a single `install` owning it by header
- [x] 2.2 Move the agent-context section in; retire `agent_context::ensure_plugin`'s own file
- [x] 2.3 Project-agents section (D5): checkout root from `ctx.location.project.directory`, `Bun.YAML` frontmatter, projection into `Agent.Info`, skip a built-in's id (D5b), watch and reload
- [x] 2.4 Fixture table shared by Rust's OpenCode dialect and the section's projection, run under Bun: same carried and withheld fields with the same reasons, values compared after the `Agent.Info` conversion (D5)
- [x] 2.5 Setup deletes earlier generated `uze-agent-context.ts`; README route for project `.agents/agents` becomes Adapted
- [x] 2.6 Reporting (D5c): the fourth `ProjectResourceRoute`, `SetupDelivered`/`SetupMissing`/`Blocked` in place of `RulesEngine`/`RulesEngineMissing`, OpenCode's answer for `Agents` from whether `uze.ts` is UZE's; update the route's doc ("no fourth answer")
- [x] 2.7 Workspace: the harness drawer's `.agents/agents` row and the agent-support popup render the three states (`design-system` skill checklist); `uze doctor --format json` carries the new names

## 3. Hooks and rules

- [x] 3.1 `UzeHome` names OpenCode's hook-document directory (generated tier)
- [x] 3.2 The bridge emits `<package>.json` as the package's receipt-owned artifact instead of `hooks-<package>.ts` (D2)
- [x] 3.3 Hooks section: load and watch documents, one ordered pass across packages (D3), unreadable document denies (D4)
- [x] 3.4 Rules section replaces `uze-rules.ts`, same engine calls
- [x] 3.5 Setup and reconcile converge earlier `hooks-*.ts` and `uze-rules.ts` (D6)
- [x] 3.6 Hook goldens and `opencode_runtime_tests.rs` run against the runtime plugin with documents
- [x] 3.7 `uze doctor` reports an OpenCode hook receipt whose document is missing

## 4. Proof and documentation

- [x] 4.1 Retire the `context-project-agent-reaches-model` declaration in `harnesses/opencode/bindings.py` and its `expected.json` entry; the contract asserts it
- [x] 4.2 Lab scenes for 1.2's two packages (the new ordering), for a document corrupted mid-session, and for a package installed while the service runs
- [x] 4.3 Full OpenCode vertical green (`python3 conformance/lab.py --harness opencode`)
- [x] 4.4 `crates/uze-integrations/src/opencode/README.md`, `docs/capabilities/portable-hooks.md` and the harness matrix (`uze-harness-matrix --check`)
- [x] 4.5 `docs/architecture/agent-memory.mmd` names the runtime plugin; run `make artifacts` and `cargo test -p uze-extensions`
