## D1 — Keep the one redaction rule in `uze-platform`

- status:   pending
- task:     1.2
- commit:   5823ad7d
- trigger:  the rule had to be reachable from `uze-git`, `uze-core`, `uze-application` and the binary, and `uze-platform`'s charter is "what UZE asks the operating system"
- decided:  `uze_platform::redact`, a domain-free text module beside `fs_name` and `path`; `uze-core`'s `acquisition::git::redact` became a re-export and `src/telemetry.rs` lost its copy
- rejected: a new leaf crate (a dependency edge in four crates for one function); `uze-core` (not reachable from `uze-git`); two copies (they had already drifted: the telemetry one cut at the first `@` and leaked a password's tail)
- blast:    moving it later is a path change at five call sites

## D2 — Redact every text-layer line as a net under the per-field rule

- status:   pending
- task:     1.2
- commit:   5823ad7d
- trigger:  `#[instrument(err)]` records an error's `Display`, which can echo user input that no field annotation reaches
- decided:  the stderr and journal `fmt` layers write through `Redacting`, which applies the rule to each formatted line; the error that echoes a whole spec (`InvalidPluginSpec`) redacts at its source too
- rejected: redacting every `UzeError` variant's `Display` (wide, and the CLI shows the operator their own input on purpose); relying on fields alone (one forgotten field is a leak)
- blast:    a line-level pass costs a scan per log line, only when a text layer is on; OTLP is not covered by the net (design.md, Risks)

## D3 — Redact the userinfo of any scheme, SSH included

- status:   pending
- task:     1.2
- commit:   5823ad7d
- trigger:  acquisition only refuses credentials over HTTP(S); `ssh://user:pass@host` passes and reaches spans
- decided:  the rule redacts the userinfo of every `scheme://` authority, so an SSH user name shows as `<redacted>` in traces
- rejected: per-scheme redaction (a password in an SSH URL is still a password, and a user name is not worth a rule)
- blast:    traces show less of an SSH remote; nothing functional

## D4 — Bind the version through the binary, not through a signed version line

- status:   pending
- task:     2.2
- commit:   6f2652a2
- trigger:  the brief allowed binding the version inside the signed `SHASUMS256.txt` if cheap
- decided:  no release-workflow change; the installers require the binary's own `--version` to report the resolved or pinned release before placing it, as `self_update::replace` does
- rejected: a signed `VERSION` entry in `SHASUMS256.txt` — the binary is already pinned by a signed digest, so its version is signed material; the line would add a third consumer (the updater), a rule for older releases, and no property the binary check lacks (neither defends a `latest` pointer moved to an older tag)
- blast:    none; adding the signed line later is additive

## D5 — Stage the binary beside its destination and rename it into place

- status:   pending
- task:     2.2
- commit:   6f2652a2
- trigger:  the version must be asked before anything is placed, and `$TMPDIR` may be mounted `noexec`
- decided:  `install.sh` installs to `<bin_dir>/.uze-install.<pid>`, runs it, and `mv`s it over `uze` only on a match; the trap removes a staged file on any exit. `install.ps1` runs it from its scratch directory (no `noexec` on Windows) before copying it
- rejected: running from `$TMPDIR` (fails on hardened hosts); placing first and checking after (a refusal would leave the wrong binary installed)
- blast:    a crash between stage and rename leaves a hidden `.uze-install.<pid>` in the bin directory

## D6 — Validate a pinned `UZE_VERSION`, and refuse to guess `latest` where nothing redirects

- status:   pending
- task:     2.2
- commit:   6f2652a2
- trigger:  the version is now a path segment chosen before download, and `install.ps1` is tested against a `file:` release that has no redirect to read
- decided:  both installers accept only `[0-9][0-9A-Za-z.-]*` without `..`; `install.ps1` refuses a non-HTTP base without `UZE_VERSION` ("set UZE_VERSION") and `install.sh` refuses a `latest` that does not redirect to `…/tag/v<version>`
- rejected: falling back to `latest/download` when resolution fails (the fallback is the unbound path this change removes)
- blast:    a mirror must answer `<base>/latest` with a redirect to a tag, or be used with `UZE_VERSION`
