Order matters. Each group merges with Linux and macOS green. Windows is
offered only at 13.3, and until then `install.ps1` refuses it.

## 1. Transport port and process crate, proven on Unix (D3, D4)

- [x] 1.1 Extract `uze-terminal/src/runtime/transport/` (`Endpoint`,
      `Listener`, `Connection`, `split()`, `peer_pid()`) with today's Unix
      code behind it. These stop naming `UnixStream`:
      - `attach()`, `Handshake`, `forward_events`, `connect_waiting`,
        `serves_this_build`, `open_space`, `listening_peer`;
      - `src/ui/orchestrator/session.rs:69` and `orchestrator.rs:441`.

      Linux and macOS suites stay green with no behaviour change.
- [x] 1.2 Move the endpoint watch, the `/tmp` rebind, `private_directory` and
      `MAX_SOCKET_PATH` behind the Unix transport.
- [x] 1.3 Create the `uze-platform` leaf crate with the Unix implementations
      moved in:
      - `ProcessTree`, from `uze-core` subprocess process groups and
        `uze-git` `run_within`;
      - `probe`, from `uze-terminal/process_probe.rs` and
        `uze-core/machine/process_cwd.rs`;
      - `interrupt`;
      - `which`.

      `pid_t` becomes `u32` in signatures. Rewire `uze-core`, `uze-git` and
      `uze-terminal` to it.
- [x] 1.4 Layering: teach `tests/architecture/layering.rs` the new leaf and
      `uze-terminal`'s widened rule. Update AGENTS.md's workspace layout.
- [x] 1.5 Add `uze-platform` to `docs/architecture/crate-layering.mmd`, then
      run `cargo test -p uze-extensions` and `uze agent artifacts check`.
- [x] 1.6 Add a root `.gitattributes` (`* text=auto eol=lf`), and verify
      that `include_str!` templates, goldens and the embedded marketplace are
      byte-identical after a fresh checkout.

## 2. Compile for Windows (D1, D2)

- [ ] 2.1 Record the baseline `cargo check --workspace --all-targets
      --target x86_64-pc-windows-msvc` error list in the PR.
- [x] 2.2 Add `windows-sys` 0.61 (only the named features) as a
      `cfg(windows)` dependency of `uze-platform`, `uze-core`, `uze-git`,
      `uze-terminal` and the root crate. Make `libc` `cfg(unix)` in
      `uze-terminal` and the root crate. Record the provenance and
      transitive weight, per AGENTS.md's Dependencies section.
- [x] 2.3 Gate the ungated Unix code:
      - product code: `uze-core/src/package/authoring.rs:351`,
        `src/self_update.rs:41,212,618,635`,
        `uze-testkit/src/fake_harness.rs:255,580`;
      - test-only code: `uze-integrations/src/codex/generate.rs:409,416,446`,
        `uze-application/src/application/tests.rs:385`.
- [x] 2.4 Add Windows stubs that answer *unknown* or return an error (never
      success) wherever a Windows implementation lands later in this list,
      so that the workspace type-checks.
- [x] 2.5 Add the Windows `check` and the `x86_64`/`aarch64` msvc `clippy`
      to the Linux `lint` job, gating from here on.
- [x] 2.6 Add the Windows x64 test row to `ci.yml`, non-gating, running
      under `pwsh` with `HOME` unset, a minimal `PATH`, and an assertion
      that `sh`/`bash`/`jq`/GNU `tar` do not resolve (D19).

## 3. Platform-honest primitives (D3, D8, D9, D15)

- [x] 3.1 `uze-platform` on Windows:
      - `ProcessTree`: a suspended spawn, then Job Object assignment, then
        `ResumeThread` via a Toolhelp thread snapshot, terminating the
        process if the assignment fails;
      - `which` (`PATH` × `PATHEXT`);
      - `interrupt` (`SetConsoleCtrlHandler`).
- [ ] 3.2 `uze-platform::probe` on Windows:
      - the pipe peer pid;
      - `QueryFullProcessImageNameW`;
      - the image stem;
      - cwd and environment from the PEB, both native and WOW64 layouts;
      - liveness that answers *unknown*;
      - a Toolhelp table walk filtered by SID.

      Add one Windows test per fact.
