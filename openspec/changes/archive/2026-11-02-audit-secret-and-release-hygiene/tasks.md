## 1. ENV-1: credentials in traces

- [x] 1.1 Regression test: a capturing subscriber around `market add` and `install_plugin` with a credentialed URL finds no userinfo in any span or event (uze-application), and `uze-git`'s span records a credentialed argument redacted
- [x] 1.2 Fix: move redaction to `uze_platform::redact`, use it in telemetry and acquisition, at every span field echoing a user-supplied source, URL or spec, in `InvalidPluginSpec`, and as a redacting writer under the text layers
- [x] 1.3 Exploit re-run: `poc-token-leak.sh` in the audit sandbox against the fixed binary

## 2. ENV-2: installers bind the release to its version

- [x] 2.1 Regression test: `tests/scripts/installer-test.sh` serves a signed release whose binary reports another version (latest and pinned) and asserts refusal with nothing installed; latest resolves to a concrete version path
- [x] 2.2 Fix: `install.sh` and `install.ps1` resolve latest, download from the version path, stage and check `--version` before renaming, record the verified version; update `installer.Tests.ps1`
- [x] 2.3 Exploit re-run: the rollback fixture against the fixed `install.sh` in the audit sandbox

## 3. Close

- [x] 3.1 Gates: build, targeted tests, clippy, fmt, `openspec validate --strict`; write evidence.md
