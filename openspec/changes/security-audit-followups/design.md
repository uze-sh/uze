## Context

Both halves come from the polish report of `fix/security-audit`. The
behavior half covers correctness-2, quality-5 and tests-3, decisions the
operator deferred to the security review. The structure half covers
architecture-2, quality-9 and performance-2. The audit changes' regression
tests are the gate for every step.

## Goals / Non-Goals

**Goals:** each concept sits where the layering says it belongs. A value
cannot be compared before it is complete. Work is spent only when its
answer is used. A person never loses running agents or a working remote to
uze's own safeguards.

**Non-Goals:** changing what is redacted or what trust asks. Merging the two
global-option skippers in `uze-git` (architecture-3, a matter of taste) is
optional, and done only if it falls out of a touched file.

## Decisions

- **Retire by recorded origin, not by inference.** The server writes
  `started_inside_pane` when it starts, and reads it from its own process.
  The value is true when the process that started it is sealed (on Linux, it
  cannot reach a probe abstract socket bound by the server before it seals
  anything). A client retires a server only when that flag is set. The
  alternative, a `no_new_privs` check on the server's pid, is what exists
  today and is rejected: it confuses a sandboxed terminal with an agent.
  The flag travels in the handshake, so the protocol version moves. An
  older server, which has no flag, is treated as today.
- **LFS: stop with a next step, do not upload.** Whether UZE may run
  `git lfs push` itself is for GHSA-97f7-95q3-wcjw to settle. Until it
  is, delivery detects LFS (a `filter=lfs` attribute in
  the checkout, or `lfs.*` in its config) and refuses. The message gives
  two ways forward: `workspace.gate: [git lfs push --all origin]`, which
  is approved and runs in the slot, or pushing by hand. The alternative,
  pushing anyway and warning, is rejected, because it leaves the remote
  broken for every other clone.
- **Pester:** a fake release host that answers `/latest` with a redirect,
  one case with no `UZE_VERSION`, and a run in CI's `installer-windows` job.

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

- [A sandboxed person's own server, started before this change, has no
  flag] → treated as today until it restarts.
- [Refusing delivery in LFS repositories is a stop where there was none] →
  the message names the one-line gate that resolves it.
- [The refactor of `CodeIdentity` changes `IntegrationPort`] → every
  integration and the conformance lab compile against it. Run the
  integrations suite and `tests/architecture`.