- [x] 3.3 Delete `kill_reaped_process_group`'s `taskkill`. Back
      `with_process_group`, `uze-git` `run_within`/`run_bounded` (whose
      deadline now holds on Windows), the provisioning runner and
      `run_shell_bounded` with `ProcessTree`.
- [ ] 3.4 Locks: on Windows, `LockFileEx` on a sentinel byte range for
      `persistence::try_lock_exclusive` (`MutationLock`, `AgentsMdGuard`
      `project_context.rs:123`, task store `task.rs:713`), `uze-git`
      `lock.rs:89-109` and `uze-terminal` `runtime/lock.rs`.
      - The pid stays at offset 0 and remains readable.
      - Delete the `Ok(())` fallbacks.
      - Map `ERROR_LOCK_VIOLATION` where `WouldBlock` is matched
        (`CONTENTION_GRACE`).
- [x] 3.5 `persistence::process_is_alive` through `probe`. An *unknown*
      answer is never treated as alive or dead, and
      `remove_abandoned_swaps` skips an unknown pid
      (`uze_platform::process::alive` answers `Option<bool>`, and only a
      `Some(false)` lets a swap be removed).
- [x] 3.6 Home: `UzeHome::from_env` on `std::env::home_dir()`.
      - The integrations' `from_env` (claude.rs:127, codex.rs:124,
        opencode.rs:113, antigravity.rs:170) take home from `UzeHome`.
      - `uze-terminal` `state.rs:55` and `runtime/lock.rs:12` call
        `home_dir()` locally.
      - `run_captured` sets `USERPROFILE` as well.
- [ ] 3.7 Paths: `strip_verbatim`, `same_path` (ordinal, case- and
      separator-insensitive on Windows) and `display` in `uze-core`,
      re-exported through `uze-application` for `src/`.
      - Route the comparing sites through them: `harness_runtime.rs:105-130`,
        `shared/tree.rs:69-82`, `shared/path.rs` pruning,
        `uze-git/repository.rs:72`, `checkout/accounting.rs`.
      - `uze-terminal` `endpoint.rs:391` `path_with_first` uses its own copy.
      - `project_id_for` normalizes once, internally.
