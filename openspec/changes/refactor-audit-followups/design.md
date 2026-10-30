## Context

See proposal.md. All three come from the review of `fix/security-audit`
(findings architecture-2, quality-9 and performance-2 of its polish report).
The behavior they wrap is pinned by the audit changes' regression tests.
Those tests are the gate for every step here.

## Goals / Non-Goals

**Goals:** each concept sits where the layering says it belongs. A value
cannot be compared before it is complete. Work is spent only when its
answer is used.

**Non-Goals:** changing what is redacted or what trust asks. Merging the two
global-option skippers in `uze-git` (architecture-3, a matter of taste) is
optional, and done only if it falls out of a touched file.

## Decisions

- **redact.** The first choice is a new leaf crate, `uze-text`: domain-free,
  depending on nothing in uze, and allowed by `tests/architecture/` as a
  leaf. The alternative is to keep it in `uze-platform` and write the
  exception into `platform.md`. That is rejected unless the crate turns out
  to hold redact alone with no second tenant in sight. A one-function crate
  is overhead the workspace already avoids.
- **CodeIdentity.** `command_execution` takes the package digest as an
  argument, so it can only build a complete identity. The integrations
  receive the digest through `IntegrationPort::package_executions(root,
  digest)`. The alternative, `digest: Option<String>`, is rejected: it moves
  the "not yet" into every comparison instead of removing it. If
  `CodeIdentity` is persisted anywhere, that is a format change; check
  before changing the field.
- **Lazy digest.** `authorize` takes a closure (or a once-cell) for the
  digest and runs it only after `crosses_trust_boundary` and a non-empty
  executable set. The before/after time of `uze update` on a local plugin
  goes in the commit body.

## Risks / Trade-offs

- [The refactor of `CodeIdentity` changes `IntegrationPort`] → every
  integration and the conformance lab compile against it. Run the
  integrations suite and `tests/architecture`.
