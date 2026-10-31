## Why

An extension's program runs confined on Linux and macOS and as the operator
on Windows, where `uze-platform` confines nothing
(`extension-programs-and-confinement` P7 deferred it). The rights a machine
reports are named after Linux's mechanisms (`syscall_filter`, `scoping`),
so even a platform that refuses the same reaches by other means reads as
weaker than it is. Experiments in Windows Sandbox showed that an
AppContainer per run, in a Job Object, refuses what the Linux confinement
refuses. That makes parity reachable now.

## What Changes

- Windows confines a program. Each run gets its own AppContainer profile,
  created before the run and deleted after it. The paths it may reach are
  granted through capability SIDs, one per path and right. The run sits in
  a Job Object that bounds its memory and processor time and ends the
  whole tree. Without network a program reaches no host, loopback
  included. With network it reaches the Internet and still no local
  service, no named pipe and no inbound connection.
- A grant on Windows is an inheritable access-control entry on the
  granted directory. It names a capability that only a process UZE starts
  can hold, and it stays in place, so later runs pay nothing.
- `Availability` reports **outcomes**, not mechanisms. Each field says
  what a confined program cannot do (reach the network or a local
  socket, accept a connection, reach another process, reach an ungranted
  device, truncate outside its writes) and which resources are bounded
  (memory, processor time, file size, open files). Every platform answers
  every field. **BREAKING** for the type's fields, which nothing outside
  `uze-platform` reads. The summary text keeps its shape.
- macOS's memory bound is settled by experiment on the CI runners: it is
  enforced if an unprivileged process can bound itself, and otherwise it
  stays a declared gap.
- The confinement tests run on all three platforms. A Windows suite
  proves through the real helper what the Linux suite proves, and the
  lifecycle tests that need a POSIX shell say so on Windows rather than
  failing there.
- The security docs and the invariants name the Windows confinement and
  its limits.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `extension-confinement`: a confined program on Windows reaches only what
  it was granted, and what a machine cannot enforce is stated as an
  outcome on every platform.

## Impact

- `crates/uze-platform/src/confine.rs`, `confine/windows.rs` (new),
  `confine/linux.rs`, `confine/macos.rs`, `confine/rlimit.rs`; the
  `windows-sys` feature list in `crates/uze-platform/Cargo.toml` gains
  `Win32_Security_Isolation`. No new dependency.
- `crates/uze-application/src/application/extensions/program.rs`:
  a Windows run grants its interpreter's directory as it does on Unix.
- `tests/cli/confine.rs` (a Windows variant), `tests/cli/main.rs`,
  `tests/lifecycle/extension_programs.rs`.
- `web/content/docs/concepts/security.mdx`,
  `docs/architecture/invariants.md`.
