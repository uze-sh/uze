## Design

This change adds an explicit **harness bootstrap layer** above the existing
package delivery path. It does not turn UZE into a universal runtime/version
manager: it covers only registered peer CLI integrations and invokes only
their official vendor routes.

```text
CLI / TUI
    │
UzeApplication::setup
    │
Integration-owned provision → detect/verify → existing prepare
    │                                               │
    └────────── secret-free provisioning state      └─ package exposure + receipts
```

`IntegrationPort` remains the vendor boundary. It receives additive
provisioning operations (or delegates them to a small integration-owned
provisioner) that produce typed facts: absent/present, action attempted,
verified version, supported/blocked, and an actionable reason. The Application
coordinates selection, calls the existing `install` preparation only after a
verified executable, and replays normal package delivery only for the selected
integration. Core types never contain a vendor URL, shell snippet, or config
schema.

`setup` chooses the official latest-stable route. On Linux, macOS and WSL,
Claude Code, Codex, OpenCode, and Antigravity CLI (the Google-family harness
since ADR-027, replacing Gemini CLI) have documented first-party install
scripts, which UZE runs. For an existing executable, the integration uses its
own documented update command where that is reliable; if UZE cannot safely
establish the method/platform, it reports a blocked result rather than
guessing.

Windows is not automated. The vendors document PowerShell or package-manager
routes there, but none of those command contracts has been exercised by a
Windows runner, so every integration answers Blocked on Windows with the
vendor's own installation page in the reason, and runs nothing. Adding a
Windows route means adding its command to the owning integration together
with a Windows CI leg that proves it, not widening the Unix gate.

When an installer puts the executable somewhere the running shell's `PATH`
does not reach yet (OpenCode's `~/.opencode/bin`, Antigravity's
`~/.local/bin`; both installers only edit rc files), setup verifies it at
that documented location and the result carries the location, so the report
says where it is and that a new shell will see it.

The command runner is injectable. Production invokes a process without a
shell unless an official installer necessarily requires one; tests assert the
exact approved command specification through a fake runner. Installation and
update commands inherit stdout/stderr so an explicit `uze setup` shows vendor
progress in real time; quiet verification probes do not. Neither output mode
persists command output. Timeouts, version verification, and nonzero exits
become structured results. Normal tests never run a network installer.

The CLI conformance suite owns a registry-complete setup matrix. It runs each
registered `uze setup <harness>` against deterministic fake vendor binaries,
then compares the exercised ids to `IntegrationRegistry`; a newly registered
harness cannot silently miss setup coverage. A dedicated legacy-binary case
captures route differences that cannot be expressed as a shared update verb,
such as OpenCode's `opencode2` binary. The real Harness Conformance Lab stays
focused on package behavior with network disabled; it does not execute vendor
installers.

`$UZE_HOME/state/provisioning.json` records only UZE-initiated action,
integration id, platform/method, executable identity when observed, version,
and time/outcome. It is distinct from `attachments.json`: a provision record
does not grant detach/removal authority. This change adds no public harness
uninstall command. That needs a future explicit ownership and vendor-uninstall
decision.

### Harness removal (not implemented)

A future `uze setup --remove <harness>` (or whatever verb it becomes) passes
an ownership gate before it touches an executable, and the gate is stricter
than the one attachments pass:

1. **Positive evidence of origin.** A `provisioning.json` record whose action
   is `INSTALL` and status `VERIFIED` is necessary, never sufficient: an
   `UPDATE` record, a failed attempt, or no record at all means the
   executable was there before UZE, and it is preserved.
2. **Identity of what is there now.** The record today keeps no executable
   identity, only a version, because an update does not prove the file is
   still the one UZE installed. Removal needs one: the resolved path and a
   content fingerprint taken at install time and compared at removal time,
   the way receipts are inspected before detach. Adding it is a record shape
   change through `uze_document`'s ladder, not an optional field.
3. **The vendor's own uninstall route only.** Like installation, removal is
   integration-owned and runs the documented uninstall; a harness without
   one is reported, never deleted by hand.
4. **Harness state is not UZE's.** Sessions, credentials and vendor config a
   harness wrote are never removed with it; UZE's own attachments are
   detached through the existing receipt flow first.

`uze add` retains the DX fix already on `main`: it prepares detected
integrations so immediate attachment works, but it never invokes provision or
network operations. This keeps scripted plugin installation predictable.

### Architectural decision

An ADR is required: vendor executable provisioning is an enduring new product
responsibility, but it must remain integration-owned and separate from both
package provenance and managed attachment ownership. The Mermaid diagrams
under `docs/architecture/` must show the Application coordinating the
optional official vendor provision path.