- [x] 3.8 Locators: `forge.rs:378-443` accepts `C:\`, `.\`, `..\` and `~\`;
      in `acquisition.rs:232-241` a drive letter is never `host:`.
- [x] 3.9 One sharing-violation wait, `uze_platform::fs::rename`: on
      Windows a rename failing with a sharing violation, a lock violation
      or access denied is tried again for about two seconds. Used by
      `persistence::replace_atomically`/`swap_in`,
      `uze-terminal/src/runtime/persist.rs`, `src/ui/extension_host.rs`
      saves and `executable::replace_running` (self-update). `uze-document`
      names no platform and keeps a plain rename for its rare set-aside.
- [ ] 3.10 Git acquisition (`acquisition/git.rs`):
      - keep the Windows system variables through `env_clear()`;
      - `pushed_config` adds `core.autocrlf=false`, `core.eol=lf`,
        `core.fileMode=false`, `core.symlinks=false` and
        `core.longpaths=true`;
      - `NUL` for the null paths;
      - `.netrc`/`_netrc` from `UzeHome`.

      Also set `core.longpaths=true` on `git worktree add`.
- [x] 3.11 `create_symlink` callers each get a Windows answer, none needing
      a privilege an ordinary session lacks:
      - a link to a directory is a junction (`uze_platform::fs::symlink`),
        which the standard library reads as a link, so Claude's runtime
        projection and every `read_link` comparison stand unchanged;
      - `checkout/pool.rs` links through `fs::link_entry`: a junction for a
        directory, a hard link for a file (the primary's file under a
        second name, read as it is now, where a copy would be stale at
        once);
      - `exposure::attach_symlink` makes only directory references, which
        are junctions.
- [ ] 3.12 Private directories (`record.rs:164`, `acquisition.rs:442`,
      `telemetry.rs:286`, prompt history) get an owner-only DACL on
      Windows. Prompt history's local day uses `GetTimeZoneInformation`.
- [ ] 3.13 The root `build.rs` embeds an application manifest
      (`longPathAware`, UTF-8 `activeCodePage`) through
      `cargo:rustc-link-arg-bins`. The client sets the console code pages
      to 65001.
- [ ] 3.14 Windows tests:
      - lock contention across two processes, with the holder pid readable;
      - a deadline killing a grandchild;
      - a `.cmd` resolved through `PATHEXT`;
      - the verbatim and case compare;
      - a sharing-violation retry;
      - a cross-platform digest using `text=auto` and symlink fixtures.

## 4. Terminal runtime on Windows (D5–D8)

- [ ] 4.1 Windows transport:
      - an overlapped named pipe with `FIRST_PIPE_INSTANCE`,
        `REJECT_REMOTE_CLIENTS` and a DACL for the token user SID;
      - a pipe name from `sha256(UZE_HOME, SID)`;
      - one instance always pending;
      - the client loops on `WaitNamedPipeW` within the deadline;
      - deadlines are a wait timeout, then `CancelIoEx`, then
        `GetOverlappedResult`.

      Write the invariants at the top of the module, and add a
      concurrent read/write stress test.
- [ ] 4.2 The client opens with `SECURITY_IDENTIFICATION` and verifies the
      pipe owner SID, the server pid plus its start time, and that the image
      is `uze` (including `uze.exe.old-*`). Otherwise it reports the holder.
      `unreachable()` names `Get-Process uze` on Windows. Unix gains the
      symmetric server-uid check.
- [x] 4.3 Server spawn (`process.rs:45-110`):
      - `EXE_SUFFIX`;
      - `CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP |
        CREATE_BREAKAWAY_FROM_JOB`, retrying without breakaway and warning
        once;
      - home as the working directory.
- [x] 4.4 Stop event `Local\uze-stop-<hash>`: the server persists and exits
      on it. `retire()` signals it, then uses `TerminateProcess` only on a
      confirmed `uze` image. Test: an upgrade, then attaching to and
      retiring the old server.
- [ ] 4.5 `uze __pane-host <job> -- <argv>`:
      - creates the named job (owner DACL, `KILL_ON_JOB_CLOSE`) and joins it;
      - `SetConsoleCtrlHandler(NULL, FALSE)`;
      - spawns the program.

      Classify it `JustifiedSlow` in `src/command_performance.rs`. The
      server opens the job by name. `stop`, `end_leftovers` and shutdown
      become `TerminateJobObject`. The process-group code in
      `server.rs:1068-1088` is Unix-only.
- [ ] 4.6 `ClosePseudoConsole` is called off the reader thread, with output
      drained until it returns.
- [ ] 4.7 Foreground status from the job's process list (newest leaf, with
      parents checked by creation time, trampoline and shim skipped). Feed
      it into `refresh_pane_status` and `shim_launched_name`.
      `PLAIN_SHELL_PROCESS_NAMES` adds `cmd`, `powershell`, `pwsh` and
      `bash`, matched on the stem. Relaunch refuses `\` and `:`.
- [x] 4.8 Default pane shell: `UZE_SHELL`, else `pwsh`, else `powershell`
      (no `COMSPEC`). No `TERM` is set on Windows; `COLORTERM=truecolor` is.
- [ ] 4.9 Windows tests:
      - Ctrl+C stops `ping -t` in a pane;
      - the startup DSR is answered;
      - resize reflow;
      - the reader ends after a pane closes;
      - closing the launching console leaves panes alive;
      - a second local account is refused.

## 5. Commands per platform (D10, D11)

- [x] 5.1 A `Command` value in `uze-core`: a string, or a map
      `{posix?, windows?}`, with `resolve(platform)`.
      - `CommandHook.command: Command` (`hook.rs:110`).
      - `WorktreePolicy.setup`/`gate: Vec<Command>`; `one_or_many` gains
        `visit_map` (`worktree.rs:332,395`).
      - The views carry it: `DeliveryPolicyView.gate` (`views.rs:430`),
        `landing/readiness.rs:24`, `checkout::materialize`.
      - No importer reads a vendor's hook file today, so Codex's
        `commandWindows` has nothing to map it.
- [x] 5.2 `run_shell_bounded` runs the resolved spelling:
      - `sh -c` on Unix;
      - on Windows, `powershell.exe -NoProfile -NonInteractive
        -ExecutionPolicy Bypass -Command
        "$ErrorActionPreference='Stop'; <line>; exit $LASTEXITCODE"`, with
        UTF-8 console encoding.

      Test that a native exit 3 stays 3, and that a failing first command
      fails the line.
- [x] 5.3 A command with no spelling for the platform is never run:
      - a setup step is skipped with a warning naming it (a setup failure
        never blocks a launch, so neither does this);
      - a gate is reported at placement, and delivery fails closed.

      `uze status` lists every project command missing a spelling for this
      platform.
- [ ] 5.4 Detect a Group Policy execution policy and Constrained Language
      Mode once, in the detection cache. On such a machine, commands and
      hooks are Unsupported with the reason. `uze doctor` reports it with
      the machine checks (`git.exe`, `ssh.exe` when an SSH marketplace
      exists, VT, OS build, `Path`, Smart App Control).
      (Done: `uze_platform::shell::refusal`, kept a day in
      `cache/shell.json`, and `uze doctor`'s `shell` problem beside `git`.
      Open: hooks reported Unsupported on such a machine, and the other
      machine checks.)
- [x] 5.5 Trust shows and compares both spellings; a Windows-only change
      re-prompts. A `hooks.json` parse failure fails trust closed
      (`trust.rs:157`).
- [x] 5.6 `uze agent plugin check` warns about a handler with no `windows`
      spelling. The minimum uze version the map form requires is stated
      in the `uze:author` skill, not warned on: every plugin written the
      recommended way would carry the warning.
- [x] 5.7 The scaffold (`authoring.rs:348` + `guard.sh`) writes
      `scripts/guard` and `scripts/guard.ps1` and declares both spellings.
      Update the `uze:author` and `uze:worktree` skills, and the region
      UZE projects into `AGENTS.md`. (Neither the worktree skill nor the
      region spells a command; the per-platform form is documented in
      `reference/project-files`.)
- [ ] 5.8 The workspace client says, when it opens on a project, which
      gates this machine cannot run (the read model is `uze status`'s
      `steps_not_spelled_here`).

## 6. Hooks and delivery on Windows (D12, D16, D17)

- [x] 6.1 `exec.ps1` template (UTF-8 BOM):
      - each handler runs as a `powershell.exe -File` process of its own,
        from a script file holding the author's line as written, under its
        deadline; past it the process and everything it started end
        (`taskkill /T /F`);
      - the payload is read whole by `JavaScriptSerializer` with no length
        limit (what `ConvertFrom-Json` lacks), and fields are looked up
        with `ContainsKey`;
      - any fault of the wrapper itself goes to `Fail`, which follows the
        group's effect;
      - UTF-8 console encoding in and out.

      `deliverable()` means "a template exists, and every handler is spelled
      for this platform".
- [ ] 6.2 Measure on a real Windows machine, per harness and version, and
      record the result in the dialect table:
      - the hook entry form: Claude exec form with `args` and its minimum
        version; Codex `commandWindows` with `&`; Antigravity unquoted
        through `cmd /C`, with the wrapper placed under a no-space or 8.3
        path;
      - which events fire, including Codex shell `PreToolUse`
        (openai/codex#24453);
      - the label a loose skill and agent shows under `<plugin>-<capability>`;
      - the MCP `.cmd` launcher form.
- [ ] 6.3 Generate those entries per harness. Paths use forward slashes.
      The OpenCode bridge (`hooks/bridge.rs:196`) spawns `powershell.exe`
      on Windows. `hooks/entries.rs` quoting becomes per dialect.
- [x] 6.4 A group with a `deny` or `ask` effect and no Windows spelling
      makes the install fail non-zero, naming the group, with nothing of the
      package attached. Observational groups are reported Unsupported. A
      Codex/Windows shell-matching group is Unsupported, with the issue
      link.
- [ ] 6.5 The shared fixture set:
      - `sh` goldens plus a Windows spelling per fixture;
      - a large payload (over 2 MB);
      - non-ASCII content.

      `exec.ps1` answers every fixture identically. The latency budget
      (≤ 400 ms p50 per `PreToolUse`) is measured and recorded.
      Done: `hooks/fixture_set.rs` holds the fixtures (now with non-ASCII
      text and a 2.1 MB payload) and the recorded answers;
      `hooks/wrapper_parity_tests.rs` holds the wrapper of the platform it
      runs on to every one of them, by decision, and `exec.ps1` answers them
      all on Windows. Measured on a Windows 11 host: `exec.ps1` with one
      handler, p50 658 ms (375 ms of it the wrapper's own PowerShell, the
      rest the handler's). Open: the 400 ms budget, which needs the handler
      run in the wrapper's process (D12's runspace) rather than a second
      `powershell.exe`.
- [x] 6.6 On-disk names on Windows: `<plugin>-<capability>` with collision
      detection that fails, naming both packages. Refuse reserved device
      names, `< > " / \ | ? *`, control characters and a trailing dot or
      space. Applies to `claude/skills.rs:104`, `codex/skills.rs:145`,
      `opencode/skills.rs:159`, `antigravity/skills.rs:126` and
      `shared/agent.rs:79`. A test checks that no write creates an alternate
      data stream. Native plugin delivery is preferred where it exists.
      (`uze_platform::fs_name::file_name_for` spells the label; a receipt
      is matched to a label through `ManagedArtifact::is_named`, so two
      labels the filesystem holds under one name contend and the second is
      a `ProjectionConflict` naming both; the suites write no `:` name.)
