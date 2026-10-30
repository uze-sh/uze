## Context

See proposal.md, Why. Two redactions already existed and disagreed:
`src/telemetry.rs` (argv words, *first* `@` of the authority, plus
provider-minted token prefixes) and `uze-core`'s `acquisition::git::redact`
(free text, *last* `@`). The first leaks the tail of a password that holds
an `@`; neither was reachable from `uze-application`'s spans or from
`uze-git`. The installers end on `uze --version` but only print it.

## Goals / Non-Goals

**Goals:**
- One redaction rule, applied where a user-supplied value enters a span,
  event or error, and once more on every text line written.
- An installer that installs a binary only after it proved to be the
  release asked for, before anything is placed.

**Non-Goals:**
- A freeze attack through the `latest` pointer itself (see Risks).
- Changing what the release workflow signs.

## Decisions

### ENV-1: `uze_platform::redact`, applied at entry and at the writer

- **Root cause fix.** `uze_platform::redact::redact(text)` is the one rule:
  per whitespace-separated word, the userinfo of any `scheme://` authority
  up to its *last* `@` becomes `<redacted>`, and a word that is a token a
  provider mints (`ghp_`, `github_pat_`, `xox…`) becomes `<redacted>`.
  `src/telemetry.rs` and `uze-core`'s acquisition code call it; their
  copies are gone.
- **Every entry point.** The span fields that record a user-supplied value
  redact it: `marketplace.add`/`register` (`source_str`),
  `install_plugin`/`install_plugin_resolving` (`spec`), `inspect_plugin`
  and `project.add` (`marketplace`), `uze-git`'s `git` span (`args`) and
  acquisition's `acquisition.git` span (`args`). `InvalidPluginSpec`, the
  one error that echoes a whole spec into the `err` field, redacts it.
- **A net.** The stderr and journal text layers write through a writer that
  redacts each formatted line, so an error message or a future field that
  forgets still cannot reach a log file.
- **Why `uze-platform`.** It is the one leaf every crate involved
  (`uze-git`, `uze-core`, `uze-application`, the binary) already depends
  on. A redaction is not an operating-system question, but it is
  domain-free text handling of the kind `fs_name` and `path` already hold
  there, and the alternatives are worse: `uze-core` is not reachable from
  `uze-git`, and a new crate for one function is a dependency edge per
  crate for nothing.
- **Rejected:** a `tracing` layer that rewrites fields for every other
  layer (tracing has no such hook: each layer reads the values itself);
  redacting only in `src/telemetry.rs` (misses OTLP, which formats fields
  itself).

### ENV-2: resolve, stage, prove, then rename

- `latest` is resolved first (`install.sh`: the redirect target of
  `<base>/latest`, as the self-updater reads it; `install.ps1`: the same
  through `HttpWebRequest`) to `v<version>`, validated as a release
  version, and every file is downloaded from `download/v<version>`. A
  pinned `UZE_VERSION` is validated the same way. Downloading from
  `latest/download` would let the pointer move between the two requests.
- The unpacked binary is staged beside its destination (`install.sh`) or
  in the scratch directory (`install.ps1`), run with `--version`, and must
  report the version word exactly, which is the check
  `src/self_update.rs`'s `replace` already makes. Only then is it renamed
  into place. Staging beside the destination rather than in `$TMPDIR`
  keeps it working where `/tmp` is mounted `noexec`, and the rename is
  atomic.
- The receipt records the verified version, not the binary's free text.
- **Signed version binding rejected.** Binding the version inside
  `SHASUMS256.txt` was considered and not done: the binary is pinned by a
  signed digest, so its own `--version` *is* signed material, and an old
  signed binary cannot report a new version. A signed version line would
  add a third consumer (the self-updater) to change and a rule for older
  releases, and buy no property the binary check does not already hold.

## Risks / Trade-offs

- [OTLP sees only the per-field rule] → every known field is covered and
  the writer net covers the text layers; a new span field echoing user
  input would reach OTLP until redacted. The spec requirement names it.
- [Freeze through `latest`] → whoever can publish a release can mark an
  older one as latest; it then installs consistently as that older
  version. Self-update still refuses a rollback; a fresh install has no
  baseline to compare with. Accepted: closing it needs signed release
  metadata with a timestamp, out of scope here.
- [Redacting `ssh://user@host`] → the user name of an SSH URL is shown as
  `<redacted>` in logs; nothing secret is lost from the trace.
- [The binary runs before acceptance] → it is already proven to be a
  signed release binary by its digest; running it is what the self-updater
  does too.
