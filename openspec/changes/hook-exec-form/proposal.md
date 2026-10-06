## Why

A hook handler is one shell line today, or a `posix`/`windows` pair of them. An author who writes a Python guard writes it twice, once per shell, and a guard with no Windows line refuses the whole package there. The line also carries `${PLUGIN_ROOT}` pasted in unquoted, so a home directory with a space in it (`C:\Users\First Last`) breaks every handler that names its own script. Every other place UZE already names a program, `mcp.json` among them, says it as `command` plus an `args` list; a hook handler is the one place that cannot.

## What Changes

- **Exec form.** A handler MAY give `args` (a list, possibly empty) beside `command`. With `args` present, `command` is a path to a script or executable inside the package, relative to its root, and `args` are passed as words, never re-read by a shell. Without `args`, `command` keeps its meaning: a shell line, or a `posix`/`windows` pair.
- **The launcher is UZE's to choose, per platform.** On POSIX an executable file runs itself (its shebang decides); otherwise, and on Windows, the file's extension picks the interpreter from a small fixed table (`.py`, `.js`/`.mjs`/`.cjs`, `.sh`, `.ps1`). An optional `interpreter` (a list of words) overrides the table. A script the table cannot place, and that is not executable, is reported and not delivered.
- **Windows Python is probed, not assumed.** The Python a Windows machine carries depends on how it was installed (`py`, `python`, or the Store's `python3`, with zero-byte aliases that only open the Store): the launcher runs each candidate and takes the first that answers, and a stub counts as absent.
- **The harness's own runtime where it has one.** OpenCode runs a plugin in its embedded Bun, so a `.js`/`.ts` handler runs there and needs nothing from the machine.
- **The interpreter becomes a requirement.** Whatever the launcher needs from the machine joins the package's effective requirements (`plugin-requirements`), attributed to the hook that needs it.
- **Quoting is UZE's.** UZE renders the launcher and the words into the line each wrapper runs, quoted for that shell; the author never quotes.

Out of scope: a dedicated hook-runner binary replacing the generated wrappers (deferred; see design), installing any interpreter, and language-package dependencies.

## Capabilities

### Modified Capabilities

- `portable-hooks`: a handler's exec form, its launcher rules per platform, and the interpreter it adds to the package's requirements.

## Impact

- `crates/uze-core`: `CommandHook` gains `args` and `interpreter`; the launcher table and the Windows Python probe in `machine`; manifest validation; trust describes the exec form; authoring check warns on a script with no launcher.
- `crates/uze-integrations`: the wrappers and the OpenCode bridge run the rendered invocation; the plan reports a handler with no launcher; hooks contribute their interpreters as requirements.
- Docs: `web/content/docs/reference/plugin-format.mdx` (the exec form, the `posix`/`windows` pair), `docs/capabilities/portable-hooks.md`.
