## Why

A Windows user cannot run uze today. The installation page says "Coming soon,
use WSL", and that is accurate. ADR-034 made Windows a non-goal so that *a*
distribution could ship, and `support-macos` left it that way. Two platforms
are now proven end to end. The next release should make Windows the third:
installed with one PowerShell line, used the way it is used on Linux and
macOS, workspace included, with nothing from the POSIX world underneath.

Unlike macOS, the gap starts at the compiler:

- **The terminal runtime is Unix code.** ADR-038 promised a port to Windows;
  no such port exists in the code.
- **The `cfg(not(unix))` branches are placeholders, some of them unsafe:**
  - a lock that locks nothing;
  - a liveness check that always answers "alive";
  - a Git timeout that disappears;
  - a symlink helper that always fails.
- **Home resolution requires `HOME`**, which stock Windows does not set.
- **Delivered names such as `flow:review` contain `:`.** On NTFS that
  silently writes an alternate data stream instead of a file.

The `platform-support` spec already sets the bar: a platform is supported
only where its own hardware builds, lints, tests and performs the product
journeys. Windows clears that bar, or it is not offered.

## What Changes

- **Native, with Git as the one explicit dependency.**
  - Nothing uze does by itself goes through a shell: the CLI, the workspace,
    upgrades, shims and the hook wrapper all run directly.
  - `git.exe`, from any distribution, is the only prerequisite.
  - No Git Bash, `sh`, WSL or POSIX tool is on any path uze runs. CI proves
    it by running the Windows suite with none of them reachable.
- **A `uze-platform` leaf crate.** Process trees (Job Objects on Windows,
  process groups on Unix) and kernel facts (peer, image, cwd, environment,
  liveness) live under `uze-core`, `uze-git` and `uze-terminal`. The
  dependency graph forbids them anywhere else.
- **The terminal runtime on Windows.**
  - First, the transport port ADR-038 promised is extracted.
  - Behind it:
    - an overlapped named pipe private to its user;
    - ConPTY;
    - a Job Object per pane, which the pane joins itself;
    - a server that survives the console that started it;
    - a stop event whose shape never changes, so any build can retire any
      other.
- **The machine layer speaks Windows:**
  - home from the platform;
  - `PATHEXT` lookup;
  - path comparison that ignores case and `\\?\`;
  - byte-range locks that keep the holder's pid readable;
  - a retry for sharing violations;
  - Git acquisition that pins line endings, so a package digests identically
    everywhere;
  - an embedded manifest for long paths and UTF-8.
- **A command line belongs to a platform.**
  - Setup steps, gates and hook handlers take a `posix` and/or a `windows`
    spelling. A plain string is `posix`.
  - On Windows they run in Windows PowerShell 5.1, with exit status
    preserved.
  - A missing spelling is never run in the wrong shell:
    - a `deny`/`ask` hook group without one **fails the install**;
    - an observer is reported Unsupported;
    - a setup step leaves its checkout not ready;
    - a gate is flagged at placement and blocks delivery.
  - The plugin scaffold writes `guard` and `guard.ps1`.
- **Hooks reach Windows.**
  - A generated PowerShell wrapper carries the same contract as the `sh`
    one.
  - Each harness gets the entry form it is measured to execute: Claude's
    exec form, Codex's `commandWindows`, Antigravity's `cmd /C`, and the
    OpenCode bridge.
  - Each is recorded per harness version.
  - Machines whose policy forbids scripts report hooks Unsupported instead of
    failing at call time.
- **Names Windows can hold.** On Windows, loose skills and agents are
  written as `<plugin>-<capability>`, with collisions refused. MCP launchers
  that are `.cmd` files start correctly.
- **Shims and upgrade.**
  - A shim is a copy of `uze.exe`.
  - The binary and the shims are replaced by renaming the running image
    aside, never by overwriting it.
  - The asset is a `.zip`, extracted with the system `tar.exe`.
- **Provisioning on Windows.**
  - Claude, Codex and Antigravity are installed through their official
    PowerShell installers.
  - OpenCode has none. An installed OpenCode is upgraded; a missing one is
    Blocked, naming scoop, choco and npm.
  - Claude's sandbox is Unsupported on native Windows.
- **The suite, the journeys and CI on Windows.**
  - `cargo test --workspace` runs on `windows-2025` and `windows-11-arm`
    under PowerShell.
  - The journey runner gains a ConPTY + VT-screen backend.
  - An installer job runs Pester on 5.1 and 7.
- **Release and install.**
  - `uze-{x86_64,aarch64}-windows.zip` (crt-static), in SHASUMS, provenance
    and an all-target SBOM.
  - `install.ps1` via `irm https://uze.sh/i.ps1 | iex`, with an uninstall.
  - A generated Scoop manifest.
  - The Windows rows stay behind a switch that is flipped, together with the
    installer, only after the proof passes.

