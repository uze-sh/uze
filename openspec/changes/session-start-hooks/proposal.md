## Why

The portable `hooks.json` accepts `PreToolUse`, `PostToolUse` and `Stop`
(ADR-033). A plugin that starts a local, idempotent service when a session
opens — the migrated marketplace's `forge` plugin brings up its UI this way —
is refused outright: `unknown variant SessionStart, expected one of
PreToolUse, PostToolUse, Stop`. The hook cannot be declared, so no harness
runs it, including the two that support it natively.

Vendor state, checked 2026-09-28:

- **Claude Code** — `SessionStart` native, matcher on the source
  (`startup`, `resume`, `clear`, `compact`).
- **Codex CLI** — `SessionStart` native in `hooks.json`, matcher on
  `startup|resume|clear`.
- **Antigravity CLI** — five events (`PreToolUse`, `PostToolUse`,
  `PreInvocation`, `PostInvocation`, `Stop`); no `SessionStart`.
  `PreInvocation` fires per invocation.
- **OpenCode** — no command hooks; a plugin can observe a `session.created`
  event.

## What Changes

- **`SessionStart` joins the portable events**, as an observational event:
  its groups take the `observe` effect only (a session cannot be denied or
  asked about), handlers get the existing `HOOK_*` environment with
  `HOOK_EVENT=session_start`, `HOOK_SOURCE` (`startup`, `resume`, `clear`,
  when the harness says) and no tool fields, and a failure is reported and
  never blocks the session.
- **An optional portable matcher on the source** (`startup`, `resume`,
  `clear`); no matcher means every source.
- **Per-harness delivery**, each claim held by the Lab:
  Claude Code and Codex native; OpenCode through the generated bridge on
  `session.created` if the Lab proves it fires once per new session, else
  Unsupported; Antigravity Unsupported with the reason ("no session-start
  event"), since emulating it on `PreInvocation` would run on every turn
  unless the wrapper keeps per-session state, which it does not.
- **An event a harness lacks is reported, not refused.** A manifest with
  `SessionStart` installs everywhere; the harness without it gets the other
  events and an Unsupported line for this one.
- Out of scope: injecting context into the session from the handler's
  output (the exit-code contract has no output channel; its own change).

## Capabilities

### New Capabilities

<!-- none -->

### Modified Capabilities

- `portable-hooks`: a fourth event with an observational-only contract and a
  source matcher; unsupported events are per-harness evidence, not a manifest
  error. (The capability is introduced by the in-progress
  `native-first-hooks`; this change adds requirements beside it.)

## Impact

- `crates/uze-core/src/capability/hook.rs`: `HookEvent::SessionStart`,
  `abi_name`, parse rule (observe only, source matcher).
- `crates/uze-core/src/delivery/exposure.rs`: `HookConfigEntry { event }`
  gains a variant — receipts are records; additive, no ladder rung needed
  unless an older build must read it (it must not: a newer record is never
  taken).
- `crates/uze-integrations/src/hooks.rs`: capabilities per target, event
  names, Claude/Codex dialect for a toolless event, OpenCode bridge
  registration; goldens.
- ADR-033's event list is extended by a dated note at archive time.
- Conformance: a `SessionStart` case in the Claude and Codex verticals; an
  OpenCode experiment deciding its route.
