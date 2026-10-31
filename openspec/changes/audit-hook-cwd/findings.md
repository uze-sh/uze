Found by: harness-watch survey and Lab measurement · Date: 2026-10-30 · Measured on: codex-cli 0.166.0, uze 1.0.0-beta.21 (9ad6db96)

One root cause: UZE's Codex hook wrapper exports the hook input's `cwd` as `HOOK_CWD` and treats it as the workspace, but Codex 0.166.0 (openai/codex#57198, #57203) reports there the directory the command runs in, resolved from the model-chosen `workdir` of the call. The portable contract (`docs/capabilities/portable-hooks.md`) promises `HOOK_CWD` is the workspace directory.

## HOOKCWD-1 - A model-chosen workdir becomes the workspace a portable hook handler and the rules engine trust

- **Severity**: Medium. A third-party portable hook handler that follows the documented contract and confines by the workspace (e.g. denies writes outside `$HOOK_CWD`, or denies a path relative to the workspace) can be steered off it; the rules engine either delivers no rules or delivers another repository's rules as session context.
- **Status**: CONFIRMED in the Lab (experiment codex/hook-cwd, kept outside the repository).
- **Threat actor**: a prompt-injected agent choosing a tool call's `workdir`.
- **Preconditions**: Codex 0.166.0 or later; a shell call with a `workdir` other than the session directory.
- **Location**:
  - `crates/uze-integrations/src/codex/hooks.rs:36` (`cwd: ".cwd // empty"`).
  - `crates/uze-integrations/src/hooks/wrapper/posix.rs:150`, `crates/uze-integrations/src/hooks/wrapper/powershell.rs:159` (`HOOK_CWD`).
  - `crates/uze-integrations/src/rules/engine.rs:306-313`, `:395-406` (repository root and path normalization from `cwd`).
- **Attack**: the model issues `exec_command` with `workdir` set to `/` or to another Git checkout. The hook receives `cwd` = that directory; `HOOK_CWD` carries it; a workspace-confining handler evaluates against it; the rules engine resolves the repository root from it.
- **Evidence** (session cwd `/work/project`):
  - no workdir: payload `cwd` and `HOOK_CWD` = `/work/project`;
  - `workdir=/work/project/sub` or `sub`: both = `/work/project/sub`;
  - `workdir=/work/elsewhere`: both = `/work/elsewhere`;
  - the payload carries no field holding the session directory; `tool_input` holds only `command`.
- **Root cause**: `HOOK_CWD` is read from a vendor field whose meaning moved from the session directory to the call's execution directory, and UZE has no separate, harness-anchored value for the workspace.


## Defenses that held

- Antigravity's workspace (`workspacePaths[0]`) was never read from the call, and `run_command` refuses a `Cwd` before any hook runs (1.4.12).
- Claude Code refuses to leave the project with `cd`: its shell is reset to the project directory (2.1.314).
- The rules engine only ever adds context; a wrong checkout cost rules, never a decision.