## What this does not change

- **Signing.** The binaries ship unsigned. SmartScreen (browser downloads)
  and Smart App Control (on by default on clean Windows 11 installs) may
  block them. The installer and `uze doctor` detect and say so, and so do the
  docs and the release notes.
- **The Windows floor.** Windows 10 22H2 (build 19045) and Windows 11. Older
  builds are refused by name.
- **The Conformance Lab.** It stays on Linux and Docker.
- **WSL.** It keeps working as Linux.
- **On-disk names on Linux and macOS.** Only Windows gets the
  `<plugin>-<capability>` form.

## Sequencing

This is one change, applied in the order its task groups give. Every group
merges with Linux and macOS green, behind the installer's refusal.

`native-first-hooks` has open tasks and owns the env-plus-exit hook contract
this change extends. It should be archived first, so this change's
portable-hooks delta and its ADR-040 addendum build on settled text.

## Capabilities

### New Capabilities

<!-- none: Windows is a platform, not a capability; every behaviour has a home -->

### Modified Capabilities

- `platform-support`:
  - Windows 10 22H2+ and Windows 11 are supported under the existing proof.
  - Guarantees hold or fail closed.
  - Paths compare as the platform compares them.
  - `install.ps1` resolves, verifies, fails closed and uninstalls.
- `terminal-runtime`:
  - Windows behind the transport and PTY ports.
  - The endpoint is a named pipe.
  - Only its own user is admitted, on every platform.
  - The server outlives its console.
  - A pane ends with its tree.
- `binary-updates`: a replacement on Windows renames the running image aside,
  with an undo. Unix keeps a single rename.
- `harness-provisioning`:
  - the vendors' Windows routes;
  - OpenCode Blocked when it is not installed;
  - shims as copies that are swapped and never overwritten;
  - shims take effect inside the workspace.
- `portable-hooks`:
  - `posix`/`windows` command spellings;
  - a deny or ask group without one fails the install;
  - the PowerShell wrapper and its exit-status contract;
  - policy-restricted machines report Unsupported.
- `worktree-policy`: setup steps and gates spelled per platform, reported
  early and never run in the wrong shell.
- `package-delivery-fidelity`: names valid on the host filesystem, and
  identical bytes and digests across platforms.
- `mcp-as-second-capability`: an MCP command that resolves to a `.cmd`
  launcher starts on Windows.

## Impact

- **Crates:**
  - new `uze-platform`;
  - `uze-terminal`: transport port, Windows backend, stop event, pane jobs;
  - `uze-core`: home, paths, locks, persistence, acquisition, `Command`,
    trust, authoring;
  - `uze-git`: deadline, lock;
  - `uze-integrations`: wrapper, per-harness entries, names, MCP,
    provisioning;
  - `uze-application`: shim placement, path re-export;
  - `uze-workspace`: setup/gate spellings, readiness, prompt history;
  - `uze-testkit`: Rust fakes instead of `#!/bin/sh`;
  - the binary crate: shim, self-update, main, UI keys, opener, colour.
- **Dependencies:** `windows-sys`, from Microsoft and already in the lock,
  becomes a direct `cfg(windows)` dependency. No other crate.
- **Repository:** a root `.gitattributes`; a `build.rs` manifest embed;
  `crate-layering.mmd`; AGENTS.md; the layering tests.
- **CI and release:** `ci.yml`, `journeys.yml`, the journeys action,
  `release.yml`, `cliff.release.toml`, `.cargo/config.toml`.
- **Tooling:** `journey.py` Windows backend (CI-only `pywinpty`, `pyte`,
  `psutil`).
- **Distribution:** `install.ps1`, a Pester suite, the `/i.ps1` rewrite, a
  Scoop manifest.
- **Docs:**
  - the installation page's Windows tab, README, roadmap, development,
    versioning;
  - invariants;
  - addenda to ADR-010, ADR-014, ADR-034, ADR-038 and ADR-040.
