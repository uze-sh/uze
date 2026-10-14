## 1. Probes (record each outcome as a `Measured:` line in design.md)

- [ ] 1.1 Antigravity 1.3.4: install from `antigravity-plugins-official`; record manifest vs `plugin list --json` presence and shape, whether a same-name import coexists, and which one `agy plugin uninstall <name>` removes (D3)
- [ ] 1.2 Claude current: no `CLAUDE.md` and no runtime import; `CLAUDE.local.md` shadowing; a launch inside `.worktrees/<id>` to see which `AGENTS.md`/`CLAUDE.md` reaches the model (D4)
- [ ] 1.3 OpenCode 2.0.28: read the session-id schema; two launches with one minted id continue; `GET /api/session/<id>` answers outside the first page; `/new` in the TUI is visible to `observe()` (D5)
- [ ] 1.4 Claude, no UZE: missing hook command with `onFailure: "block"` blocks; 2.1.294 accepts or rejects the key (D6)
- [ ] 1.5 Codex 0.162.1 and OpenCode 2.0.28: model creates a worktree through the harness tool; record where it lands (D7)
- [ ] 1.6 Antigravity 1.3.4 `--experiment`: plugin with root `hooks.json` and `agents.json`; hooks load and act; subagent dispatches and reaches the plugin MCP (D8)
- [ ] 1.7 Windows Sandbox, Codex ≥0.162.1: shell PreToolUse fires, deny and transform hold (D9)
- [ ] 1.8 OpenCode 2.0.28: a skill with `disable-model-invocation: true` in `~/.claude/skills` is hidden from the model (task 8.2)
- [ ] 1.9 Antigravity 1.3.4: generated plugin with a remote MCP under `url`; server appears and its tool reaches the model (D10)
- [ ] 1.10 Vendor files beside `AGENTS.md`: OpenCode 2.0.28 with only `CLAUDE.md`, and with both; Antigravity 1.3.4 with `GEMINI.md` and `AGENTS.md` both at the root (additive, or one wins?); record which reach the model so each harness's `shadowed_by` is measured, not assumed (D4)
- [ ] 1.11 Prune what was refuted: 1.1 → antigravity census scenarios; 1.3 → `agent-session-continuity` "named by UZE" scenario and section 4; 1.4 → `portable-hooks` block-on-failure clause and section 5; 1.5 → `checkout-accounting` outside-place scenario and section 6; 1.9 → antigravity MCP scenario reverts to `serverUrl`; edit the affected decision in place

## 2. Harness version floor

- [ ] 2.1 Port: each integration declares its minimum version; core compares dotted versions without naming a vendor
- [ ] 2.2 `uze doctor` and `uze status` report a harness below its floor with both versions; an unreadable version reports nothing
- [ ] 2.3 A delivery refused because of the floor names the version needed
- [ ] 2.4 Tests: below, at, above, unreadable; classify any new read in `command_performance.rs` if it touches a command's path

## 3. Antigravity census (break)

- [ ] 3.1 Install preflight and detach read one `agy plugin list --json` census per operation (array shape), matching by name and staged path
- [ ] 3.2 Two plugins of one name: inspection Conflict, detach removes nothing
- [ ] 3.3 Keep the manifest read (`{"imports": …}`, `null`) for budgeted inspection; delete only the 1.1.x text reading and `No imported plugins.`; an unknown CLI shape blocks
- [ ] 3.4 Unit tests: new shape, unknown shape, marketplace plugin of the target name, coexisting twin
- [ ] 3.5 Lab check: a marketplace-installed same-name plugin makes UZE refuse install and detach
- [ ] 3.6 Antigravity floor 1.3.4; fix "verified against 1.1.19" in the module doc

## 4. Claude context native

- [ ] 4.1 Core: `ContextDelivery::Native { files, shadowed_by }`; Claude declares the three shadowing files, others `&[]`
- [ ] 4.2 Delete `ContextDelivery::Bridge` and what only it used (bridge plan/reconcile, `BridgePlan`, `BridgeStatus`, `ContextMechanism::Bridge`, `ResourceDelivery::Bridged`, `HarnessContextDelivery` `BRIDGE`/`PROJECTED`)
- [ ] 4.3 Shadow detection at the root: own content shadows; empty, `@AGENTS.md`/`@./AGENTS.md`, a symlink to the project's `AGENTS.md`, or only an earlier `instruction-bridge` region does not
- [ ] 4.4 `ShadowedBy { file }` as a portability gap in `status`, `doctor`, the TUI (including the "Bridged" legend in `src/ui/overlay.rs`) and `agent context inspect`; drop `bridges` from `agent context plan|reconcile --format json`
- [ ] 4.5 Claude runtime shim: no `AGENTS.md` projection, no `CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD`, active only with `.agents/`; update the doc in `uze-core/src/machine/harness_runtime.rs`
- [ ] 4.6 `/uze:init` Flow B: migrate the portable core into `AGENTS.md`, inline Claude `@path` imports, end with `CLAUDE.md` = `@AGENTS.md` + Claude-only lines (deletion offered, not done); nested `CLAUDE.md` → nested `AGENTS.md`
- [ ] 4.7 `/uze:init` shadowed flow: `AGENTS.md` + shadowing `CLAUDE.md` merged into `AGENTS.md`, each disagreement asked; `CLAUDE.local.md` reported, never merged
- [ ] 4.8 `/uze:init` rest of #154's rework: `PARTIALLY_PORTABLE` routing to the shadowed flow, `PORTABLE` summary without "bridged", MUST NOT lines, `plugins/uze/plugin.json` description; a test that reconcile and install leave a shadowing `CLAUDE.md` byte-identical
- [ ] 4.9 Claude floor 2.1.281 (2.1.295 if 1.4 forces it)
- [ ] 4.10 Deterministic tests: `tests/integrations/runtime_projection.rs`, `tests/memory/inspection.rs`, `tests/memory/reconciliation.rs`, `src/ui` context rows, a shadowing table covering every case in 4.3
- [ ] 4.11 Lab: `context-agents-md-reaches-model` with no bridge; new `context-shadowed-by-claude-local`
- [ ] 4.12 Docs: Claude README (root-only shadowing caveat), `docs/capabilities/context-manager.md`, `invariants.md`, site context and harness pages, `03-context` journey

