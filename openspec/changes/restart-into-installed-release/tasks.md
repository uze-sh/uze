## 1. Platform

- [ ] 1.1 `uze_platform::process::replace_self`: `exec` on Unix; on Windows a child on the same console, waited for with Ctrl+C ignored, its exit code returned

## 2. Client

- [ ] 2.1 `self_update::restart`: the receipt's binary, `workspace --resumed`
- [ ] 2.2 A hidden `--resumed` flag lands the client where it left off
- [ ] 2.3 `WorkspaceExit::Restart` and `ui::run` answering it; `main` replaces the process after the terminal is restored
- [ ] 2.4 The notice's detail row restarts (`WorkspaceHit::RestartIntoRelease`), the headline opens the notes; `Action::RestartUze` offered while a release is installed; click == key tests

## 3. Checks

- [ ] 3.1 `docs/versioning.md`; `openspec validate --strict`; fmt, clippy, targeted tests
