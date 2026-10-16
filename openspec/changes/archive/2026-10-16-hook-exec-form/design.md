## Context

The portable hook contract (`HOOK_*` in, exit code out, a JSON object on stdout for `transform`) is compiled into a generated wrapper per harness (ADR-040): `sh` plus `jq` on POSIX, Windows PowerShell 5.1 on Windows, and a plugin module on OpenCode's embedded Bun. Each runs a handler as one line in its shell (`sh -c`, a `.ps1` file, `Bun.spawn([...SHELL, line])`). Nothing here changes that: the exec form is a different way to write the line, not a different way to run it.

Two independent reviews of the idea (one against this repository, one against how pre-commit, lefthook, husky, GitHub Actions, npm and the four harnesses solved it) shaped the decisions below.

## Decisions

### D1 — `command` + `args` is the exec form; no new word
`mcp.json` already says a program as `command` plus `args`, and Claude Code's own hook entry calls that the exec form. A hook handler with `args` (even `[]`) means the same; without it, `command` stays a shell line. Alternatives: a new `run` field (a third word for the same thing). The exec form takes a single string `command`: a `posix`/`windows` pair beside `args` is rejected, since choosing per platform is the launcher's job.

### D2 — The launcher is chosen when the hook is delivered, from the platform and the file
On POSIX, a file with an execute bit runs itself, so its shebang (`#!/usr/bin/env -S uv run --script`, a venv's python) decides; this is checked before the extension. Otherwise the extension picks: `.py` → `python3`, `.js`/`.mjs`/`.cjs` → `node`, `.sh` → `sh`, `.ps1` → `pwsh`. On Windows: `.py` → the probed Python (D3), `.js`/`.mjs`/`.cjs` → `node`, `.ps1` → `powershell.exe -NoProfile -ExecutionPolicy Bypass -File`, `.exe` runs itself; `.sh` has no Windows launcher and is treated as an unspelled handler. `interpreter` (a list of words) replaces the table. Alternatives: Windows file associations or `PATHEXT` (machine-dependent, and a `.cmd` re-parses its arguments in `cmd.exe`); choosing between `pwsh` and `powershell.exe` by what is installed (5.1 and 7 differ in encoding and operators, so behaviour would follow the machine).

### D3 — Windows Python is probed and stubs are absent
Candidates in order: `py -3`, `python`, `python3`. Each is asked `--version` with a short deadline; the first that exits 0 and names Python 3 is the launcher. The `python`/`python3` App Execution Aliases a clean Windows carries pass a `PATH` lookup and then exit 9009 or open the Store, so only a candidate that answers counts. With none, the requirement is `python`, reported unmet, and the line names `python` so it works once the person installs one. `py` is deprecated from Python 3.14, which is why it is a candidate rather than the answer.

### D4 — OpenCode runs JavaScript in its own Bun
OpenCode is a single executable built by `bun build --compile` (measured on 2.0.24: `BUN_BE_BUN=1 opencode --version` prints the embedded Bun 1.4.2). Its bridge runs a `.js`/`.mjs`/`.cjs`/`.ts` handler as `[process.execPath, script, ...args]` with `BUN_BE_BUN=1`, and adds no requirement. Every other handler on OpenCode is spawned from its argv directly, with no shell between.

### D5 — UZE quotes; the author never does
The wrappers still run one line, so the launcher and the words are rendered into it with the platform's own quoting (`uze_platform::shell::command_line`): `'python3' '/root/hooks/guard.py' '--strict'` for `sh`, `& 'py' '-3' 'C:\root\hooks\guard.py' '--strict'` for PowerShell. The script path is absolute, resolved against the delivered package root, so no placeholder is involved.

### D6 — The interpreter is a requirement, and a guard without one says so loudly
The executable a launcher needs (`python3`, `python`, `node`, `pwsh`) joins the package's effective requirements, attributed to the hook (`plugin-requirements`). `sh` on POSIX and `powershell.exe` on Windows are always there and add nothing. A fail-closed group whose interpreter is missing denies every call it matches, which on a `shell` matcher stops the agent; that is safe, and consistent with the wrapper's rule for a missing `jq`, but the report names the consequence rather than listing a quiet unmet row.

### D7 — No hook-runner binary, for now
Replacing the generated wrappers with one small Rust runner would remove `jq` and give one implementation of the contract, and both reviews weighed it. It is deferred: OpenCode would keep its bridge anyway, a copied binary meets macOS quarantine and Windows application control (Smart App Control already blocks the host in this repository's own Windows testing), a running `.exe` cannot be replaced in place, and the wrappers already live under `$UZE_HOME`, so the runner would not survive an uninstall any better. It is reopened with measurements, Windows cold-start latency first.

## Risks / Trade-offs

- [A probe at delivery goes stale] → the line names a launcher word, not a path, so `PATH` decides at run time; `uze install` and `uze doctor` re-probe.
- [The harness's `PATH` is not the installing shell's] → named, not solved: a GUI-launched harness can miss an interpreter a terminal has. The requirement report says the check ran against the installing shell.
- [Older UZE builds reject the new fields] → `CommandHook` denies unknown fields, so an older build refuses a manifest using `args` loudly; acceptable before 1.0 and stated in the plugin-format docs.