- [x] 6.7 MCP on Windows: a command that resolves to a `.cmd`/`.bat` is
      projected as `cmd /c <command> <args>`
      (`uze_platform::executable::direct_launch`, applied where a server is
      resolved, so the envelope and the managed entry both carry it).
- [ ] 6.8 Windows acquisition records Git link entries as links, by
      `digest.rs`'s rule, and materializes them as in-package copies for
      harnesses.

## 7. Shims and provisioning (D13)

- [x] 7.1 `runtime_shim.rs:49-82` places `<harness>.exe` as a copy of
      `uze.exe`.
      - A differing shim is swapped by rename-aside, never overwritten.
      - The comparison is size and mtime first, then a hash; it runs off
        the draw thread.
      - `EXE_SUFFIX` at `tasks.rs:140` and `doctor.rs:756`.
      - The `.old-*` sweep covers the shims directory.
      - Both patterns are named in `UzeHome`.
- [x] 7.2 `shim::detect` matches on `file_stem`. On Windows,
      `run_replacing_process` runs the harness in a `ProcessTree`, swallows
      Ctrl+C for itself and forwards the exit code. The workspace identifies
      the harness through `UZE_SHIM_PID` and the foreground walk.
- [ ] 7.3 `.cmd`/`.bat` launchers spawn correctly from the shim,
      `run_captured` and `CapturingRunner` (`src/cli/setup.rs:431`).
      Surface `std`'s `InvalidInput` for arguments it cannot escape.
