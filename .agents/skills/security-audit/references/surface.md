# UZE's attack surface

What an auditor attacks, one section per red-team agent. Each section names
the code, the claims to break, the hypotheses worth trying, and what earlier
audits proved. Update it at the end of every audit (`SKILL.md`, step 7).

## Threat actors

1. **Malicious marketplace or plugin author.** The operator adds their
   marketplace or installs their plugin.
2. **Malicious repository.** The operator clones it and opens it with uze:
   `agents.yaml`, `.agents/`, extension files, git config, symlinks, hooks.
3. **Prompt-injected agent inside uze.** It tries to escape its worktree,
   forge approvals or consent, reach the operator's panes or state, or
   affect other agents.
4. **Another local user or process.** It attacks through sockets, temp
   files, permissions and races.
5. **Network attacker.** It targets downloads, the update channel,
   telemetry and `fetch`.

## Surfaces

### acq: acquisition, marketplaces, Store

- **Code:** `crates/uze-core/src/package/` (acquisition, `git.rs`, store,
  naming, trust) and the `add`/`install`/`update`/`remove` verbs.
- **Claims to break:**
  - Git cannot be steered by a repository or by an option-shaped value.
  - Plugin bytes stay contained.
  - The lock pins bytes.
  - Trust is asked before anything executable is installed or changed.
- **Hypotheses:**
  - Git option and transport injection.
  - The cloned `.git/config` executing during uze's git calls.
  - `marketplace.json` traversal, and symlinks or hard links in the bytes.
  - Lock and digest TOCTOU.
  - Trust bypass through update, non-tty or `agents.yaml`.

### deliv: delivery into harnesses

- **Code:** `crates/uze-core/src/delivery/`, `capability/`,
  `crates/uze-integrations/src/*`, the hook compiler (`hooks/wrapper/`) and
  `src/shim.rs`.
- **Claims to break:**
  - What a harness executes is what trust showed.
  - Config writers cannot be injected.
  - Receipts never own a foreign file.
- **Hypotheses:**
  - Quoting in the compiled hook wrappers.
  - TOML/JSON/JS config injection.
  - Names with `../`.
  - Receipt swap and TOCTOU.
  - Vendor envelopes (`.claude-plugin/`, `.codex-plugin/`) carrying
    execution.

### script: the extension script engine

- **Code:** `crates/uze-script` (engine, reach, fetch, resident,
  primitives) and the extension runner and consent in `uze-application`.
  ADRs 041 and 048.
- **Claims to break:**
  - The engine is bounded.
  - The grants are the only reach.
  - Consent is per digest.
- **Hypotheses:**
  - Path escape in `read_checkout`.
  - SSRF and redirects in `fetch`.
  - Native string functions that allocate before Rhai's size check.
  - Nesting that persists across resident calls.
  - Consent widening on update.

### wks: workspace, approval, agents

- **Code:** `crates/uze-workspace` (checkout, landing, continuity,
  approval, task), the `uze agent` and `uze workspace` verbs, and
  `src/asker.rs`. ADRs 053, 055 and 058.
- **Claims to break:**
  - A project's commands run only once approved.
  - An agent cannot approve.
  - A repository's git config runs nothing.
  - Relaunch argv is uze's own.
- **Hypotheses:**
  - Approval hash framing.
  - Env-stamp removal.
  - A forged record.
  - Hooks in the common git dir.
  - Option-shaped values read back from git config or from harness session
    stores.

### term: terminal runtime and local IPC

- **Code:** `crates/uze-terminal`, and `crates/uze-platform` (endpoint,
  process, confine). ADRs 038 and 056.
- **Claims to break:**
  - A pane cannot attach, stop or type into another pane.
  - Another uid cannot connect.
  - The protocol is bounded.
- **Hypotheses:**
  - Escaping pane classification by daemonizing.
  - The key file being readable from a pane.
  - Flooding pane-allowed verbs.
  - Escape-sequence re-emission.
  - pid reuse.

### ext: declared extensions and parsers

- **Code:** `crates/uze-contract`, `crates/uze-extensions` (declared
  engine, bind), the KDL bounded decoder
  (`uze-core/src/capability/extension/`), `crates/uze-document`, and what
  `src/ui/` renders. ADRs 045, 050 and 060.
- **Claims to break:**
  - An extension describes and never stalls the draw.
  - The sanitizer neutralizes terminal control.
- **Hypotheses:**
  - Template fan-out.
  - KDL slashdash/escline nesting.
  - Bidi spoofing of consent dialogs.
  - Downgrade through the record ladder.

### env: distribution, environment, secrets

- **Code:** `install.sh`, `install.ps1`, `src/self_update.rs`,
  `crates/uze-core/src/machine/`, `UzeHome` permissions, telemetry and
  logging, and `.github/workflows/`.
- **Claims to break:**
  - Releases are authentic and current.
  - No test-only env var acts in release builds.
  - Secrets never reach logs.
- **Hypotheses:**
  - Installer rollback.
  - Tokens in span fields.
  - Predictable `/tmp` names.
  - `remove_dir_all` on a path a record controls.
  - CI `pull_request_target`.

## Findings index

| Audit | ID | Change or private advisory (open findings: id only) |
|---|---|---|
| 2026-10-30 | DELIV-1, ACQ-1 | `audit-consent-covers-execution` |
| 2026-10-30 | TERM-1, TERM-2, WKS-4, WKS-5 | `audit-agent-containment` |
| 2026-10-30 | WKS-1, WKS-2, WKS-3 | `audit-repository-git-execution` |
| 2026-10-30 | EXT-1, EXT-2, SCRIPT-1, SCRIPT-2 | `audit-bounded-extensions` |
| 2026-10-30 | ENV-1, ENV-2 | `audit-secret-and-release-hygiene` |
| 2026-10-30 | open | GHSA-97f7-95q3-wcjw |
| 2026-10-30 | open (TERM-1 partially closed) | GHSA-w6jq-59jg-h4pg |
| 2026-10-30 | follow-ups, not findings | `harden-audit-followups`, `refactor-audit-followups` |

## Defenses that held (2026-10-30)

Each `findings.md` lists them in full. The short version:

- **Acquisition git:** an option-shaped value is rejected at every entry.
  The environment is cleared. `core.hooksPath=/dev/null`,
  `fsmonitor=false` and `GIT_ALLOW_PROTOCOL` are set, and `.git` is
  removed before ingest.
- **Package containment:** symlinks, `..`, FIFOs, case collisions and the
  size, entry and depth budgets all hold.
- **Lock integrity:** the full-tree digest is checked on project
  installs.
- **Compiled hook wrappers:** they quote correctly and fail closed.
- **Names:** agent, skill and package ids are charset-validated.
- **Terminal output:** the sanitizer strips ESC, CSI, OSC, C1, bidi and
  zero-width characters. The pane reply sink drops OSC 52 and title
  reports.
- **Terminal runtime:** the socket directory is 0700 and the socket 0600.
  `SO_PEERCRED` is checked on both ends. Frames are bounded before
  allocation.
- **Approval digest:** the framing has no collisions, and the hasher and
  the runner read one parsed policy.
- **Self-update:** it is ssh-keygen signed, takes HTTPS only, refuses a
  rollback and ignores `UZE_BASE_URL` in release builds.
- **State and secrets:** `~/.uze` is 0700, and secrets are 0600 files
  written with `O_NOFOLLOW`.
