## 1. Probes (record each outcome as a `Measured:` line in design.md)

- [ ] 1.1 Antigravity 1.3.4: install from `antigravity-plugins-official`; record manifest vs `plugin list --json` presence and shape, whether a same-name import coexists, and which one `agy plugin uninstall <name>` removes (D3)
- [ ] 1.2 OpenCode 2.0.28: read the session-id schema; two launches with one minted id continue; `GET /api/session/<id>` answers outside the first page; `/new` in the TUI is visible to `observe()` (D4)
- [ ] 1.3 Claude, no UZE: missing hook command with `onFailure: "block"` blocks; 2.1.294 accepts or rejects the key (D5)
- [ ] 1.4 Codex 0.162.1 and OpenCode 2.0.28: model creates a worktree through the harness tool; record where it lands (D6)
- [ ] 1.5 Antigravity 1.3.4 `--experiment`: plugin with root `hooks.json` and `agents.json`; hooks load and act; subagent dispatches and reaches the plugin MCP (D7)
- [ ] 1.6 Windows Sandbox, Codex ≥0.162.1: shell PreToolUse fires, deny and transform hold (D8)
- [ ] 1.7 OpenCode 2.0.28: a skill with `disable-model-invocation: true` in `~/.claude/skills` is hidden from the model (task 7.2)
- [ ] 1.8 Antigravity 1.3.4: generated plugin with a remote MCP under `url`; server appears and its tool reaches the model (D9)
- [ ] 1.9 Prune what was refuted: 1.1 → antigravity census scenarios; 1.2 → `agent-session-continuity` "named by UZE" scenario and section 4; 1.3 → `portable-hooks` block-on-failure clause and section 5; 1.4 → `checkout-accounting` outside-place scenario and section 6; 1.8 → antigravity MCP scenario reverts to `serverUrl`; edit the affected decision in place

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

## 4. OpenCode assigned continuity

- [ ] 4.1 Port `new_session_id()` defaulting to `SessionId::generate()`; `continuity::start` uses it; correct the doc on `generate`
- [ ] 4.2 OpenCode: mint the schema shape, pass `--session`, declare `SessionContinuity::Assigned`
- [ ] 4.3 `exists()` asks `GET /api/session/<id>`; `observe()` keeps following newer sessions in the cwd since launch
- [ ] 4.4 Tests: an id outside the first page is continued; a gone id starts fresh with the reason once; `/new` is followed
- [ ] 4.5 Lab continuity check for OpenCode in the assigned shape

## 5. Claude block-on-failure

- [ ] 5.1 Hook dialect gains its native failure marker; Claude writes `"onFailure": "block"` on deny/ask/transform groups only
- [ ] 5.2 A delivered entry whose current rendering differs from its receipt is Stale and rewritten by install/setup; upgrade test from an entry without the marker
- [ ] 5.3 Unit tests: observational groups carry no marker; other targets unchanged
- [ ] 5.4 Contract: "wrapper removed" variant of `_fail_closed`; targets without a marker declare it through `bindings.unsupported` with a reason
- [ ] 5.5 `invariants.md` entry naming that check; `docs/capabilities/portable-hooks.md` table

## 6. Harness worktree places

- [ ] 6.1 Core type `WorktreePlace::{InCheckout, Outside}` returned resolved by the port; Claude keeps `.claude/worktrees`
- [ ] 6.2 Codex declares `command_home/.codex/worktrees`; OpenCode the place measured in 1.4, through `uze-platform` if it is a per-OS data directory
- [ ] 6.3 Accounting (`checkouts.rs`, `accounting.rs`) classifies an outside place as the harness's; tests from the primary and from a slot
- [ ] 6.4 Lab check: a model-created worktree is shown as the harness's and the slot record is untouched

## 7. Measured moves and small corrections

- [ ] 7.1 Antigravity vendor leg: check that pins where hooks load (plugin `hooks.json` or shared file); replace the `phase_hooks_delivery` reference in `scenarios.py`
- [ ] 7.2 OpenCode `hand_written_controls`: add `disable-model-invocation` (if 1.7 confirmed)
- [ ] 7.3 If 1.5 confirmed hooks: deliver them in the generated plugin; retire shared-file receipts first; amend the `exact coverage` requirement and `antigravity.rs` evidence strings; update `conformance/DECISIONS.md` and the README
- [ ] 7.4 If 1.5 confirmed agents: deliver through the generated plugin's `agents.json`; Lab check dispatches the subagent
- [ ] 7.5 If 1.6 confirmed: remove Codex's PowerShell `Unfired` and its test; otherwise update its `why`
- [ ] 7.6 If 1.8 confirmed: keep `url`, rewrite `httpUrl` to `url`; correct `antigravity-compatibility.md:41,52`, `generate.rs`, `antigravity.rs:37`, the Antigravity README
- [ ] 7.7 Stale text: `claude/hooks.rs` "Input rewriting is not yet claimed"; `opencode.rs` hook-plan evidence; OpenCode README "Limitations"; Antigravity `SessionStart`/`overwrite` "undocumented" in hooks.rs and FACTS
- [ ] 7.8 `harness-watch` skill: OpenCode V2 source is the `dev` branch, releases are tags `v2.0.N`

## 8. Evidence and gate

- [ ] 8.1 Re-record the four gates on the current releases; bump `FACTS` `VERSION`/`measured_on`, tool snapshots and `expected.json` so they agree
- [ ] 8.2 Regenerate the harness matrix (continuity and hooks cells move)
- [ ] 8.3 `make check`, `cargo test --workspace --no-fail-fast`, `cargo clippy --all-targets -- -D warnings`, `openspec validate --all --strict`