- [ ] 7.4 Windows provisioning routes, kept in each integration and run in
      Windows PowerShell, the script downloaded as text (`irm` hands a
      script served as `application/octet-stream` back as bytes) and run
      with `Invoke-Expression`, in a hidden console of its own (with no
      console at all PowerShell runs nothing). After the installer, the
      executable is looked for again on this process's `PATH`, then on the
      one a new shell gets from the registry. Claude (2.1.289) and Codex
      (0.160.0) installed and delivered in Windows Sandbox, 2026-10-04:
      - Claude: `https://claude.ai/install.ps1`, verified at
        `%USERPROFILE%\.local\bin\claude.exe`;
      - Codex: `https://chatgpt.com/codex/install.ps1`;
      - Antigravity: `https://antigravity.google/cli/install.ps1`, with its
        location confirmed;
      - OpenCode: no automatic route. An installed `opencode.exe` takes
        `opencode upgrade`; otherwise it is Blocked, naming the
        `scoop`/`choco`/`npm` commands.

      `platform_has_automated_route` becomes per integration and per
      platform. `uze setup` does not update a harness whose executable a
      pane holds.
- [x] 7.5 Claude's sandbox preference is reported Unsupported on native
      Windows (`claude/preferences.rs:61`).
- [ ] 7.6 Update ADR-010's Windows paragraph to the routes now automated.

