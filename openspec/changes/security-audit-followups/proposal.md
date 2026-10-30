## Why

The review of `fix/security-audit` (2026-10-30) left two kinds of work. Some items change behavior and were not yet decided. Others are structural problems in the code the audit's fixes added. Two further items are security findings that are still open; they are tracked privately and named here by id only (AGENTS.md, Security findings). The operator asked for the recommended answer to each decision to be recorded as the plan.

## What Changes

**Behavior**

- **A client retires only a server that started inside a pane.** Today an
  unconfined client retires any server running under `no_new_privs`,
  together with every agent in it. A person who opened uze in a sandboxed
  terminal (Flatpak, a `NoNewPrivileges` unit) and later in an ordinary one
  loses their running agents. The server will record, when it starts,
  whether it was started from inside a pane, and only such a server is
  retired.
- **Delivery in a Git LFS repository says what it does not do.** UZE's own
  Git runs no hooks, so its delivery push does not upload LFS objects. A
  repository that uses LFS will be told so before the push. Delivery then
  stops with a next step: add the upload to the approved `workspace.gate`,
  or push from a terminal. A remote is never left pointing at objects it
  does not have.
- **The Windows installer's `latest` path is tested.** `install.ps1`
  resolves `latest` through a redirect, and no Pester case covers that path
  without `UZE_VERSION`.
- **Two open findings, tracked privately:** GHSA-w6jq-59jg-h4pg (High) and
  GHSA-97f7-95q3-wcjw (Medium). Each is fixed as its own `audit-<subject>`
  change. That change enters the repository with its fix, never before.

**Structure** (no behavior change; the audit changes' regression tests pin it)

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

## Capabilities

### New Capabilities
None.

### Modified Capabilities
- `terminal-runtime`: which server a client may retire.
- `worktree-policy`: delivery in a repository that uses Git LFS.

## Impact

- `crates/uze-terminal/src/runtime/{endpoint,server}.rs`, plus a field in the
  runtime's persisted state or handshake. That is a protocol or format
  change, so design.md covers the version.
- `crates/uze-workspace/src/landing/{deliver,publish}.rs`, and the TUI and
  CLI text for the stop (design-system skill).
- `tests/scripts/installer.Tests.ps1`.
- `crates/uze-platform/src/redact.rs`, and its callers in `uze-git`,
  `uze-core`, `uze-application` and `src/telemetry.rs`. A new leaf crate, or
  a recorded exception in `.agents/rules/platform.md` (design.md decides).
- `crates/uze-core/src/package/trust.rs` (`CodeIdentity`,
  `command_execution`, `package_executions`) and
  `crates/uze-integrations/src/shared/envelope.rs`.
- `crates/uze-application/src/application/lifecycle/{install,update}.rs`.
