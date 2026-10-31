## 1. Outcomes in `Availability`

- [x] 1.1 Replace `Availability`'s mechanism fields with the outcome fields of D6, add `Filesystem::AppContainer`, and rewrite `summary()` to name what a program can still reach and which limits are missing
- [x] 1.2 `of_linux` maps each Landlock ABI and the seccomp filter onto the outcomes, and macOS answers them from Seatbelt (`memory` false, D7); unit tests cover ABI 2, 3, 5 and 6, macOS and Windows summaries

## 2. Windows confinement

- [x] 2.1 `confine/windows.rs`: create the per-run profile (D1), sweep profiles of dead helpers, and delete the run's profile when the program ends
- [x] 2.2 Grants (D3): derive each `(right, path)` capability, skip a path that already carries its ACE or that `ALL APPLICATION PACKAGES` already reaches, otherwise add the inheritable ACE; refuse `devices` and a path whose DACL cannot be changed
- [x] 2.3 Start the program suspended in a bounded job (D4), with only the standard handles (D5) and `internetClient` when `network`, then resume it, wait, and exit with its code
- [x] 2.4 `availability()` reports AppContainer with the outcomes of D6; add `Win32_Security_Isolation` to the `windows-sys` features

## 3. Proof on every platform

- [x] 3.1 A Windows half of `tests/cli/confine.rs` through the real helper, with `cmd.exe` and PowerShell: reads, writes and executes only as granted, the profile stays unreadable, there is no network without the grant, with it there is the Internet but no loopback, other processes are out of reach, memory past the bound is refused, and the exit code and environment pass through
- [x] 3.2 The lifecycle tests that need a POSIX shell say why they skip on Windows, and one Windows lifecycle test runs a pinned `cmd.exe` that reads the checkout and nothing outside it
- [x] 3.3 macOS is proved by the lifecycle suite, which runs its programs confined under Seatbelt; its `tree` outcome decides the one test that needs a system-call filter
- [x] 3.4 CI is green on Linux, macOS, Windows and Windows Arm
- [x] 3.5 Remove `lab/` and `.github/workflows/confinement-lab.yml`, its findings recorded in the design

## 4. Documentation

- [x] 4.1 `web/content/docs/concepts/security.mdx` names the three confinements, what each enforces, and Windows' readable HKCU environment (D8)
- [x] 4.2 `docs/architecture/invariants.md` cites the Windows tests beside the Linux ones; `openspec validate --all --strict` passes

## 5. Open before archiving

Found by the polish review of this change. Each says what is known and
what would settle it.

- [ ] 5.1 A grant that never finished: the skip in `grant` checks only the ACE on the granted directory. If propagation was cut short (the run was killed while a large tree was being rewritten) or reached a child whose inheritance is turned off, the files beneath stay unreadable for good, and later runs never repair them. This fails closed. Settle it with a test that kills the helper mid-grant (or protects a child) and reads beneath. Fix it with a marker written once propagation completes, or by repairing on access denied.
- [ ] 5.2 `fetch` through a proxy on the loopback or the local network fails on Windows, because `internetClient` reaches neither, while Linux and macOS reach both. Decide whether a network grant also takes `privateNetworkClientServer` (the LAN), and how a loopback proxy is reached, which an app container cannot do without an exemption made as administrator.
- [ ] 5.3 A pinned `.cmd`/`.bat` program is refused under confinement on Windows (npm's `npx.cmd` and the like). Decide whether that refusal stands, or whether a batch file runs through `cmd.exe` with arguments escaped the way Rust's `std` does, refusing those it cannot.
- [ ] 5.4 Inside an app container `cmd.exe` starts no program ("Access is denied"), while the helper and PowerShell start the same programs. Find the cause (cmd's own search and its reads of parent directories are the suspects) and whether a grant or a launch option lifts it, or keep it as a documented limit.
- [ ] 5.5 Suspected: the run's profile is deleted right after its job is closed, while the job's processes are still being ended, so a descendant holding a file in the profile's `TEMP` could leave the folder behind. Confirm it with a test whose program leaves such a child. If it holds, wait for the job's active process count to reach zero before the delete.
- [ ] 5.6 Windows bounds a program's user time and not its kernel time, so the processor limit is partial (security page). Decide whether that stays documented, or whether the helper adds a watchdog on the job's total time.
- [ ] 5.7 Git's Windows test skips on Windows Server, which closes `NUL` to app containers, so the x64 CI leg never runs it. Decide whether a Windows 11 x64 runner joins the matrix, or whether the Arm leg alone stands as Git's proof.
