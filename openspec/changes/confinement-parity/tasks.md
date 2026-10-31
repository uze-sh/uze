## 1. Outcomes in `Availability`

- [ ] 1.1 Replace `Availability`'s mechanism fields with the outcome fields of D6, add `Filesystem::AppContainer`, and rewrite `summary()` to name what a program can still reach and which limits are missing
- [ ] 1.2 `of_linux` maps each Landlock ABI and the seccomp filter onto the outcomes, and macOS answers them from Seatbelt (`memory` false, D7); unit tests cover ABI 2, 3, 5 and 6, macOS and Windows summaries

## 2. Windows confinement

- [ ] 2.1 `confine/windows.rs`: create the per-run profile (D1), sweep profiles of dead helpers, and delete the run's profile when the program ends
- [ ] 2.2 Grants (D3): derive each `(right, path)` capability, skip a path that already carries its ACE or that `ALL APPLICATION PACKAGES` already reaches, otherwise add the inheritable ACE; refuse `devices` and a path whose DACL cannot be changed
- [ ] 2.3 Start the program suspended in a bounded job (D4), with only the standard handles (D5) and `internetClient` when `network`, then resume it, wait, and exit with its code
- [ ] 2.4 `availability()` reports AppContainer with the outcomes of D6; add `Win32_Security_Isolation` to the `windows-sys` features

## 3. Proof on every platform

- [ ] 3.1 A Windows half of `tests/cli/confine.rs` through the real helper, with `cmd.exe` and PowerShell: reads, writes and executes only as granted, the profile stays unreadable, there is no network without the grant, with it there is the Internet but no loopback, other processes are out of reach, memory past the bound is refused, and the exit code and environment pass through
- [ ] 3.2 The lifecycle tests that need a POSIX shell say why they skip on Windows, and one Windows lifecycle test runs a pinned `cmd.exe` that reads the checkout and nothing outside it
- [ ] 3.3 macOS runs `tests/cli/confine.rs` as well, with its system prefixes
- [ ] 3.4 CI is green on Linux, macOS, Windows and Windows Arm; remove `lab/` and `.github/workflows/confinement-lab.yml`

## 4. Documentation

- [ ] 4.1 `web/content/docs/concepts/security.mdx` names the three confinements, what each enforces, and Windows' readable HKCU environment (D8)
- [ ] 4.2 `docs/architecture/invariants.md` cites the Windows tests beside the Linux ones; `openspec validate --all --strict` passes
