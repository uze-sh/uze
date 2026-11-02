Audited commit: 98d1210f · Date: 2026-10-30 · Skill: `security-audit`

## ENV-1 - A marketplace URL's credentials reach logs, the journal and traces

- **Severity**: Low–Medium. The operator's own token lands in local logs,
  in the always-on TUI journal (which operators attach to bug reports) and
  in any OTLP collector.
- **Status**: CONFIRMED for the CLI with `UZE_LOG=info`. PLAUSIBLE for the
  TUI journal: the code was read but not run.
- **Threat actor**: none needed. The leak is self-inflicted and goes
  wherever the logs go.
- **Location**:
  - `crates/uze-application/src/application/marketplace.rs:304,317`
    (`#[tracing::instrument(fields(source_str = %source_str))]`).
  - Redaction exists only for the root `cli` span (`src/telemetry.rs:331`
    `redact`).
  - The TUI reaches the same span through `src/ui/worker.rs:514`.
- **Attack / evidence**: `UZE_LOG=info uze market add 'https://alice:ghp_SECRET…@127.0.0.1:9/org/market.git'` logs:
  ```
  cli{… argv=market add https://<redacted>@…}:marketplace.register{source_str=https://alice:ghp_SECRETTOKEN1234567890@…}: … refusing a URL carrying inline credentials; UZE never stores a secret
  ```
- **Root cause**: redaction is applied to one span, not at the point where
  any user-supplied source enters a span.

## ENV-2 - The installers do not bind the signed release to the version they install

- **Severity**: Low. Someone able to publish a GitHub release can serve an
  older, genuinely signed but vulnerable build under a new tag (rollback or
  freeze).
- **Status**: PLAUSIBLE. A signed fixture needs the private key.
- **Threat actor**: a network or release-channel attacker (5) holding a
  stolen token or a compromised workflow.
- **Location**: `install.sh:285-345` and `install.ps1`. They verify that
  `SHASUMS256.txt` is signed and that the archive matches it. `confirm` runs
  `uze --version` but never compares it to the requested or resolved tag.
  The archive names carry no version, so an old triple verifies under any
  tag.
- **Root cause**: the version is not part of what is signed or checked.
  `src/self_update.rs:907` (`replace`) already closes this for the
  self-updater.

## Defenses that held

- `UZE_BASE_URL` is honoured only under `debug_assertions`.
- The self-update is verified with `ssh-keygen -Y verify` against an
  embedded key. `curl` and `ssh-keygen` are resolved from system
  directories, `--proto =https` is enforced, and `CURL_*` is stripped.
- Self-update refuses a rollback, and versions are SemVer-validated before
  they become paths.
- Test-only environment variables sit behind `#[test]` or the testkit.
- `~/.uze` is 0700, and secrets are 0600 files written with `O_NOFOLLOW`
  and `create_new` plus rename.
- No predictable `/tmp` names are used.
- Maintenance deletes only links into `UZE_HOME`, or hashed paths.
- All third-party actions are pinned by SHA, untrusted input reaches `run:`
  only through `env:`, and the signing key sits in a protected environment.
- `cargo deny` advisories pass.
