Lab: the Harness Conformance Lab (`conformance/`), real harness binaries
against the synthetic provider, image `15660b84822ef1b5` built from this
branch (claude 2.1.314, codex-cli 0.166.0, antigravity 1.4.12, opencode
v2.2.5). The audited wrapper and engine are the goldens of the audited
commit `9ad6db96`, run beside this branch's.

## HOOKCWD-1

- **Fix**: `HOOK_CWD` is read per harness from the session
  (`Workspace` in each hook dialect, the OpenCode bridge's location), and a
  shell call's directory is `HOOK_WORKDIR`; the rules engine finds its
  checkout from the workspace and reads relative paths against the
  command's directory (`crates/uze-integrations/src/hooks/wrapper*`,
  `*/hooks.rs`, `hooks/bridge.rs`, `rules/engine.rs`, `rules/plugin.rs`).
- **Regression test**:
  `crates/uze-integrations/src/hooks/wrapper_tests.rs::a_shell_call_is_told_where_it_runs_beside_the_session_s_workspace`,
  `crates/uze-integrations/src/hooks/opencode_runtime_tests.rs::a_shell_call_is_told_where_it_runs_beside_the_location_s_workspace`,
  `crates/uze-integrations/src/rules/engine_tests.rs::the_checkout_is_the_workspace_the_harness_names_whatever_directory_a_command_ran_in`
  (`cargo test -p uze-integrations`); against the audited sources they do
  not hold (the audited wrapper exports no `HOOK_WORKDIR` and takes
  `HOOK_CWD` from the call, as the re-run below shows).
- **Exploit re-run**: a shell call whose payload names `/elsewhere`,
  launched in the workspace `$LAB/ws`, with a denying handler that prints
  both values:
  ```
  audited codex:  HOOK_CWD=/elsewhere HOOK_WORKDIR=unset
  audited claude: HOOK_CWD=/elsewhere HOOK_WORKDIR=unset
  fixed codex:    HOOK_CWD=$LAB/ws HOOK_WORKDIR=/elsewhere
  fixed claude:   HOOK_CWD=$LAB/ws HOOK_WORKDIR=/elsewhere
  ```
  The rules engine started in checkout `ws2`, for a `PostToolUse` whose
  `cwd` is checkout `other2` and whose command reads `src/a.ts`:
  ```
  audited: {"hookSpecificOutput":{…"additionalContext":"<rule path=\".agents/rules/src.md\" … matched=\"src/a.ts\">\nRULE-OF-other2\n</rule>"}}
  fixed:   <nothing>
  ```
  The hooks contract on each harness (`python3 conformance/lab.py
  --harness <h> --contract hooks`), every `hooks-directories-*` check
  green, `HOOK_CWD` `/work/project` throughout:
  ```
  codex       55/55  other: started in /work/elsewhere, HOOK_CWD ['/work/project'], HOOK_WORKDIR ['/work/elsewhere']
  claude      66/66  after-cd-sub: started in /work/project/sub, HOOK_CWD ['/work/project'], HOOK_WORKDIR ['/work/project/sub']
                     [DECLARED] hooks-directories-post-is-start: cd-sub: started in /work/project, HOOK_WORKDIR ['/work/project/sub']
  antigravity 67/67  [DECLARED] hooks-directories-named (`Cwd` refused before any hook)
  opencode    65/65  sub-rel: started in /work/project/sub, HOOK_CWD ['/work/project'], HOOK_WORKDIR ['/work/project/sub']
  ```
  Codex's source, two live sessions under one app-server started from
  `/work/project` (`conformance/experiments/codex/hook-daemon.py`): every
  hook of the `/work/project2` session ran in `/work/project2`, matching
  its transcript's `session_meta.payload.cwd`.
- **Residual**: none for the workspace. On Claude Code a `post_tool_use`
  handler's `HOOK_WORKDIR` is where the shell was left (declared, measured
  every run); a `cd` inside one command line is reported by no harness.

## Gate

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo test -p uze-integrations`: 455 passed, 0 failed, 3 ignored (the
  regenerators).
- `cargo test --test integrations --test projection`: 120 and 31 passed.
- `for t in conformance/tests/test_*.py; do python3 "$t"; done`, `ruff
  check`, `ruff format --check` (in `conformance/`): clean.
- `openspec validate --all --strict`: 93 passed.
- `python3 conformance/lab.py --harness {codex,claude,antigravity,opencode}
  --contract hooks`: 55/55, 66/66, 67/67, 65/65.
