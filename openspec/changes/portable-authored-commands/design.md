## Context

`ShellCommand` (`uze-core/src/machine/shell.rs`) is the one type behind
every authored command: `agents.yaml`'s `setup` and `gate`
(`uze-workspace/src/worktree.rs`) and `hooks.json`'s handler `command`
(`uze-core/src/capability/hook.rs`). A `Line` is POSIX-only; a
`PerPlatform` pair carries a spelling per shell. Setup and gate run through
`run_shell_bounded` (`machine/subprocess.rs`): `spawn_tree` in its own
group (a Job Object on Windows), bounded output, a timeout that ends the
tree. Hooks run inside a generated wrapper with no `uze` on its path
(ADR-040).

The requirements this change modifies were introduced by `windows-support`
and are not yet in the main specs; this change is archived after it.

## Goals / Non-Goals

**Goals:**
- A plain `setup`/`gate` line carries real shell logic (sequences, pipes,
  redirections, `test`, file builtins) and means the same on every
  platform.
- What an author sees on their own machine is what every other platform
  does.
- No author loses anything: what runs today keeps a way to run.
- Proof, with pass conditions written before each run.

**Non-Goals:**
- Changing what a hook's string means. Hook logic belongs in a script, and
  the exec form already makes a script portable.
- Control flow (`if`, `for`, functions). A line that needs it keeps the
  POSIX-only meaning or uses the pair.
- Exposing the portable shell to people as a command of its own.

## Decisions

### D1. `deno_task_shell` is the portable shell

Chosen on 2026-10-21 over two alternatives, on what was measured that day.

