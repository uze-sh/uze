## Context

See [proposal.md](proposal.md). The event enum is `HookEvent` in
`uze-core/src/capability/hook.rs`, with `deny_unknown_fields` on the
`HookManifest` map; matchers accept tool aliases or `native:`
(`HookMatcher::parse`). Per-harness event sets are `HookTarget::capabilities()`
(`uze-integrations/src/hooks.rs`), read from the `HookTarget` each vertical
declares (`claude::HOOKS`, ...). Before this change the Claude dialect answered
a denial on any event that was not a tool event in the `Stop` form (the
dialect's `deny_document`, rendered by `wrapper_source` in
`uze-integrations/src/hooks/wrapper.rs`), and the OpenCode bridge registers
only `execute.before`/`execute.after` (`opencode_bridge` in
`uze-integrations/src/hooks/bridge.rs`). ADR-040's wrapper is
`exec <root> <event> <effect> …`, env in, exit code out.

## Decisions

### Observe only

A session has no decision to relay: Claude and Codex accept `continue:false`
on SessionStart, but a plugin stopping the user's session on startup is not a
portable semantic worth claiming. Restricting to `observe` keeps the wrapper's
fail-open path the only one, and means a missing `jq` cannot block a session.

### Source is a matcher, not a tool

The matcher grammar gains a per-event rule: tool events keep aliases and
`native:`; `SessionStart` accepts `startup|resume|clear`. The wrapper exports
`HOOK_SOURCE` from the payload's `source` field.

### Antigravity is Unsupported, not adapted

`PreInvocation` with `invocationNum == 1` would approximate it, but the
wrapper is stateless and portable across harnesses; adding a per-event
payload predicate for one vendor is an adapter the precedence rules reserve
for last resort. Revisit if the vendor adds the event.

### OpenCode decided by experiment

The bridge can subscribe to `event` and filter `session.created`. The Lab's
`--experiment` loop decides whether it fires once per new session (and not on
resume); the route is Native only with that evidence, Unsupported otherwise.

## Risks / Trade-offs

- [Claude runs SessionStart hooks concurrently and with a short default
  timeout] → the manifest's per-handler timeout is compiled into the entry;
  the author's script must be idempotent and fast (document it).
- [Receipt enum grows] → additive; an older build never reads a newer record.
