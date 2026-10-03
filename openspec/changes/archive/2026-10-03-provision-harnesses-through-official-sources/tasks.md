## Decision and research

- [x] Record ADR-010 for official, integration-owned harness provisioning
      (`docs/adr/010-provision-supported-harnesses-through-official-routes.md`).
- [x] Record the official install/update/verification routes and platform
      restrictions for Claude Code, Codex, OpenCode, and Antigravity CLI
      (which replaced Gemini CLI as the Google-family harness, ADR-027);
      include source links and do not infer missing Windows routes.

## Provisioning boundary

- [x] Add typed, integration-owned provision/detect/verify results and an
      injectable process runner; keep vendor commands out of Core and the
      Application.
- [x] Add atomic, secret-free provisioning state under `$UZE_HOME/state`,
      separate from `attachments.json` and integration setup state.
- [x] Evolve application setup orchestration: provision selected harness,
      verify, prepare, republish, then deliver already stored packages to only
      that integration.
- [x] Preserve plugin install (`uze install <name>@<marketplace>`) as an
      offline/no-provision operation that only prepares detected harnesses.

## Peer implementations

- [x] Implement and contract-test supported Unix/WSL routes for Claude Code,
      Codex, OpenCode, and Antigravity CLI. Install routes:
      `tests/integrations/provisioning.rs`; update routes: the setup matrix in
      `tests/cli/machine.rs`.
- [x] Implement documented Windows/macOS routes only where the vendor exposes
      a safe official automation path; return actionable unsupported results
      otherwise. macOS takes the vendors' Unix installers; Windows has no
      route exercised by a Windows runner, so every integration answers
      Blocked naming the vendor's installation page (`research.md`).
- [x] Capture version and provision provenance without credentials or complete
      command output. The record keeps action, status, method, platform,
      version and time; never a command, URL or output. Installer output
      goes to `$UZE_HOME/cache/logs/setup-<harness>.log` instead.

## Product presentation and safety

- [x] Surface install/update/verify/prepare outcomes in CLI and the existing
      TUI using Application read models only.
- [x] Ensure provision failure never records a prepared integration or
      attachment receipt, and does not remove packages or external artifacts.
- [x] Do not implement harness removal; document the future ownership gate
      (`design.md`, "Harness removal"). Removal is listed under "Not in this
      change".

## Verification

- [x] Test missing → official install → verify → prepare in isolated HOME and
      UZE_HOME with a fake process runner.
- [x] Test present → official update → verify → prepare (the record keeps the
      `UPDATE` action) and update failure → blocked/no preparation.
- [x] Test package added before harness provision is delivered by later setup
      without duplicate native/capability attachments.
- [x] Test plugin install never invokes a provision command, including when
      all harnesses are absent.
- [x] Add a registry-complete CLI conformance matrix for `uze setup` that
      covers every registered harness, its official update route reaching the
      vendor binary on `PATH`, and default shim creation, without network
      access.
- [x] Cover the OpenCode legacy `opencode2` route: it must use the official
      installer instead of passing the stable-only `upgrade` subcommand.
- [x] Verify OpenCode at its documented install location when the
      installer's own `PATH` edit is not live in this process: the official
      installer writes `~/.opencode/bin/opencode` and edits the shell rc, so
      `uze setup opencode` reported "installer finished but `opencode` could
      not be verified" (migration report, 2026-09-28). Done: the installer's
      destinations are searched in its own order when `PATH` has no
      `opencode`. The setup report (CLI and TUI) names the location found and
      that a new shell is needed for `PATH`.
- [x] Test the workspace's setup of an installed harness: verified from the
      executable already there, never updated, shim placed
      (`the_workspace_sets_up_what_is_installed_without_updating_it`).
- [x] Run cargo test, cargo clippy -- -D warnings, cargo fmt --check,
      openspec validate --all --strict, and git diff --check.
