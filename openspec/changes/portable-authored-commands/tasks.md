## 1. Corpus evidence first (E1, E2)

- [x] 1.1 Collect at least 400 one-line commands with source URL and commit (GitHub Actions `run:`, `package.json` scripts, devcontainer `postCreateCommand`, justfile lines, this repository's setup and gate lines); freeze them in `tests/_fixtures/portable-commands/corpus.tsv`
- [x] 1.2 Add `deno_task_shell` (pinned) to `uze-core`, answering the `dependencies` rule's checks in the PR: publisher, no C on any `release.yml` target, `cargo deny check`, `cargo tree` before; run `make attributions`
- [x] 1.3 Write the L0 parse tables first (design Test plan: acceptance table, refusal table with reason and position, completeness test over refusals), then the wrapper's `parse(line)` in `uze-core/src/machine/` until they pass
- [x] 1.4 D6b divergence list in the wrapper (`${`, `echo -n`/`-e`, external `2>&1` into `cat`/`xargs`), each entry naming its E4 row, covered by the refusal table
- [x] 1.5 Corpus ratchet tests: share parsed (E1) and divergence count (E2: builtin names with flags the builtin rejects); record the values
- [x] 1.6 Record E1/E2 in design.md's Evidence section; if E2 exceeds 3%, stop and bring the list back for the D2 decision before section 2

## 2. Running a portable line

- [x] 2.1 L0 matrix test for the "runs as here" answer (design D7), exhaustive over plain-portable, plain-not-portable and every pair shape × both families; then add it to `ShellCommand`
- [x] 2.2 L0 test that every `hooks.json` fixture's hook spelling is unchanged per family (E8); keep it green through the change
- [x] 2.3 Wrapper `run(line, cwd)` building a current-thread runtime and returning the status
- [x] 2.4 Hidden `uze shell -c <line>` entry in `src/cli/`, reaching the wrapper through `uze-application`, classified in `command_performance.rs`
- [x] 2.5 One shared resolution of UZE's own executable (`current_exe`, else `uze` on `PATH`), used by both `uze terminal` and `uze shell`
- [x] 2.6 `run_shell_bounded` starts `uze shell -c` under `spawn_tree(Seat::OwnGroup)` for a portable line and is unchanged otherwise
- [x] 2.7 Architecture tests: only the wrapper names `deno_task_shell`; no tokio runtime is built outside the `uze shell` path

## 3. Equivalence and control (E3, E4, E5), in `tests/cli/portable_shell.rs`

- [x] 3.1 Testkit probe program: records argv, environment and cwd to a file, exits with a requested code, and on request starts a grandchild that blocks on an inherited pipe
- [x] 3.2 Bounded-exhaustive line generator over the supported constructs and the hostile word set, with a reference trace computed from each generated line; no new dependency
- [x] 3.3 E3: every generated line through `uze shell -c` matches its reference trace and status on the Linux, macOS and `windows-2025` rows
- [x] 3.4 E4: one named test per matrix row (success, failure, 127, `.cmd` program, `cd`, scoped assignment, pipeline status, each builtin's claimed flags, unsupported-flag error) and one per D6b entry proving the divergence (the hang row under a bounded wait asserting the timeout)
- [x] 3.5 E5: a gate whose grandchild outlives the timeout leaves neither pid alive (checked through `uze_platform` liveness) and the held pipe sees EOF; output past the bound is truncated; an interrupt ends the child. No sleeps or retries
- [x] 3.6 With the operator's go-ahead, report the `2>&1 | cat` hang upstream with the E4 reproduction
- [x] 3.7 Record E3–E5 in design.md's Evidence section

## 4. Workspace surfaces

- [x] 4.1 `steps_not_spelled_here`, placement, delivery and `uze status` name a line that is not portable, with construct and position and the pair as the alternative (D6); L0 tests on the rendered reason
- [x] 4.2 Approval on Windows shows portable lines; L0 test that trust identity stays the authored text
- [x] 4.3 `src/ui/orchestrator/space_overview.rs` renders commands through `describe()`; existing overview tests updated, not removed
- [x] 4.4 Acceptance tests in `tests/acceptance/`: portable setup and gate place and deliver; a `for` gate refuses delivery naming `for`; a `${` step is reported POSIX-only and still runs in `sh` on Linux and macOS; a `posix:`-only step runs in `sh`
- [x] 4.5 Collapse suite fixtures that spell the same line twice where the line is portable

## 5. Hooks (D5)

- [x] 5.1 L0 launcher tests for both columns first (extensionless with `.exe` and `.ps1`, only `.ps1`, neither, POSIX unchanged), then `launcher::launch`
- [x] 5.2 The `--hook` scaffold (`crates/uze-core/src/package/authoring/hooks.json`) and `playground/plugin/hooks.json` declare one exec-form entry naming the extensionless script; a test scaffolds it and runs `uze agent plugin check` on every row

## 6. End to end and cost (E6, E7)

- [ ] 6.1 E6 on the Windows CI row: the section 4.4 and 5.2 acceptance tests pass there; a scaffolded hook plugin denies through the PowerShell wrapper
- [ ] 6.2 E7: release binary size delta per target, start-up of `uze shell -c true` against `sh -c true` and `powershell -Command exit`, no C on any target
- [ ] 6.3 Record E6–E8 in design.md's Evidence section

## 7. Documentation

- [x] 7.1 Worktree policy docs: plain lines run in the portable shell; its constructs, builtins and their flags; what is refused and why (D6b); the pair as the escape hatch
- [x] 7.2 `docs/capabilities/portable-hooks.md` and the `uze:author` Skill: exec form with extensionless names first, the pair second; a hook string stays POSIX
- [x] 7.3 Run the `design-system` checklist on every changed report and help text
- [x] 7.4 `make check` passes, including `openspec validate --all --strict`

## 8. After the operator validates by hand

- [ ] 8.1 Journey scene: portable setup and gate through the TUI, checked against the checkout and the target branch
- [ ] 8.2 Archive, after `windows-support` is archived (its requirements are the ones this change modifies)
