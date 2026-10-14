## 0. Status

- [ ] 0.1 Choose the option in proposal.md (A, B) and revise design.md and these tasks to it

## 1. Probes

- [ ] 1.1 Claude current: no `CLAUDE.md` and no runtime import; `CLAUDE.local.md` shadowing; a launch inside `.worktrees/<id>` to see which `AGENTS.md`/`CLAUDE.md` reaches the model (C1)

## 2. Claude context native (carried from refresh-harness-deliveries)

- [ ] 2.1 Core: `ContextDelivery::Native { files, shadowed_by }`; Claude declares the three shadowing files, others `&[]`
- [ ] 2.2 Delete `ContextDelivery::Bridge` and what only it used (bridge plan/reconcile, `BridgePlan`, `BridgeStatus`, `ContextMechanism::Bridge`, `ResourceDelivery::Bridged`, `HarnessContextDelivery` `BRIDGE`/`PROJECTED`)
- [ ] 2.3 Shadow detection at the root: own content shadows; empty, `@AGENTS.md`/`@./AGENTS.md`, a symlink to the project's `AGENTS.md`, or only an earlier `instruction-bridge` region does not
- [ ] 2.4 `ShadowedBy { file }` as a portability gap in `status`, `doctor`, the TUI (including the "Bridged" legend in `src/ui/overlay.rs`) and `agent context inspect`; drop `bridges` from `agent context plan|reconcile --format json`
- [ ] 2.5 Claude runtime shim: no `AGENTS.md` projection, no `CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD`, active only with `.agents/`; update the doc in `uze-core/src/machine/harness_runtime.rs`
- [ ] 2.6 `/uze:init` Flow B proposes the `@AGENTS.md` line for a shadowing `CLAUDE.md`, as #154 did
- [ ] 2.7 `/uze:init` rest of #154's rework: `PARTIALLY_PORTABLE` routing for `ShadowedBy`, `PORTABLE` summary without "bridged", MUST NOT lines, `plugins/uze/plugin.json` description; a test that reconcile and install leave a shadowing `CLAUDE.md` byte-identical
- [ ] 2.8 Claude floor 2.1.281 (2.1.295 if refresh-harness-deliveries raises it)
- [ ] 2.9 Deterministic tests: `tests/integrations/runtime_projection.rs`, `tests/memory/inspection.rs`, `tests/memory/reconciliation.rs`, `src/ui` context rows, a shadowing table covering every case in 2.3
- [ ] 2.10 Lab: `context-agents-md-reaches-model` with no bridge; new `context-shadowed-by-claude-local`
- [ ] 2.11 Docs: Claude README (root-only shadowing caveat), `docs/capabilities/context-manager.md`, `invariants.md`, site context and harness pages, `03-context` journey
- [ ] 2.12 Close PR #154 pointing at this change's PR
