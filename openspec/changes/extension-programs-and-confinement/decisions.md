<!-- Decisions taken while implementing, one entry each, judged at archive. -->

## D1 — A packaged program is written `./bin/x`; any `/` makes it packaged

- status:   pending
- task:     1.1, 1.2
- commit:
- trigger:  D4 says a program is "a relative path inside the extension or a bare name" without saying how the two are told apart, and `bin/x` and `x` would both pass as names of something.
- decided:  An argv's first element containing `/` is a file inside the extension, written with or without `./` and resolved after stripping it; one without `/` is a program's name, which must also be a valid `exec:` name (letters, digits, `.`, `_`, `-`, `+`, at most 64, not starting with `.` or `-`). An absolute path is refused as outside the extension.
- rejected: A separate node (`program "bin/x"` versus `run "x"`): one spelling for both reads like the shell an author already knows.
- blast:    The decoder and the check; a manifest would only change spelling.

## D2 — A shell string is detected by its first element alone

- status:   pending
- task:     1.3
- commit:
- trigger:  The spec refuses "a single string to be split or interpreted by a shell", but an argument like `--format=%h %s` is a legitimate single argument with a space.
- decided:  Only the program element is checked: a space, tab, `|`, `&`, `;`, `<`, `>`, `$`, `` ` ``, `(`, `)`, `*` or `?` in it refuses the declaration as a shell command. Later elements are whole arguments by construction and are never split, so a space in them is data.
- rejected: Refusing shell characters in every element: it would refuse correct argv like `git log --format=%h %s`.
- blast:    A program with such a character in its file name cannot be packaged; renaming it is the fix.

## D3 — `{field}` before `--` is refused by the check, not warned

- status:   pending
- task:     1.3
- commit:
- trigger:  The parent plan (S8, 22.5) asks to refuse a dash value at run and warn in the check, but the check has no warning channel for manifest decoding (`Report` only carries errors).
- decided:  The check refuses an argument that begins with a `{field}` reference when no `--` precedes it. The run-time refusal of a value beginning with `-` stays as the second line of defence (task 3.2). An argument that only contains a reference after literal text (`--name={name}`) passes, since the value cannot become a flag there.
- rejected: Adding a warning severity to manifest diagnostics for this one rule: a manifest that would fail at the first unlucky value is not one that should install.
- blast:    Authors write `"--"` before substituted values; the scaffold shows it.

## D4 — A program source is asked at most every 30 seconds, refused below

- status:   pending
- task:     1.3
- commit:
- trigger:  The parent's 25.5 asks the check to *warn* when a program-backed `every` is under 30 s; there is no warning channel (D3).
- decided:  `every` under 30 s on a source answered by `run` is refused, naming the 30 s floor for a program; a script's floor stays 5 s.
- rejected: Accepting it silently: each answer is a process, and two lanes serve every extension.
- blast:    A meter that wanted 10 s refreshes asks for a script that calls `run_program` less often, or waits for an event.

## D5 — `output "lines"` takes an optional one-character `separator`

- status:   pending
- task:     1.2
- commit:
- trigger:  The spec's `lines` mode is tab-separated, and `nvidia-smi --format=csv`, the proof's own program, prints `, `-separated lines.
- decided:  `output "lines" separator=","` splits on that character (not a letter or digit), a tab by default, and trims spaces around each field. `fields` is required with `lines` and refused without it.
- rejected: A `csv` mode: quoting rules would make it a parser, and a separator covers what CLIs print.
- blast:    Contract `Output::Lines { fields, separator }`; removing it later means tab only.

## D6 — An `exec` node names exact paths a program reads; no globs

- status:   pending
- task:     1.2
- commit:
- trigger:  D6 writes the device reads as `/dev/nvidia*`, but Landlock grants paths, not patterns, and a glob expanded at enable time would change meaning as devices come and go.
- decided:  `exec "<name>" { reads "/abs/path" … }`, absolute paths without `..`; a directory covers what is beneath it. A `reads` for a program the manifest does not want is refused. The consent screen lists them.
- rejected: Globs resolved at run: what the operator saw on the consent screen would not be what was granted.
- blast:    `gpu-meter` lists `/dev/nvidiactl`, `/dev/nvidia0`, `/dev/dxg` and the driver directories it needs.

## D7 — A harness's own program is refused by the check, from the registry's shim names

- status:   pending
- task:     1.4
- commit:
- trigger:  D6 refuses "the harnesses' own names" by asking the integration registry, but core may not name a harness and the check in core has no registry.
- decided:  `uze-application`'s check asks every integration for its `shim_name()` (`claude`, `codex`, `opencode`, `agy`) and refuses an `exec:` grant naming one; `exec:uze` is refused by the decoder in core, which may name UZE.
- rejected: A list of harness names in core: it would break `core_never_names_a_vendor_harness`.
- blast:    A new harness is refused automatically with its integration.

## D8 — A packaged script must be granted its interpreter at compile, and a native program needs no grant

- status:   pending
- task:     1.3
- commit:
- trigger:  The spec makes a shebang need its interpreter's grant; the check reads only text, and a native program's bytes are not text.
- decided:  Compiling reads the first 256 bytes of each packaged program (`Files::head`). `#!/path/name args` or `#!/usr/bin/env name args` names `name`, which must be wanted as `exec:<name>`. `env` with options (`-S`, `VAR=`) is refused. Anything without `#!` is a native program of the extension's own, compiled without a grant and disclosed at consent as "ships a program of its own".
- rejected: Executing packaged scripts directly and letting the kernel read `#!`: `env` would search `PATH` at run time, defeating the pinning.
- blast:    `uze-core::capability::extension::program::packaged`, used again by the runner.