| Engine | Covers | New crates in the default build | C on release targets | Windows |
|---|---|---|---|---|
| A subset compiled to PowerShell (UZE's own) | words, quotes, `$VAR`, `&&`/`\|\|`/`;` | 0 | no | to be built |
| **`deno_task_shell` 0.33.3** | the above, plus pipes, redirects, `$(...)`, globs, `~`, 19 builtins | 46 (11 new to the workspace) | no | in production (it is `deno task`) |
| `brush-core` 0.5 (+ builtins, + coreutils) | full bash | 79 / 124 / 308 | yes (`psm`; `blake3`) | "experimental" (v0.4.0) |

- **The compiled subset** was too narrow for real `setup` lines (no
  `test -f x || cp …`, no file builtins).
- **`brush`** compiles C, which the `dependencies` rule refuses on the
  release matrix, and is experimental on Windows.
- **Others** (`alba-shell`, `epsh`, `rush-sh`, `rust-bash`) fail the rule
  on age, adoption or `0.0.x`.

`deno_task_shell` builtins are interpreted by the shell itself, never
resolved by CMD or PowerShell. So the alias conflicts of a coreutils-on-
Windows install (`cp`, `echo`, `mkdir` shadowed by the host shell) cannot
happen.

### D2. The same shell on every platform, not only on Windows

A plain line the shell can read runs in it on Linux and macOS too. If Linux
kept `sh`, a line that passes on the author's machine could fail only on a
colleague's Windows machine. With one shell, the author sees the result
first.

A builtin shadows the system tool of the same name. Where it lacks a flag
the tool has (`head -1`, `cp -a`, `set -u`), the line is refused at parse
and keeps running in `sh` (decisions D1). No working line breaks on Linux
or macOS because of a builtin. The `posix:` key still forces `sh` for one
line.

### D3. A line the shell cannot read keeps its POSIX-only meaning

`deno_task_shell::parser::parse` decides. On error, the line is a `posix`
spelling exactly as today, and the error (construct, and caret position)
becomes the reason every report shows (D6). Nothing that runs today is
refused.

### D4. The shell runs in a `uze` child, under `spawn_tree`

`run_shell_bounded` gives a portable line to `uze shell -c <line>`, a hidden
entry. It starts it with `spawn_tree(Seat::OwnGroup)` exactly as it starts
`sh -c` today. The child builds a current-thread tokio runtime (the shell's
API is `!Send`), runs the parsed line with the inherited environment and
cwd, and exits with its status. Every process the shell starts is a
descendant of that child, so the existing timeout ends the whole tree on
both platforms. Output capture and its bound are unchanged.

Alternatives:
- **Run in-process on a thread.** The shell's children would escape
  `spawn_tree`, and a timeout would have to go through the shell's own
  `KillSignal`, a second process-control path.
- **Keep the tokio runtime out of the parent.** Done: only the child builds
  one.

The child's executable is resolved the way `uze terminal` resolves its
server (`current_exe`, else `uze` on `PATH` when the binary was replaced
while running). That resolution moves into one shared place instead of
being copied.

Placement:
- **The wrapper:** in `uze-core/src/machine/`, with `parse`, which returns
  the reason, and `run`, which returns the status.
- **The re-export:** `uze-application` re-exports it for `src/`.
- **The hidden command:** it lives in `src/cli/` and is classified in
  `command_performance.rs`.

### D5. Hooks: semantics unchanged, extensionless exec scripts

A hook string stays POSIX. The interpreter is never placed on a hook's
execution path. `launcher::launch` resolves a script named without an
extension to its `.exe`, else its `.ps1`, sibling on Windows. The scaffold
and the playground declare one exec-form entry naming `scripts/guard`.

### D6. One reason, every report; trust is the authored text

- **One reason.** The parse error is the one value `uze status`, placement
  and delivery render, in the design-system's vocabulary.
- **Trust identity.** `describe()` is unchanged for a `Line`, so trust
  identity is the authored text.
- **Approval on Windows.** It now shows portable lines; `approval.rs`
  filters by `here()`, which becomes `Some` for them.
- **`space_overview.rs`.** It renders through `describe()` instead of its
  own pair formatting.

### D6b. A known divergence is refused at parse, never run silently

Beyond the shell's own parse, the wrapper walks the parsed line and treats
every known silent divergence from POSIX sh as not portable, with a
reason:
- `${` outside single quotes;
- `echo` with `-n` or `-e` as its first word;
- an external command with `2>&1` piped into `cat` or `xargs`, until the
  hang is fixed.

Each entry names the E4 row that proves it diverges. An entry is removed
only in the change that makes that row pass, so the list can only shrink
with evidence. A divergence found later joins the list together with its
row.

### D7. Where `ShellCommand` changes

What a command *runs as* on a platform becomes a small enum:
- portable line;
- line for the platform shell;
- nothing, with a reason.

The workspace asks for it, and the hook compiler keeps asking for the
POSIX/PowerShell spelling. That is how one type serves two schemas whose
strings mean different things. No caller branches on platform.

## Findings from the probe (2026-10-21, Linux)

Verified:
- `&&`, `||`, `;`;
- `test`/`[`;
- `cp -r`, `rm -rf`, `mkdir -p`;
- `X=$(...) && echo $X`;
- `export`;
- globs, `~`, `<` and `>`;
- `2>&1` into a builtin that stops early (`head -n`);
- status 127 for a missing program.

Two gaps go into the spec and the tests:
- **`${NAME}` is not expanded** (prints `${X}`). The spec promises `$NAME`
  only, and E2 counts `${` in the corpus.
- **Hang:** an external program with `2>&1` piped into a builtin that reads
  to EOF (`cat`, `xargs`) never finishes (`sh -c "echo x" 2>&1 | cat`
  times out; `| /bin/cat` and `| head -n 1` do not). The redirected stderr
  most likely keeps the pipe's write end open. No upstream issue matches.
  It is refused at parse (D6b), reproduced in E4, and reported upstream as
  denoland/deno_task_shell#188.

`echo -n` prints `-n`, which is a silent divergence, refused at parse
(D6b).

## Evidence

Pass conditions are fixed before each run. Results are written into this
section; there is no separate report file.

- **E1 Coverage.** A corpus of at least 400 real one-line commands, each
  with source URL and commit, frozen in
  `tests/_fixtures/portable-commands/`:
  - GitHub Actions `run:` steps;
  - `package.json` scripts;
  - devcontainer `postCreateCommand`;
  - justfile lines;
  - the setup and gate lines in this repository's fixtures.

  Measure the share the shell parses, with a histogram of what it rejects.
  - **Baseline:** 0% of plain lines run on Windows today.
  - **Pass:** at least 80% parse. Every rejection keeps today's meaning, so
    a lower share limits the gain, never anyone's line.
- **E2 Divergence on Linux and macOS.** Over the corpus, count commands
  whose name is a builtin and whose flags the builtin does not accept, and
  lines using `${`. Each is a line whose behavior changes on POSIX.
  - **Pass:** at most 3% of the corpus.
  - **Above that:** stop and decide, before rollout, between narrowing D2
    (sh on POSIX, the portable shell on Windows) and accepting it.
- **E3 Linux/Windows equivalence.** A bounded-exhaustive generator (no new
  dependency) produces lines over the supported constructs with hostile
  words: spaces, quotes, `%`, `^`, `&` quoted, unicode, empty. They run
  against a testkit probe that records argv, environment and cwd and exits
  with a chosen code, on the Linux, macOS and `windows-2025` rows.
  - **Pass:** identical traces and statuses.
- **E4 Semantics matrix**, on every row:
  - success, failure and status 127;
  - a `.cmd` program (`pnpm.cmd`-like);
  - `cd` then a program;
  - an assignment scoped to one command;
  - pipeline status;
  - every builtin with the flags it claims;
  - the unsupported-flag error;
  - one row per D6b entry, proving the divergence while the entry stands
    (the hang row runs under a bounded wait and asserts the timeout, so it
    never hangs the suite).
- **E5 Process control:**
  - a gate that spawns a grandchild and outlives its timeout leaves no
    process behind, on Linux and Windows;
  - output past the bound is truncated as today;
  - an interrupt of the workspace ends the child.
- **E6 End to end on Windows CI:**
  - a project with `setup: test -f .env || cp .env.example .env` and a
    portable `gate` places and delivers;
  - a gate using `for` refuses delivery, naming `for`;
  - a scaffolded hook plugin passes `uze agent plugin check` and denies
    through the PowerShell wrapper.
- **E7 Cost:**
  - `cargo deny check` passes;
  - no target in `release.yml` compiles C;
  - record the release binary size delta per target;
  - record the start-up cost of `uze shell -c true` against `sh -c true`
    and `powershell -Command exit`.
  - **Pass:** start-up no slower than the PowerShell it replaces on Windows.
- **E8 Nothing taken away:** every existing `agents.yaml` and `hooks.json`
  fixture parses, and every pair and exec-form handler yields the same
  spelling as before.

## Test plan

Levels follow `tests/README.md`. CI runs the deterministic suites on the
Linux, macOS and `windows-2025` rows, so every test below runs on all three
unless it says otherwise. No test sleeps or retries (`tests-layout` rule):
anything waiting on a process waits on a signal it gives.

**L0, in the crate, every host:**
- **Parse acceptance.** A table of lines over every supported construct
  (sequence, pipe, each redirect, assignment, `$NAME`, `$(...)`, glob, `~`,
  every builtin) expected portable.
- **Parse refusal.** A table of lines over every refused construct (`if`,
  `for`, `while`, `case`, functions, `$$`, `$#`, multiple redirects, and
  each D6b divergence), each with its expected reason and position. A new
  refusal without a row fails the table's completeness test.
- **The "runs as here" matrix** (D7): plain line portable or not, pair with
  either half or both, for `Family::Posix` and `Family::PowerShell`.
  Exhaustive over the enum.
- **Hook spellings unchanged:** for every `hooks.json` fixture and each
  family, the hook compiler's spelling equals the one before this change
  (E8).
- **Trust:** `describe()` of a `Line` is its text; an approval keyed on a
  portable line is not invalidated by anything but the text.
- **Launcher, both columns:** an extensionless name with `.exe` and `.ps1`
  siblings, with only `.ps1`, with neither (Unplaceable, with a reason),
  and the POSIX column unchanged.
- **Corpus ratchets (E1, E2):** the parsed share and the divergence count
  against their recorded values.

**L1/L3, in `tests/`, through `CARGO_BIN_EXE_uze`:**
- **`tests/cli/portable_shell.rs`** drives `uze shell -c` directly:
  - E3: the generator against the testkit probe, compared with a
    reference trace computed from the generated line;
  - E4: the semantics matrix, one named test per row, so a failing row
    names itself;
  - E5: process control, where the probe starts a grandchild that blocks
    on a pipe the test holds; after the timeout, the test asserts both pids
    are gone through `uze_platform`'s liveness, and that the pipe's reader
    saw EOF.
- **`tests/acceptance/`:**
  - a project with a portable `setup` and `gate` places a checkout and
    delivers;
  - a gate using `for` refuses delivery naming `for`;
  - a `${`-using step is reported POSIX-only on every row and still runs
    in `sh` on Linux and macOS;
  - a `posix:`-only step runs in `sh` (the builtin-shadowing escape).
- **Scaffold:** a plugin scaffolded with `--hook` passes
  `uze agent plugin check`; on Windows its exec-form guard is delivered
  as the `.ps1`.

**Architecture (`tests/architecture/`):**
- Only the wrapper module names `deno_task_shell`.
- No tokio runtime is built on a path other than `uze shell`.
- `uze shell` is classified in `command_performance` (covered by the
  existing `every_cli_command_is_classified`).

**Not affected:**
- **The Lab:** hook semantics are unchanged on Linux, and the Lab runs
  Linux only.
- **Journeys:** a journey scene (portable setup and gate through the TUI)
  is written only after the operator has validated the behavior by hand.

### Results

- **E1 (2026-10-23).** Corpus of 1,224 commands from 60 repositories:
  - 1,163 portable (95.0%); the pass mark was 80%.
  - Refused: 29 `${NAME}`, 12 reserved words (`if`, `for`…), 9
    patterns that would match either case (decisions D6), 6 unsupported
    builtin flags, 2 `echo` flags, 2 `{ …; }` groups, and 1 multiple
    redirect.
  - The ratchet holds 950‰. It held 958‰ on 2026-10-21, before the
    walk reached inside `$(...)`: two lines ran `head -1` there, and one
    `xargs` line was refused for its command's flag. It held 957‰ until
    the caseless patterns were refused.
- **E2 (2026-10-21).** With builtin flags refused at parse (decisions D1),
  no portable line runs differently on Linux or macOS because of a
  builtin. The remaining dependence on which `sh` runs a line is brace
  alternatives and `echo` escapes: 2 of 1,224 (0.16%), both written for
  expansion (decisions D2). The pass mark was 3%, and the ratchet holds
  1‰.
- **E3 (2026-10-21, Linux row).** 164 generated lines (every structure of
  up to three commands over `&&`, `||`, `;` and `|`, success and failure,
  scoped assignments, and 13 hostile words dealt round-robin) delivered
  through `uze shell -c` exactly the argv, variables, cwd and status of
  the reference. The reference itself agrees with `/bin/sh` on all 164
  (`the_reference_agrees_with_sh`). The macOS and `windows-2025` rows run
  in CI.
- **E4 (2026-10-21, Linux row).** Every matrix row passes, including one row
  per divergence proving it still stands. The `.cmd` row is Windows-only
  and runs in CI.
- **E5 (2026-10-21, Linux row).**
  - A timed-out line whose probe started a grandchild ended both: each
    probe's connection to the test closed.
  - 512 KiB of output came back bounded and announced as truncated.
  - The hang is reported upstream as denoland/deno_task_shell#188.
- **E7 (2026-10-21, Linux x86_64 release).**
  - `cargo deny check` passes, and `cargo tree -i cc` is empty on all eight
    `release.yml` targets.
  - The release binary grows from 28,888,208 to 30,329,904 bytes (+1.44 MB,
    +5.0%).
  - Start-up, mean of 100 runs: `uze shell -c true` 3.1 ms against
    `sh -c true` 0.9 ms. With work, `mkdir -p d && cp -r d e && rm -rf d e`
    takes 4.5 ms against 5.5 ms in `sh`, since the builtins start no
    process.
  - The Windows rows (size per target, start-up against
    `powershell -Command exit`) are measured in CI.

## Risks / Trade-offs

- **Builtins shadow tools on POSIX** (`head -1`). → E2 measures it, the
  error is loud, and `posix:` is the escape.
- **Upstream bugs such as the `2>&1 | cat` hang.** → Refused at parse until
  fixed (D6b). The version is pinned, and every bump re-runs E3/E4: a D6b
  row that starts passing is the signal to drop its entry.
- **46 more crates and tokio in the default binary.** → E7 records the
  size. Tokio only runs in the child.
- **0.x dependency with breaking releases** (0.33 after 33 minors). → It is
  used through one module (D4), so a bump touches one file.
- **Two meanings of a string** (portable in `agents.yaml`, POSIX in
  `hooks.json`). → Each schema's docs say so. A hook's portable path is
  the exec form, which the scaffold teaches.

## Migration Plan

Manifests need no edit. Implementation may start now. Only this change's
archive waits for `windows-support`'s. Order:
1. E1/E2 measured on the corpus. If E2 fails, decide D2 before going on.
2. Wrapper, `uze shell -c`, `run_shell_bounded`.
3. E3–E5, then E6/E7.
4. Hooks (D5), docs.
5. Archive, after `windows-support`'s.

Rollback: `run_shell_bounded` stops routing portable lines to the child,
which is one branch, and the dependency is removed.

## Candidate ADRs

- **Project commands run in an embedded portable shell
  (`deno_task_shell`) on every platform; hook strings stay POSIX.** It is a
  new external dependency that fixes the command language authors write.
