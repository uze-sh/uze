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

## D20 — Programs are pinned at consent, per machine; the project is refused at run

- status:   pending
- task:     4.1
- commit:
- trigger:  D6 pins `exec:` "at enable time", but consent is per machine and enabling per project (change 4's D41), and the pinned path cannot depend on which project enabled it.
- decided:  `consent` resolves every `exec:` name once and records `exec_paths` with `enforced_at_accept`, refusing a resolution beneath `$UZE_HOME` or the system temporary directory. A pinned path beneath the checkout a run is asked for is refused by the runner at that run. A name not found is absent from `exec_paths` and its source says "`<name>` not found" until the operator presses "Resolve again", which re-pins without asking consent again (it is the operator's gesture, and the screen shows the new path).
- rejected: Pinning per project: one extension would run a different `gh` in each project for the same consent.
- blast:    `Extensions::{consent, resolve_again, pinned_programs, programs_to_pin}`.

## D21 — Accepted unconfined, any new digest asks again; accepted confined, losing confinement asks again

- status:   pending
- task:     4.1, 4.3
- commit:
- trigger:  D6 re-asks an unconfined extension "by digest on every update" and refuses a run whose confinement no longer applies; change 4's update rule applies any same-grant update silently.
- decided:  `consent::judge` returns `Unconsented` for a changed digest when `enforced_at_accept` is `false`. `judged` returns `NeedsConsent` when the record says `true` and the machine no longer confines. Either way the projects it was enabled in stay recorded, so consenting again is the only step.
- rejected: A new state "needs consent again": the screens already say "needs consent", and the reason is in the record.
- blast:    `uze-workspace::extensions::consent::judge`, `uze-application` `judged`.

## D22 — The programs step is a second dialog, and nothing is recorded until both are allowed

- status:   pending
- task:     4.2
- commit:
- trigger:  The spec asks `exec:` grants "after the other grants", without saying whether it is one dialog with two parts or two dialogs.
- decided:  Allowing the grants dialog opens `Confirmation::AllowPrograms` with each program's pinned path, the devices it reads, "can run anything" for a general runner, and the fixed "runs as you" text where unconfined (titled "…run as you?", danger tone). Only accepting it records consent and enables. An extension whose only grants are programs opens straight on that step. `uze install` asks the same two questions in order. The `exec:` grants are left out of the first list.
- rejected: One dialog listing both: an operator who reads "reads the checkout" and presses Allow would also have agreed to a program running as them.
- blast:    `src/ui/model.rs` (`ProgramsStep`), `src/ui/overlay.rs`, `src/cli/extensions.rs`.

## D23 — A packaged program runs from a copy of the loaded bytes, in the run's own directory

- status:   pending
- task:     5.1
- commit:
- trigger:  The plan executes "the program the declaration names", but the files on disk can change between the load whose digest was accepted and the run, especially for a linked extension.
- decided:  Each run gets `runtime/extensions/scratch/<pid>-<n>/` with `tmp/` (its `TMPDIR`, writable) and `bin/` (read and execute). A packaged program is written into `bin/` from the `Loaded` bytes of the consented compile and made runnable; a packaged script runs as its pinned interpreter with that copy as argument. The directory is removed after every run.
- rejected: Executing from the extension's Store directory: what ran could differ from what was digested.
- blast:    `uze-application::extensions::program::prepare`.

## D24 — An `exec` `reads` path under `/dev/` is a device: read, write and `ioctl`

- status:   pending
- task:     5.1
- commit:
- trigger:  The proof's `nvidia-smi` cannot initialize NVML under Landlock ABI ≥ 5 with `/dev/dxg` only readable: it needs `ioctl` and write on the device (tried on this machine: "Failed to initialize NVML" without, the GPU's line with).
- decided:  `Confinement` gains `devices`, granted read, `WriteFile` and `IoctlDev` (Seatbelt: `file-read* file-write-data file-ioctl`), never make or remove. A `reads` path beginning with `/dev/` goes there; any other stays read-only.
- rejected: A separate `devices` node in the manifest: the operator reads one list of what the program reaches, and `/dev/` already says device.
- blast:    `uze-platform::confine::{Confinement, linux, seatbelt}`, the consent text lists them as reads.

## D25 — A source that refuses fails, an action that refuses is a notice

- status:   pending
- task:     5.1
- commit:
- trigger:  The spec says exit 3 is "a refusal whose reason is the surface's notice", but a source that refuses has no data, and `Answer` has no notice-without-data for a source.
- decided:  An action's exit 3 answers an envelope whose `notice` is the reason (nothing refreshed). A source's exit 3 is `Failed` with "`<program>` refused: <reason>", which the surface draws as its failure line.
- rejected: An empty envelope with a notice for sources: it would draw an empty board as if the program had answered nothing.
- blast:    `program::outcome`.

## D26 — The confinement helper is this binary, and the runner's default confines

- status:   pending
- task:     5.1
- commit:
- trigger:  The runner needs the helper's path, and `ExtensionRunner` was `Default`, which the workspace builds with `Arc::default()`: a default that ran programs unconfined would be the easy mistake.
- decided:  `ExtensionRunner::new()` and its `Default` use `std::env::current_exe()` as the helper when the machine confines. `confining_through(helper)` names one explicitly (the tests pass `CARGO_BIN_EXE_uze`, or a broken helper). A runner with no helper refuses a run whose record says it was accepted confined.
- rejected: A helper path in `UzeHome` or the config: the helper must be the binary whose code the operator trusted.
- blast:    `runner.rs`.

## D27 — A helper that could not confine withholds that run; the record is left alone

- status:   pending
- task:     4.3, 5.1
- commit:
- trigger:  The spec says an extension whose confinement failed "SHALL show as needing consent again"; the helper's 125 can also mean a single run's grant path vanished, which is not the machine losing confinement.
- decided:  A run that comes back 125 is withheld, "its confinement did not apply (<reason>); nothing ran", and nothing else changes. Losing confinement for real (the machine stops offering it) is judged by `availability()` on every judgement and does put the extension back to needing consent (D21).
- rejected: Rewriting `enforced_at_accept` on one failed run: a transient failure would silently move an extension towards running as the operator.
- blast:    `program::outcome`; the spec's wording is met by availability, not by the run.

## D28 — Programs are tested on the real kernel, from a checkout under Cargo's `target/tmp`

- status:   pending
- task:     5.3
- commit:
- trigger:  The checkout rule refuses the system temporary directory, where the test environments live, and the program tests must run real confinement.
- decided:  `tests/lifecycle/extension_programs.rs` installs a packaged `sh` probe from a staged marketplace, consents and enables it, and asks it through `ExtensionRunner::confining_through(Some(CARGO_BIN_EXE_uze))` over a checkout under `CARGO_TARGET_TMPDIR`. Nine tests: secrets absent, writes into `.git/hooks`, `README.md`, outside and `$UZE_HOME` denied, reads of a key outside and another `/proc/<pid>/environ` denied, a signal to the test process refused, `socket()` and a Unix socket refused, `setsid` refused, a hang timed out, stdout flood and scratch fill cut off, a helper exiting 125 running nothing. Each skips, saying why, where the machine does not confine. Running it found `commit_everything_in` committing into the surrounding checkout; fixed in the testkit.
- rejected: Mocking the kernel: the promise is about the kernel.
- blast:    One suite file.

## D29 — One store file per extension and project, under `state/extensions/store/<plugin@marketplace>/<id>/`

- status:   pending
- task:     6.1
- commit:
- trigger:  D18 fixes the scope (per identity and project, or global) and the bound, not the layout or the tier.
- decided:  A record (`uze_document::Shaped`, shape 1): `<project id>.json` (the short hash ADR-055 names projects by) or `global.json`, each `{schema_version, store}`, written atomically under a lock beside it, at most 1 MiB as written. Removing the plugin removes `<plugin@marketplace>/` whole, with its consents. An unreadable store reads as empty.
- rejected: Keys inside `state/extensions.json`: every patch would rewrite every extension's consent under one lock.
- blast:    `UzeHome::{extension_stores_dir, extension_plugin_stores_dir, extension_store_path}`, `uze-workspace::extensions::store`.

## D30 — A patch nobody may keep, or one over the bound, becomes the answer's notice

- status:   pending
- task:     6.2
- commit:
- trigger:  The spec drops a request without its grant "named in the surface's notice", and refuses an oversized patch, without saying where an answer's other members go.
- decided:  The runner applies `store` only from a fresh answer (never a remembered one), before the answer reaches the engine. An extension without `store`/`store:global` gets its notice extended with "it asked to keep something, and does not want `store`"; an oversized patch with the bound. `data`, `toast` and `refresh` still apply. A check (`--run`) keeps nothing.
- rejected: Failing the answer: a meter that cannot keep history can still draw its reading.
- blast:    `runner::keep_store`.

## D31 — A script's `run_program` arguments are taken as written, and its wait is not the script's time

- status:   pending
- task:     7.1
- commit:
- trigger:  A declaration's `{field}` is substituted; a script builds its own arguments, so substituting them again would let data become syntax. And the script's 100 ms deadline would end any real program.
- decided:  `run_program(name, args)` exists only when the extension holds some `exec:` grant, refuses a name it does not hold, runs through the same program runner (pinned path, environment, bounds, confinement) with the arguments verbatim, and returns `#{ code, stdout, stderr }` (`code` is `()` after a signal; a time-out, flood or unenforced confinement throws). The time spent in the call is subtracted from the script's deadline; the program has its own 10 s.
- rejected: A deadline that grows for the whole answer: a script could loop after the call for as long as the program took.
- blast:    `uze-script::{reach::ProgramReply, engine::register_host}`, `runner::programs_for`.

## D32 — `check --run` runs a program only where this machine confines, resolving its programs now

- status:   pending
- task:     7.2
- commit:
- trigger:  S12: a check runs what an author is writing without consent; a script is confined by the engine, a program only by the OS.
- decided:  With a confining helper, `rehearse` runs a `run` source with its `exec:` names resolved on the spot (nothing recorded) and as confined as an enabled run, keeping no store. Without one it withholds: "this machine does not confine programs, so a check runs none; its recorded answers stand in".
- rejected: `--checkout <path>` to run unconfined on purpose: the operator can enable the extension on that checkout instead, with the "runs as you" step.
- blast:    `runner::rehearse_program`.

## D33 — A program source that is handed the store is remembered like any other

- status:   pending
- task:     6.2
- commit:
- trigger:  The session cache keys an answer by its input; a source that also reads the store would answer from a stale store when asked again with the same input.
- decided:  Left as it is: a source that reads its store is a meter refreshed by `every` or an event, and both ask with `Refresh`, which runs again. Only a plain reopen within the session is served from memory.
- rejected: Keying by the store's digest: every patch would be a new question for every source of the extension.
- blast:    A store-reading source opened twice without a refresh shows its first answer.