## D9 — A run past its output bound is ended, not drained

- status:   pending
- task:     3.1
- commit:
- trigger:  `machine::subprocess::read_bounded` keeps reading past its cap so a chatty child is not stalled, while the spec says a program past its output bound is stopped and counted a failure.
- decided:  `extension_program` reads each stream on a thread up to its bound (stdout 1 MiB, stderr 64 KiB); the first byte past it raises a flag and closes the pipe, and the run's whole group is ended twice. A program that died of the closed pipe before the flag was seen still reports `Flooded`, naming the stream.
- rejected: Reusing `read_bounded`: it would let a program write forever until its deadline.
- blast:    `crates/uze-core/src/machine/extension_program.rs`.

## D10 — `argv[0]` is the pinned path, not the program's name

- status:   pending
- task:     3.1
- commit:
- trigger:  The parent's 5.1 asks for `argv[0] = name`; the confinement helper executes the path it is given, with that path as `argv[0]`.
- decided:  A program sees its absolute pinned path as `argv[0]`, confined or not. A CLI that dispatches on its name still sees its own file name.
- rejected: A helper flag carrying `argv[0]`: one more argument parsed before confinement, for a nicety.
- blast:    A multi-call binary (`busybox`) pinned by a link would see the link's target name; it would be granted by that name.

## D11 — The environment keeps the locale, terminal, `PATH`, `HOME`, `USER` and `LOGNAME`

- status:   pending
- task:     3.1
- commit:
- trigger:  The spec lists "locale, terminal, `PATH`, `HOME`, and the contract's variables" without saying whether `USER`, `TZ` or `NO_COLOR` count.
- decided:  `LANG`, `LANGUAGE`, `LC_*`, `TERM`, `COLORTERM`, `NO_COLOR`, `TZ`, `PATH`, `HOME`, `USER`, `LOGNAME` are inherited; `TMPDIR` is the scratch directory; each flat input is also `UZE_IN_<NAME>` (upper case, `-` as `_`); Git gets `GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL=/dev/null`, `GIT_TERMINAL_PROMPT=0` and `GIT_CONFIG_COUNT` pairs disarming `core.fsmonitor`, `core.hooksPath`, `core.sshCommand` and `credential.helper`, with `safe.directory` set to the checkout. Nothing else from UZE's environment reaches it.
- rejected: Only `PATH` and `HOME`: CLIs that print dates or need a user name break for no gain, and none of these hold a credential.
- blast:    A program that needs another variable cannot have it; a later change adds declared environment if a gallery entry needs it.

## D12 — `landlock` 0.4.7 reviewed and taken

- status:   pending
- task:     2.1
- commit:   2ab73594
- trigger:  D6 names the crate and asks for the dependency review before it is taken.
- decided:  Taken as a Linux-only target dependency of `uze-platform`. Publisher: one crates.io owner, `l0kod` (Mickaël Salaün, author and kernel maintainer of the Landlock LSM), repository `landlock-lsm/rust-landlock`. MIT OR Apache-2.0, `build = false`, no C. New in the lock: `landlock`, `enumflags2` 0.7.12, `enumflags2_derive` 0.7.12 (MIT OR Apache-2.0, no build script); `libc`, `thiserror` 2 and `syn` 2 were already there. `unsafe`: about 38 sites in `landlock` (the syscall wrappers) and about 20 in `enumflags2`. `cargo deny check` passes; `CREDITS.md` regenerated. `serde` with derive joins `uze-platform` for the helper's JSON argument.
- rejected: Raw `landlock_*` syscalls over `libc`: the ruleset and ABI-compatibility logic is what the crate is for, and its publisher is the LSM's own maintainer.
- blast:    Removing it means writing the ruleset by hand in `confine/linux.rs`.

## D13 — Without seccomp the machine does not confine, whatever Landlock says

- status:   pending
- task:     2.2
- commit:   a9350e58
- trigger:  `Availability` is per right, but D6 promises no network and Landlock alone cannot refuse a Unix socket or `socket()` itself.
- decided:  `Availability::of_linux` reports the machine unavailable when the seccomp probe fails, even with Landlock present, so the "runs as you" step applies. Landlock's ABI is probed with `landlock_create_ruleset(NULL, 0, VERSION)`, seccomp with `SECCOMP_GET_ACTION_AVAIL`.
- rejected: Reporting Landlock alone as confinement: the consent screen would promise no network it could not keep.
- blast:    `crates/uze-platform/src/confine.rs`.

## D14 — Every filesystem right the ABI knows is handled, under `HardRequirement`

