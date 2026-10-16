## 1. Probes (record each outcome as a `Measured:` line in design.md)

- [x] 1.1 Antigravity 1.3.4: install from `antigravity-plugins-official`; record manifest vs `plugin list --json` presence and shape, whether a same-name import coexists, and which one `agy plugin uninstall <name>` removes (D3)
- [x] 1.2 OpenCode 2.0.28: read the session-id schema; two launches with one minted id continue; `GET /api/session/<id>` answers outside the first page; `/new` in the TUI is visible to `observe()` (D4)
- [x] 1.3 Claude, no UZE: missing hook command with `onFailure: "block"` blocks; 2.1.294 accepts or rejects the key (D5)
- [x] 1.4 Codex 0.162.1 and OpenCode 2.0.28: model creates a worktree through the harness tool; record where it lands (D6)
- [x] 1.5 Antigravity 1.3.4 `--experiment`: plugin with root `hooks.json` and `agents.json`; hooks load and act; subagent dispatches and reaches the plugin MCP (D7)
- [x] 1.6 Windows Sandbox, Codex ≥0.162.1: shell PreToolUse fires, deny and transform hold (D8)
- [x] 1.7 OpenCode 2.0.28: a skill with `disable-model-invocation: true` in `~/.claude/skills` is hidden from the model (task 7.2)
- [x] 1.8 Antigravity 1.3.4: generated plugin with a remote MCP under `url`; server appears and its tool reaches the model (D9)
- [x] 1.9 Prune what was refuted: 1.1 → antigravity census scenarios; 1.2 → `agent-session-continuity` "named by UZE" scenario and section 4; 1.3 → `portable-hooks` block-on-failure clause and section 5; 1.4 → `checkout-accounting` outside-place scenario and section 6; 1.8 → antigravity MCP scenario reverts to `serverUrl`; edit the affected decision in place (nothing refuted: 1.1-1.8 each confirmed, so no scenario or section was pruned)

## 2. Harness version floor

- [x] 2.1 Port: each integration declares its minimum version; core compares dotted versions without naming a vendor
- [x] 2.2 `uze doctor` and `uze status` report a harness below its floor with both versions; an unreadable version reports nothing
- [x] 2.3 A delivery refused because of the floor names the version needed
- [x] 2.4 Tests: below, at, above, unreadable; classify any new read in `command_performance.rs` if it touches a command's path

## 3. Antigravity census (break)

- [x] 3.1 Install preflight and detach read one `agy plugin list --json` census per operation (array shape), matching by name and staged path
- [x] 3.2 Two plugins of one name: inspection Conflict, detach removes nothing
- [x] 3.3 Keep the manifest read (`{"imports": …}`, `null`) for budgeted inspection; delete only the 1.1.x text reading and `No imported plugins.`; an unknown CLI shape blocks
- [x] 3.4 Unit tests: new shape, unknown shape, marketplace plugin of the target name, coexisting twin
- [x] 3.5 Lab check: a marketplace-installed same-name plugin makes UZE refuse install and detach (measured on the operator's session instead, see D3: the official marketplace needs a signed-in, online session the Lab does not have)
- [x] 3.6 Antigravity floor 1.3.4; fix "verified against 1.1.19" in the module doc

## 4. OpenCode assigned continuity

- [x] 4.1 Port `new_session_id()` defaulting to `SessionId::generate()`; `continuity::start` uses it; correct the doc on `generate`
- [x] 4.2 OpenCode: mint the schema shape, pass `--session`, declare `SessionContinuity::Assigned`
- [x] 4.3 `exists()` asks `GET /api/session/<id>`; `observe()` keeps following newer sessions in the cwd since launch
- [x] 4.4 Tests: an id outside the first page is continued; a gone id starts fresh with the reason once; `/new` is followed
- [x] 4.5 Lab continuity check for OpenCode in the assigned shape

## 5. Claude block-on-failure

- [x] 5.1 Hook dialect gains its native failure marker; Claude writes `"onFailure": "block"` on deny/ask/transform groups only
- [x] 5.2 An entry delivered without the marker is replaced by the next install/setup; upgrade test from an entry without the marker
- [x] 5.3 Unit tests: observational groups carry no marker; other targets unchanged
- [x] 5.4 Contract: "wrapper removed" variant of `_fail_closed`; targets without a marker declare it through `bindings.unsupported` with a reason
- [x] 5.5 `invariants.md` entry naming that check; `docs/capabilities/portable-hooks.md` table

## 6. Harness worktree places

- [x] 6.1 Core type `WorktreePlace::{InCheckout, Outside}` returned resolved by the port; Claude keeps `.claude/worktrees`
- [x] 6.2 Codex declares `command_home/.codex/worktrees`; OpenCode the place measured in 1.4, through `uze-platform` if it is a per-OS data directory
- [x] 6.3 Accounting (`checkouts.rs`, `accounting.rs`) classifies an outside place as the harness's; tests from the primary and from a slot
- [x] 6.4 Lab check: a model-created worktree is shown as the harness's and the slot record is untouched (OpenCode, `experiments/opencode/worktree-tool`; the classification itself is `accounting_tests`)

## 7. Measured moves and small corrections

- [x] 7.1 Antigravity vendor leg: check that pins where hooks load (plugin `hooks.json` or shared file); replace the `phase_hooks_delivery` reference in `scenarios.py`
- [x] 7.2 OpenCode `hand_written_controls`: add `disable-model-invocation` (if 1.7 confirmed)
- [x] 7.3 If 1.5 confirmed hooks: deliver them in the generated plugin; retire shared-file receipts first; amend the `exact coverage` requirement and `antigravity.rs` evidence strings; update `conformance/DECISIONS.md` and the README
- [x] 7.4 If 1.5 confirmed agents: deliver through the generated plugin's `agents.json`; Lab check dispatches the subagent
- [x] 7.5 If 1.6 confirmed: remove Codex's PowerShell `Unfired` and its test; otherwise update its `why` (confirmed: the mechanism went with it, and the Windows entry line now exits with the wrapper's code, without which UZE's deny did not hold)
- [x] 7.6 If 1.8 confirmed: keep `url`, rewrite `httpUrl` to `url`; correct `antigravity-compatibility.md:41,52`, `generate.rs`, `antigravity.rs:37`, the Antigravity README
- [x] 7.7 Stale text: `claude/hooks.rs` "Input rewriting is not yet claimed"; `opencode.rs` hook-plan evidence; OpenCode README "Limitations"; Antigravity `SessionStart`/`overwrite` "undocumented" in hooks.rs and FACTS
- [x] 7.8 `harness-watch` skill: OpenCode V2 source is the `dev` branch, releases are tags `v2.0.N`

## 8. Evidence and gate

- [x] 8.1 Re-record the four gates on the current releases; bump `FACTS` `VERSION`/`measured_on`, tool snapshots and `expected.json` so they agree (2026-10-14 with the final binary: contract and vendor legs green on Claude 2.1.299, Codex 0.162.1, OpenCode 2.0.29, Antigravity 1.3.5; tool snapshots re-recorded, new input fields only)
- [x] 8.2 Regenerate the harness matrix (continuity and hooks cells move)
- [x] 8.3 `make check`, `cargo test --workspace --no-fail-fast`, `cargo clippy --all-targets -- -D warnings`, `openspec validate --all --strict`