## 5. OpenCode assigned continuity

- [ ] 5.1 Port `new_session_id()` defaulting to `SessionId::generate()`; `continuity::start` uses it; correct the doc on `generate`
- [ ] 5.2 OpenCode: mint the schema shape, pass `--session`, declare `SessionContinuity::Assigned`
- [ ] 5.3 `exists()` asks `GET /api/session/<id>`; `observe()` keeps following newer sessions in the cwd since launch
- [ ] 5.4 Tests: an id outside the first page is continued; a gone id starts fresh with the reason once; `/new` is followed
- [ ] 5.5 Lab continuity check for OpenCode in the assigned shape

## 6. Claude block-on-failure

- [ ] 6.1 Hook dialect gains its native failure marker; Claude writes `"onFailure": "block"` on deny/ask/transform groups only
- [ ] 6.2 A delivered entry whose current rendering differs from its receipt is Stale and rewritten by install/setup; upgrade test from an entry without the marker
- [ ] 6.3 Unit tests: observational groups carry no marker; other targets unchanged
- [ ] 6.4 Contract: "wrapper removed" variant of `_fail_closed`; targets without a marker declare it through `bindings.unsupported` with a reason
- [ ] 6.5 `invariants.md` entry naming that check; `docs/capabilities/portable-hooks.md` table

## 7. Harness worktree places

- [ ] 7.1 Core type `WorktreePlace::{InCheckout, Outside}` returned resolved by the port; Claude keeps `.claude/worktrees`
- [ ] 7.2 Codex declares `command_home/.codex/worktrees`; OpenCode the place measured in 1.5, through `uze-platform` if it is a per-OS data directory
- [ ] 7.3 Accounting (`checkouts.rs`, `accounting.rs`) classifies an outside place as the harness's; tests from the primary and from a slot
- [ ] 7.4 Lab check: a model-created worktree is shown as the harness's and the slot record is untouched

## 8. Measured moves and small corrections

- [ ] 8.1 Antigravity vendor leg: check that pins where hooks load (plugin `hooks.json` or shared file); replace the `phase_hooks_delivery` reference in `scenarios.py`
- [ ] 8.2 OpenCode `hand_written_controls`: add `disable-model-invocation` (if 1.8 confirmed)
- [ ] 8.3 If 1.6 confirmed hooks: deliver them in the generated plugin; retire shared-file receipts first; amend the `exact coverage` requirement and `antigravity.rs` evidence strings; update `conformance/DECISIONS.md` and the README
- [ ] 8.4 If 1.6 confirmed agents: deliver through the generated plugin's `agents.json`; Lab check dispatches the subagent
- [ ] 8.5 If 1.7 confirmed: remove Codex's PowerShell `Unfired` and its test; otherwise update its `why`
- [ ] 8.6 If 1.9 confirmed: keep `url`, rewrite `httpUrl` to `url`; correct `antigravity-compatibility.md:41,52`, `generate.rs`, `antigravity.rs:37`, the Antigravity README
- [ ] 8.7 Stale text: `claude/hooks.rs` "Input rewriting is not yet claimed"; `opencode.rs` hook-plan evidence; OpenCode README "Limitations"; Antigravity `SessionStart`/`overwrite` "undocumented" in hooks.rs and FACTS
- [ ] 8.8 `harness-watch` skill: OpenCode V2 source is the `dev` branch, releases are tags `v2.0.N`

## 9. Evidence and gate

- [ ] 9.1 Re-record the four gates on the current releases; bump `FACTS` `VERSION`/`measured_on`, tool snapshots and `expected.json` so they agree
- [ ] 9.2 Regenerate the harness matrix (context, continuity and hooks cells move)
- [ ] 9.3 `make check`, `cargo test --workspace --no-fail-fast`, `cargo clippy --all-targets -- -D warnings`, `openspec validate --all --strict`
- [ ] 9.4 Close PR #154 pointing at this change's PR
