## 1. HOOKCWD-1 - measure every source

- [x] 1.1 Lab measurement per harness of each candidate (payload, hook process directory and environment, plugin context), and two simultaneous Codex sessions (`conformance/experiments/codex/hook-daemon.py`)

## 2. HOOKCWD-1 - the handler context

- [x] 2.1 Regression tests: `crates/uze-integrations/src/hooks/wrapper_tests.rs::a_shell_call_is_told_where_it_runs_beside_the_session_s_workspace`, `crates/uze-integrations/src/hooks/opencode_runtime_tests.rs::a_shell_call_is_told_where_it_runs_beside_the_location_s_workspace`, failing on the audited commit
- [x] 2.2 Fix: `Workspace` and `workdir` in `PayloadPaths`, the POSIX and PowerShell wrappers, the three dialects, the OpenCode bridge; goldens regenerated

## 3. HOOKCWD-1 - the rules engine

- [x] 3.1 Regression test: `crates/uze-integrations/src/rules/engine_tests.rs::the_checkout_is_the_workspace_the_harness_names_whatever_directory_a_command_ran_in`, failing on the audited commit
- [x] 3.2 Fix: the engine's workspace argument and launch directory, `RulesDelivery::Entries.workspace`, the OpenCode plugin's spawn directory

## 4. HOOKCWD-1 - the Lab holds it

- [x] 4.1 `conformance/contract/hook_directories.py` in the hooks contract, with `hooks.shell-workdir-parameter` (Antigravity) and `hooks.post-workdir-is-start` (Claude) declared and registered
- [x] 4.2 `docs/capabilities/portable-hooks.md`, ADR-061

## 5. Proof

- [x] 5.1 Gates: fmt, clippy, `cargo test -p uze-integrations`, the integrations and projection suites, the Lab's Python tests and ruff, `openspec validate --all --strict`
- [x] 5.2 Exploit re-run: the hooks contract on all four harnesses; evidence.md
