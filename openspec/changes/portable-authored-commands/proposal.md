## Why

A project's `setup` steps and `gate` in `agents.yaml` are, today, one POSIX
line or a `posix`/`windows` pair. A plain line never runs on Windows, so
`gate: cargo test` refuses every delivery there until the author writes it
again under `windows:`, and a step like
`test -f .env || cp .env.example .env` has to be rewritten in PowerShell.
Windows support (the `windows-support` change) made the gap visible. Hooks
show the same pattern: the scaffold and the playground declare a pair only
to point at `guard` and `guard.ps1`.

## What Changes

- **A plain `setup` or `gate` line runs in one shell on every platform**:
  a portable shell built into UZE. It offers sequences (`&&`, `||`, `;`),
  pipes, redirections, `VAR=value cmd`, `$VAR`, `$(...)`, globs, and
  builtins that behave the same on Linux, macOS and Windows (`cp`, `mv`,
  `rm`, `mkdir`, `cat`, `echo`, `test`, `cd`, `export`, `exit` and
  others). The shell runs in a `uze` child process, so timeouts, process
  trees and bounded output work as today.
- **On Linux and macOS** a plain line that used to run in `sh` now runs in
  the portable shell. A line using something the portable shell lacks or
  runs differently (a builtin flag it does not implement, `${NAME}`, `[`)
  keeps running in `sh`. The `posix:` key forces `sh` for any line.
- **A line the portable shell cannot read** (`if`, `for`, functions) keeps
  its POSIX-only meaning, never refused. `uze status` names it, the
  construct and its position, and what to write instead.
- **The `posix`/`windows` pair stays** as the escape hatch.
- **Hooks keep their semantics** (a string is POSIX, logic lives in an
  exec-form script). An exec-form script named without an extension
  resolves to its `.exe` or `.ps1` sibling on Windows, so the scaffold
  declares one entry instead of a pair.
- **Evidence before rollout**, with pass conditions written down first:
  - coverage of a corpus of real commands;
  - an audit of builtin-flag divergence on Linux;
  - Linux/Windows equivalence;
  - process-tree control;
  - cost (crates, binary size, start-up).

## Capabilities

### New Capabilities
- `portable-commands`: the shell a project's plain `setup`/`gate` line runs
  in on every platform, its builtins, how a line it cannot read is
  reported, how its processes are controlled, and what trust identifies.

### Modified Capabilities
- `worktree-policy`: a string `setup` step or `gate` runs in the portable
  shell where it can be read, and as the `posix` spelling otherwise.
- `portable-hooks`: an exec-form script without an extension resolves per
  platform, and the scaffold produces one entry naming it.

## Impact

- `uze-core`: a new `machine` module wrapping `deno_task_shell` (parse
  check and run); `machine/subprocess.rs` (`run_shell_bounded` starts the
  `uze` child for a portable line); `machine/launcher.rs` (extensionless
  scripts); `package/authoring` (scaffold template).
- `uze-workspace`: `worktree.rs` (`steps_not_spelled_here` names lines the
  shell cannot read); `approval.rs` (Windows shows portable lines).
- `uze-application` and `src/`: the hidden `uze shell -c` entry, its
  `command_performance` classification, status and placement reports;
  `space_overview.rs` renders through `describe()`.
- **New dependency:** `deno_task_shell` (denoland, MIT). It brings 46 crates
  into the default build, 11 of them new to the workspace, `tokio` among
  the rest. It compiles no C. It follows the `dependencies` rule's checks.
- Docs: worktree policy docs, `docs/capabilities/portable-hooks.md`, the
  `uze:author` Skill, `playground/plugin`.
- Archived after `windows-support`, whose requirements it modifies.
