## Context

See proposal.md. The findings come from the polish report of
`fix/security-audit`: correctness-2, quality-5 and tests-3. The operator
deferred these decisions to the security review, so each decision below is
that review's recommendation, adopted as the plan.

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

## Risks / Trade-offs

- [A sandboxed person's own server, started before this change, has no
  flag] → treated as today until it restarts.
- [Refusing delivery in LFS repositories is a stop where there was none] →
  the message names the one-line gate that resolves it.
