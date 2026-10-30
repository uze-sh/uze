## Why

The review of `fix/security-audit` (2026-10-30) found three structural
problems in the code the audit's fixes added, none of which changes what uze
does. They are recorded here so they are paid down on purpose rather than
forgotten; each makes the next security change in its area safer or cheaper.

## What Changes

- **`redact` leaves `uze-platform`.** `uze_platform::redact` is domain-free
  text processing with provider-specific token prefixes, not an operating
  system question, and `.agents/rules/platform.md` says `uze-platform`
  answers only those. It sits there only because it is the one leaf that
  `uze-git`, `uze-core`, `uze-application` and the binary share.
- **A `CodeIdentity` is never half-built.** `trust::command_execution` returns
  an execution whose `code.digest` is an empty placeholder until
  `trust::package_executions` replaces it. Every integration that answers
  `IntegrationPort::package_executions` gets such values, and two revisions
  compare equal on an empty digest if anything uses them before the
  replacement.
- **A package's tree is hashed only when trust needs it.** An update hashes
  the installed package twice, and an install hashes the materialized tree,
  before `authorize` learns whether the source crosses the trust boundary or
  whether the package executes anything at all.

No behavior changes: the trust decisions, the redaction output and the
consent prompts stay as they are, and the existing tests pin them.

## Capabilities

### New Capabilities
None.

### Modified Capabilities
None. This is a refactoring, so `skip_specs` is set.

## Impact

- `crates/uze-platform/src/redact.rs`, and its callers in `uze-git`,
  `uze-core`, `uze-application` and `src/telemetry.rs`. A new leaf crate, or
  a recorded exception in `.agents/rules/platform.md` (design.md decides).
- `crates/uze-core/src/package/trust.rs` (`CodeIdentity`,
  `command_execution`, `package_executions`) and
  `crates/uze-integrations/src/shared/envelope.rs`.
- `crates/uze-application/src/application/lifecycle/{install,update}.rs`.