## 8. CLI, TUI and upgrade (D14, D21)

- [ ] 8.1 Self-update:
      - `asset()` for Windows;
      - System32 `tar.exe`/`curl.exe` by absolute path, `NUL`, `uze.exe`;
      - `replace()` renames aside, then in, with an undo and the retry
        helper;
      - the startup `.old-*` sweep;
      - D6 flags in `hand_off_check`;
      - a shim refresh (`executable::refresh_launchers` after the binary
        is replaced: done).

      Update `the_asset_is_the_one_the_installer_picks`.
- [x] 8.2 `src/main.rs`: `--quiet` through `SetStdHandle(NUL)`, and a
      `BrokenPipe` on stdout ends quietly.
- [x] 8.3 Colour from `is_terminal()` plus VT enable (`progress.rs:98`),
      through `uze_platform::stdio::escapes_reach_the_terminal` (it used
      to require `TERM`, which Windows never sets).
      Home display through the re-exported `display`: `src/ui.rs:314`,
      `progress.rs:320`, `orchestrator.rs:361`, `tabs.rs:437`,
      `root_picker.rs:360`.
- [x] 8.4 Opener: `explorer.exe <url>` (`uze_platform::desktop::URL_OPENERS`),
      which hands an address to the default browser with none of the
      `cmd /c start` parsing that splits one at its `&`.
- [x] 8.5 Keys:
      - drop `Release`/`Repeat` before notice dismissal
        (`session/keys.rs:92-101`, `session/manage.rs:97-106`);
      - Ctrl+Alt plus a printable character is text when unbound
        (`orchestrator/input.rs:205-250`);
      - `TestBackend` tests for both.
- [x] 8.6 Display names split on both separators
      (`uze-extensions/shared/checkout.rs:56`, `code.rs:920`). Verify
      `git diff --no-index /dev/null` (`code/changes.rs:211`) on Git for
      Windows (`/dev/null` is Git's own spelling of no file there too, the
      same reason acquisition passes it as `GIT_CONFIG_GLOBAL`).

## 9. The suite on Windows (D18)

- [ ] 9.1 `uze-testkit`: `FakeHarness`, `scripted_agent` and `FakeSsh`
      dispatch through `uze-fake-harness` (a `.exe` copy on Windows, a
      symlink on Unix).
- [ ] 9.2 Port the inline shebang fakes in the 32 test files, and replace
      `/tmp` and `/bin/sh` literals with testkit helpers. Gate the genuinely
      Unix tests `#[cfg(unix)]` with a reason, each with a Windows
      counterpart where the behaviour exists.
- [ ] 9.3 `cargo test --workspace --no-fail-fast` passes under `pwsh` on
      `windows-2025` and `windows-11-arm`. The x64 row becomes gating.

## 10. Journeys on Windows (D20)

- [ ] 10.1 `journey.py`: `Screen` (tmux, or pywinpty + pyte with DSR
      answered and clicks mapped) and `Machine` (`/proc`/`ps`, or psutil)
      backends. `fcntl` and `termios` move into the Unix backend. Linux and
      macOS runs are unchanged.
- [ ] 10.2 `journeys/requirements-windows.txt`, pinned, installed with
      `--only-binary :all:` (check for `win_arm64` wheels). The composite
      action uses `actions/setup-python` on Windows.
- [ ] 10.3 World setup: `shell:` steps run under the runner's Git Bash by
      absolute path, outside the environment of the `uze` under test. World
      stand-ins become `uze-fake-harness` copies. Provisioning checks gain
      their PowerShell-route counterpart.
- [ ] 10.4 `07-upgrade` is declared unsupported on Windows, with its reason,
      until a Windows release exists. `tap` and OSC 52 get a ConPTY answer,
      or the check stops the run.
