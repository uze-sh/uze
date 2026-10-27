## Context

`add-extension-platform/design.md`: D18 (preferences, secrets), D19
(`fetch` over `curl`, no TLS crate), *Product decisions owed* (`edit:checkout`
by gesture, only if needed), R-S8 (secrets never reach an agent).

## Goals / Non-Goals

**Goals:** `ci-status` shows its runs with a token the operator typed once,
reaching only the forge's API host.

**Non-Goals:** a TLS stack, a proxy configuration of UZE's own, writing the
checkout, a pipe between extensions.

## Decisions

### G1. `fetch` is a program run, with `curl` as the program

`fetch` builds `curl` argv (never a shell) and runs it through
`machine::extension_program` with the run's bounds: confined where the
machine confines, with network allowed and the filesystem read-only. The
host checks the URL's scheme and host against `net:` before and refuses a
redirect elsewhere (`--max-redirs 0` and the client follows only what it
checked).

### G2. A secret is used, never read

A script names a secret only as `fetch(url, #{ secret: "token", header:
"Authorization", scheme: "Bearer" })`; the host reads the keychain and puts
it into `curl`'s config on stdin, so it is in no argv, no environment and no
answer. An answer that echoes it is scrubbed before it is drawn.

### G3. What is deferred is said

`edit:checkout`, digest pins as a preference, pipes and extension CLI
commands are recorded as not built, each with the gallery entry that would
bring it.

## Risks / Trade-offs

- [A forge echoing the token] → scrubbed from every answer by value.
- [No keychain on a headless Linux] → the `0600` file, said on the screen.
