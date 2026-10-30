## ENV-1

- **Fix**: `5823ad7d`. One rule, `uze_platform::redact`, replaces both
  earlier copies; every span field that echoes a user-supplied source,
  spec or marketplace (`marketplace.add`, `marketplace.register`,
  `marketplace.install_plugin[_resolving]`, `marketplace.inspect_plugin`,
  `project.add`), both Git spans (`uze-git`'s `git`, acquisition's
  `acquisition.git`) and the `InvalidPluginSpec` message redact it, and the
  stderr and journal text layers redact every line they write.
- **Regression test**:
  - `crates/uze-application/src/application/tracing_tests.rs::no_span_or_event_carries_the_credentials_of_a_url_the_operator_typed`
    (`cargo test -p uze-application --lib no_span_or_event_carries`)
  - `crates/uze-git/src/lib.rs::tests::a_git_span_never_records_the_credentials_of_a_url`
    (`cargo test -p uze-git a_git_span_never`)
  - `src/telemetry.rs::tests::the_journal_never_keeps_the_credentials_of_a_url`
    (`cargo test -p uze --lib the_journal_never_keeps`), the net alone
  - `crates/uze-platform/src/redact.rs::tests::every_url_of_a_word_is_redacted`
    and `::a_password_carrying_a_slash_is_redacted` (`cargo test -p
    uze-platform redact`): a second URL in one word and an unescaped `/` in a
    password (found by the branch's review; the first version kept both).
  - `crates/uze-platform/src/redact.rs::tests` (`cargo test -p uze-platform redact`)

  The first two were run before the fix and failed, recording
  `source_str=https://alice:ghp_SECRETTOKEN…@127.0.0.1:9/…`,
  `spec=flow@https://alice:ghp_SECRETTOKEN…`, an `InvalidPluginSpec` error
  echoing the token (and a slug of it), and
  `args=… rev-parse --verify https://alice:s3cretpassw0rd@…`. The journal
  test fails without `Redacting` by construction: its span field is a
  literal no entry point redacts.
- **Exploit re-run**: `poc-token-leak.sh` in sandbox `env1` against the
  fixed binary: no line contains `SECRET` or `s3cret`. A recount at
  `UZE_LOG=info` and `UZE_LOG=trace`, plus `uze install flow@<credentialed URL>`:
  ```
  SECRET occurrences (info):
  0
  marketplace.register{source_str=https://<redacted>@127.0.0.1:9/org/market.git}
  s3cret occurrences (uze_git=debug,trace):
  0
  spec in install (trace):
  0
  ```
- **Residual**: the OTLP exporter reads span fields itself, so it is
  covered by the per-field rule and not by the writer net; a future span
  field that echoes user input unredacted would reach a collector (the
  `observability` requirement names the rule). An SSH URL's user name is
  shown as `<redacted>` in traces (decisions.md D3).

## ENV-2

- **Fix**: `6f2652a2`. `install.sh` and `install.ps1` resolve `latest` to
  `v<version>` before downloading, validate a pinned `UZE_VERSION`,
  download from `download/v<version>`, and run the unpacked binary before
  placing it: unless its `--version` reports that version they refuse,
  name both versions, and leave any installed binary and the receipt
  untouched. The receipt records the verified version.
- **Regression test**: `tests/scripts/installer-test.sh` (`sh
  tests/scripts/installer-test.sh`, 47 checks): a signed `9.9.7` build
  served under the latest `v9.9.9` and under a pinned `v9.9.8` is refused
  with both versions named, the previously installed binary is left as it
  was and no receipt is written; an unresolvable `latest` and a
  path-shaped `UZE_VERSION` are refused. Against the audited `install.sh`
  (`98d1210f`) the pinned case installs the old build and exits 0, as
  observed with the same fixture: `uze 9.9.7` installed and the receipt
  says `"version": "9.9.7"`. `tests/scripts/installer.Tests.ps1` gains the
  same cases for `install.ps1`; PowerShell is not available here, so
  `install.ps1` and its Pester tests were reviewed, not run — CI's
  `installer-windows` job runs them.
- **Exploit re-run**: `poc-rollback.sh` in sandbox `env2`: a release host
  whose latest is `v2.0.0`, serving a build signed by the release key that
  reports `1.0.0`:
  ```
  --- audited installer, latest = v2.0.0 serving a signed 1.0.0 build
  ✓ Checksum verified
  ✓ Installed /home/attacker/work/run/bin-audited/uze
  ✓ uze 1.0.0
  exit-status=0 installed: uze 1.0.0
  --- fixed installer, latest = v2.0.0 serving a signed 1.0.0 build
  ✓ Latest release is v2.0.0
  ✓ Checksum verified
  × Checking the binary is v2.0.0
    the release's binary reports 1.0.0, not 2.0.0
  exit-status=1 installed: nothing
  ```
- **Residual**: whoever can publish a release can still point `latest` at
  an older, genuinely signed tag; a fresh install then installs that older
  release consistently (the updater refuses a rollback, a fresh install has
  no baseline). Closing it needs signed, timestamped release metadata and
  is out of scope (design.md, Risks). `install.ps1` is unrun here.

## Gate

- `cargo build --locked --bin uze`: ok.
- `cargo test -p uze-platform -p uze-git -p uze-core -p uze-application`:
  all pass (96, 28, 740 + integration, 415 + integration).
- `cargo test -p uze --lib telemetry`: 15 pass.
- `cargo test --test architecture`, `--test project`, `--test packages`:
  all pass.
- `sh tests/scripts/installer-test.sh`: 47 passed, 0 failed.
- `cargo clippy --all-targets -- -D warnings`, and the same for the four
  touched crates with `-p`: clean.
- `cargo fmt --check`: clean.
- `openspec validate audit-secret-and-release-hygiene --strict`: valid
  (`--all --strict` fails only on other audit changes still in flight).
- Not run here: `cargo test --workspace` (left to the parent after the
  join), `shellcheck install.sh` (not installed), the Pester suite.
