## 1. Model

- [x] 1.1 `CommandHook` gains `args: Option<Vec<String>>` and `interpreter: Option<Vec<String>>`; validation rejects `args` beside a `posix`/`windows` pair, an absolute or escaping script path, and `interpreter` without `args`
- [x] 1.2 The launcher in `uze-core::machine`: per-platform extension table, the POSIX execute-bit rule, the `interpreter` override, the Windows Python probe with stubs treated as absent; pure over the platform family and an injected probe, so both tables are tested on every host
- [x] 1.3 `CommandHook::invocation` (a shell line, or an argv with the executable it needs, or why it cannot run here); `unspelled_here` and trust's description read it

## 2. Delivery

- [x] 2.1 The `sh` and PowerShell wrappers run the rendered, quoted line for an exec-form handler
- [x] 2.2 The OpenCode bridge spawns an exec-form handler from its argv, and a JavaScript one in its own Bun (`process.execPath`, `BUN_BE_BUN=1`)
- [x] 2.3 A handler with no launcher here is reported Unsupported with the reason, under the same fail-closed rule as an unspelled one
- [ ] 2.4 Hooks contribute their interpreters to the package's effective requirements (`plugin-requirements`)

## 3. Authoring, docs, proof

- [x] 3.1 `uze agent plugin check` warns on an exec-form script with no launcher on some platform, and on a POSIX script that is neither executable nor placeable by its extension
- [x] 3.2 Tests: launcher tables per family, quoting with spaces, quotes and `$`, the bridge's argv and Bun paths, the plan's Unsupported reason
- [ ] 3.3 Docs: `plugin-format.mdx` describes the exec form and the `posix`/`windows` pair; `portable-hooks.md` drops the stale Windows limitation and states the exec form
- [ ] 3.4 Lab: `hook-exec`, a deny group whose guard is a non-executable `.js` named in exec form with a word full of quotes, `$` and backticks, proves on every harness that the launcher starts it (`node`, or OpenCode's own Bun), the word arrives intact and the denied tool never runs
