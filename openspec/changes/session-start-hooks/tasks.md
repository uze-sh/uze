## 1. Canonical event

- [x] 1.1 `HookEvent::SessionStart` (`session_start`), observe-only parse
  rule, source matcher; `plugin check` messages.
- [x] 1.2 Unsupported event per harness is evidence, never a manifest error.

## 2. Delivery

- [x] 2.1 Claude and Codex: native `SessionStart` entry, wrapper exports
  `HOOK_SOURCE`, no tool fields; fix the dialect's unknown-event fallthrough
  to `Stop`. Goldens.
- [x] 2.2 Antigravity: Unsupported with reason.
- [x] 2.3 OpenCode: `--experiment` on `session.created`; bridge registration
  if proven, else Unsupported. (Measured on 2.0.18: the plugin event stream
  never carries `session.created`; Unsupported.)

## 3. Evidence and docs

- [ ] 3.1 Lab: session-start case in Claude and Codex verticals. (Proven as
  `experiments/{claude,codex}/session-start` with the `hook-session-plugin`
  fixture; promotion into the verticals pending.)
- [x] 3.2 `docs/capabilities/portable-hooks.md` and the web hooks page list
  the event, its effect rule and per-harness status.
- [x] 3.3 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --workspace --no-fail-fast`,
  `openspec validate session-start-hooks --strict`.
