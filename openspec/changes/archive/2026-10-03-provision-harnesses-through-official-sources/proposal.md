## Why

`uze setup` currently prepares UZE-owned attachment prerequisites only after a
harness is already installed. That makes a fresh environment require users to
learn each vendor's installer before UZE can compose their agent environment.

UZE should make `setup` the explicit, reproducible bootstrap for supported CLI
harnesses while keeping plugin distribution independent from executable
provisioning.

## What Changes

- Make `uze setup [harness]` ensure the selected harness is installed at its
  official latest-stable channel, update an existing supported installation,
  verify the executable, then run the existing UZE preparation and package
  exposure steps.
- Introduce an integration-owned provisioning contract and secret-free
  provisioning record. The Application orchestrates it but never embeds
  vendor installer URLs, command syntax, or platform-specific schemas.
- Retain `uze install <name>@<marketplace>` as a non-provisioning path: it
  prepares and attaches to harnesses already detected, but never downloads or
  upgrades a harness implicitly.
- Make setup the one writer of runtime shims. Opening the workspace runs the
  same setup for harnesses it finds installed and not set up (from the
  executable already there, never updated), and asks which to provision only
  on a machine with no harness.
- Keep vendor installer output off UZE's report: it goes to a per-harness
  log under `$UZE_HOME/cache/logs/`, and the installer runs without a
  controlling terminal.
- Start with Claude Code, Codex, OpenCode, and Antigravity CLI (which
  replaced Gemini CLI as the Google-family harness, ADR-027) using documented
  official installation/update routes. An unavailable official route on a
  platform is reported clearly rather than replaced with an unofficial one.
- Record only UZE-initiated provisioning provenance for future safe harness
  removal. Harness removal is not introduced by this change.

## Capabilities

### New Capabilities

- `harness-provisioning`: Explicit setup can install, update, verify, and
  prepare supported CLI harnesses through their official vendor routes.

### Modified Capabilities

- None.

## Impact

- `UzeApplication::setup`, CLI/TUI setup presentation, `IntegrationPort` and
  peer integrations gain a narrow provisioning path.
- `$UZE_HOME/state` gains secret-free provisioning provenance distinct from
  attachment ownership.
- Tests need a fake official-command runner and isolated process contracts;
  no real vendor installer runs in ordinary `cargo test`.
- ADR-010 (`docs/adr/010-provision-supported-harnesses-through-official-routes.md`)
  and this OpenSpec change describe UZE as a compatibility/distribution
  layer with an explicit harness bootstrap layer, not a general SDK/version
  manager.

## Not in this change

- Reporting, on a plugin install, that delivery to an absent harness is
  pending: today absent harnesses are skipped without a word. Follow-up.
- Harness removal and its ownership gate (a removal that preserves an
  executable UZE has no install record for); `design.md` sketches the gate.
  Follow-up.
- Showing the provision path in the architecture diagrams under
  `docs/architecture/` and describing `uze setup` in the README. Follow-up.