- [ ] 10.5 The whole suite passes on `windows-2025` and `windows-11-arm`.
      Fix what it finds in product code. In `journeys.yml`, the slice map
      gains `windows: 4` and `windows-arm: 2`, and both are in the default
      `PLATFORMS`.

## 11. CI and release (D19)

- [ ] 11.1 `ci.yml`: Windows rows triggered per D19. Caches save from the
      nightly only, with `CARGO_INCREMENTAL=0`. Defender excludes the target
      directory. The proof key treats an empty image id as unknown.
- [ ] 11.2 `installer-windows` job: PSScriptAnalyzer and Pester 5 (pinned)
      on 5.1 and pwsh 7, against a fake release over `python -m
      http.server`, with a mockable refusal gate. It covers:
      - a pinned version;
      - a checksum mismatch;
      - Arm64 under emulation;
      - an old build refused;
      - the BOM-less receipt;
      - `Path` as `REG_EXPAND_SZ`;
      - a missing `git.exe`;
      - `-Uninstall`.

      Add the job to `gate`.
- [x] 11.3 `.cargo/config.toml`: `+crt-static` for windows-msvc. Remove the
      stale `onig_sys` musl `[env]`, and the `musl-tools` step at
      `release.yml:312-327`.
- [ ] 11.4 `release.yml`:
      - Windows targets: `windows-2025` and native `windows-11-arm`;
      - a `tar.exe -a -cf` zip and a `uze.exe --version` smoke step;
      - SBOM with `--target all`;
      - `publish` globs, SHASUMS and attestation take `*.zip`;
      - the "six tarballs" text updated.

      Keep the Windows rows behind a repository variable. Add a
      `workflow_dispatch` package-only release-candidate run.
- [ ] 11.5 `cliff.release.toml`: the PowerShell install and verify lines,
      and the SmartScreen and Smart App Control notes.

## 12. Install, uninstall, say so (D22)

- [x] 12.1 `install.ps1` per D22:
      - a scriptblock body, errors thrown;
      - native architecture read from the registry;
      - the build ≥ 19045 check and the `git.exe` check;
      - `Get-FileHash`;
      - `%LOCALAPPDATA%\Programs\uze\bin`;
      - `Path` as `REG_EXPAND_SZ`, with `WM_SETTINGCHANGE` and the current
        session updated;
      - the receipt as UTF-8 without BOM;
      - the Smart App Control warning;
      - `-Uninstall` and `-Purge`.

      It refuses Windows as "not yet supported" until 13.3.
- [ ] 12.2 `web/vercel.json`: rewrite `/i.ps1`. A Scoop manifest is
      generated by the release.
- [ ] 12.3 Docs:
      - `web/content/docs/installation.mdx`: the Windows tab gets the
        PowerShell line, the prerequisites (Windows 10 22H2+ or 11, Git, a
        VT terminal), per-platform command spellings, SmartScreen and Smart
        App Control, uninstall, and the note that `/i.ps1` serves `main`;
      - `README.md:21,50`, `roadmap.mdx`, `development.mdx:119-129` and
        `docs/versioning.md`.
- [ ] 12.4 `docs/architecture/invariants.md`: add each property with the test
      that proves it:
      - a cross-process lock with a readable holder;
      - a pipe private to its user;
      - a pane ends with its tree;
      - filesystem-valid names;
      - identical digests across platforms;
      - no POSIX tool on a Windows path UZE runs.

## 13. Proof before offering it

- [ ] 13.1 A full nightly run green on both Windows rows: build, clippy,
      test, journeys.
- [ ] 13.2 Install a release candidate by hand on Windows 11 x64 and on
      Windows on Arm through `irm https://uze.sh/i.ps1 | iex`. On each:
      - `uze setup` for every harness;
      - `uze install` of a plugin with deny hooks spelled for Windows;
      - `uze workspace` with an agent pane surviving the launching tab's
        close;
      - `uze upgrade` while the workspace and a shimmed harness run;
      - `install.ps1 -Uninstall`.
- [ ] 13.3 Set the repository variable that enables the Windows release
      rows, and make `install.ps1` stop refusing Windows, in the same PR.