- status:   pending
- task:     2.3
- commit:   a9350e58
- trigger:  D5's "read, write, execute" leaves the exact Landlock rights per kind of path open.
- decided:  `read` gets ReadFile and ReadDir; `read_execute` adds Execute; `write` adds WriteFile, Remove*, MakeDir, MakeReg, MakeFifo, MakeSym, Refer and Truncate (as the ABI has them), never Execute, MakeSock, MakeChar, MakeBlock or IoctlDev. A file grant (`/dev/null`) is trimmed to the file rights. A grant path that does not exist fails the run with 125, so the caller lists only paths that exist. TCP: without network, bind and connect are handled and no port is allowed; with network (change 8) only bind is handled. At ABI ≥ 6 abstract sockets and signals are scoped.
- rejected: Skipping a missing path silently: a typo in a grant would be a grant nobody sees.
- blast:    The runner filters optional system prefixes (`/nix/store`, `/snap`) to those present.

## D15 — Signals below Landlock ABI 6 may reach positive pids; groups other than its own are refused

- status:   pending
- task:     2.3
- commit:   a9350e58
- trigger:  D6 limits signals to "the program's own group or itself" below ABI 6, but shells signal their children by positive pid, which a filter cannot tell from any other pid of the user.
- decided:  Below ABI 6, `kill`, `tkill`, `tgkill`, `rt_sigqueueinfo` and `rt_tgsigqueueinfo` are allowed for targets ≥ 0 and for `-getpgrp()`; any other negative target, `-1` included, is EPERM. `pidfd_send_signal` is refused outright. At ABI ≥ 6, the default here, Landlock's signal scoping closes the positive-pid gap.
- rejected: Refusing every positive pid: `sh` could not end its own background jobs.
- blast:    On kernels before 6.12 a program can signal another process of the same user by pid; the consent screen's per-right line names "signal scoping" as unenforced there.

## D16 — `setsid` and `setpgid` are refused, so no descendant leaves the group

- status:   pending
- task:     2.3
- commit:   a9350e58
- trigger:  D6 contains a fork bomb with `PR_SET_CHILD_SUBREAPER` in the helper, but the helper `execve`s into the program, and the runner kills by process group.
- decided:  The seccomp filter refuses `setsid` and `setpgid` with EPERM. Every descendant stays in the group the runner created, and the runner's double group kill reaches all of them; no subreaper is set. The x32 ABI is refused too, since it shares x86-64's audit arch.
- rejected: A subreaper helper that waits instead of `execve`ing: it doubles the processes per run and makes the program's exit code second-hand.
- blast:    A program that daemonizes cannot; none an extension needs should.

## D17 — The `__confine` helper is parsed before anything else in `main`

- status:   pending
- task:     2.5
- commit:   26fa6cbb
- trigger:  A hidden clap command runs after the self-update sweep, the shim check, `UzeHome` and telemetry, any of which would read or write state the confined program must not see the helper touch.
- decided:  `main` checks for `__confine` first and hands over to `uze_platform::confine::enter`; on failure it prints `uze __confine: not confined: <reason>` and exits 125. The clap variant exists only for the help and the performance classification (`JustifiedSlow`).
- rejected: Routing through clap's `run`.
- blast:    `src/main.rs`, `src/cli/confine.rs`.

## D18 — macOS denies `process-exec` outside `read_execute` and allows `file-read-metadata` everywhere

- status:   pending
- task:     2.4
- commit:   a9350e58
- trigger:  D6's profile sketch does not say whether `stat` outside the grants is allowed, and Landlock does not restrict it.
- decided:  The generated profile allows `file-read-metadata` everywhere, as Landlock does, `process-exec` only beneath `read_execute`, fixed dyld and system read paths, and a small `mach-lookup` list. DNS, TLS and configd services arrive only with network. Paths that are not UTF-8 or not absolute are refused. If `sandbox-exec` itself fails, its own exit code reaches the runner rather than 125: a known gap, listed for the macOS CI job.
- rejected: Denying metadata outside the grants: many tools `stat` their way up a tree and would fail where Linux lets them.
- blast:    `crates/uze-platform/src/confine/seatbelt.rs`.

## D19 — Rhai is highlighted with a grammar of UZE's own

- status:   pending
- task:     (operator's request, 2026-10-27)
- commit:   8218145f
- trigger:  The operator asked for Rhai highlighting beside KDL. The community grammars are TextMate grammars for Oniguruma, not Sublime syntaxes syntect loads with fancy-regex.
- decided:  `crates/uze-extensions/grammars/Rhai.sublime-syntax`, with no `extends` and no class intersections. `.rhai` files and `rhai` fences resolve to it. Two fixed-length lookbehinds tell a closure's `|` and the unit `()` apart, and a double-quoted string ends at its line unless the line ends in `\`. Map keys are scoped like KDL properties. The requirement went into `extension-manifest-and-scripts`'s `extension-scripts` spec, beside the scripts it is for.
- rejected: Vendoring a TextMate grammar through a converter: it needs Oniguruma features syntect refuses.
- blast:    Odd spacing before a closure's `|` is read as an operator.
