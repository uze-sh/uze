## Why

- **ENV-1** breaks *a credential the operator typed never reaches a trace*:
  the userinfo of a marketplace URL is redacted on the root `cli` span only,
  so `marketplace.add`/`register`, the install spans and Git's own spans
  carry it into stderr logs, the always-on TUI journal and any OTLP
  collector.
- **ENV-2** breaks *an installer installs the release it was asked for*:
  `install.sh` and `install.ps1` verify that the checksums are signed and
  the archive matches, but never that what they install is the requested or
  resolved version, so a signed older build served under a newer tag
  installs silently.

## What Changes

- One redaction rule, in a leaf crate every layer reaches, used by the root
  CLI span, every span field and error message that echoes a user-supplied
  source, URL or spec, and both Git spans (`uze-git` and acquisition).
- The text layers (stderr and the journal) redact every line they write, as
  a net under the per-field rule.
- `install.sh` and `install.ps1` resolve `latest` to a concrete release
  before downloading, download from that release's own path, and refuse to
  install a binary that does not report that version; nothing is placed on
  a refusal, and the receipt records the verified version.

## Capabilities

### New Capabilities

### Modified Capabilities
- `observability`: a trace, event or log line never carries the userinfo
  of a URL the operator supplied.
- `platform-support`: the installer installs only the release it resolved
  or was pinned to, proven by the binary's own version, and fails closed.

## Impact

- `crates/uze-platform` (new `redact` module), `crates/uze-core`
  (acquisition redaction, plugin-spec errors), `crates/uze-git` (span
  args), `crates/uze-application` (marketplace and project span fields),
  `src/telemetry.rs` (argv and text-layer redaction).
- `install.sh`, `install.ps1`, `tests/scripts/installer-test.sh`,
  `tests/scripts/installer.Tests.ps1`.
- No release-workflow change, no new dependency.
