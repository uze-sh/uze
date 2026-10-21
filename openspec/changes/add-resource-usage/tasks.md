## 1. Platform

- [x] 1.1 `uze_platform::usage`: `Machine`, `machine()`, `Reading`, `Memory`, `of(pid)`, one implementation per platform as D6 says
- [x] 1.2 `pane::census(variable)` and `Census::of_pane(leader, group)` / `Census::tree_of(pid)` per D1; `pane::Group::cpu_time()` from the job's accounting on Windows, `None` on Unix
- [x] 1.3 Add `Win32_System_ProcessStatus` to `uze-platform`'s windows-sys features
- [x] 1.4 Tests: a busy child raises CPU, a reaped child shows in its parent's children time, an allocation raises memory, an ended pid reads `None`, a process in its own group under the leader and a detached marked process are in the pane's census

## 2. Terminal runtime

- [x] 2.1 `ClientRequest::Usage { version }`, `ClientEvent::Usage(UsageReport)` with `started`, `window` and the four `Load` totals; `PROTOCOL_VERSION` 23
- [x] 2.2 Record the server's start; record each attached client's pid
- [x] 2.3 Sampling per D2, coalesced; `Usage` admitted from a pane; refusal message, `OpenSpace`/`open_space` docs and `invariants.md` updated
- [x] 2.4 `uze_terminal::usage()` beside `stop()`: running, not running, another build
- [x] 2.5 Tests: tab classification into totals; the answer carries no label or launch variable; a pane process admitted for usage and refused `Attach`; concurrent requests share a sample

## 3. Application

- [x] 3.1 Move the checkout walk into a helper the checkout list and the disk view share
- [x] 3.2 `Workspace::disk()` per D5 with tests: nested checkouts once, a missing project skipped, every tier listed

## 4. CLI

- [x] 4.1 `Command::Usage { format }` (`uze usage`) with help text and a row in the root help
- [x] 4.2 Sample and walk concurrently with a spinner; render per D9; JSON per the spec
- [x] 4.3 Classify `usage` as `JustifiedSlow`
- [x] 4.4 CLI tests for the text and JSON with no workspace running; design-system checklist

## 5. Architecture and gate

- [x] 5.1 `docs/architecture/containers.mmd`: the CLI's relation to the terminal server; `make artifacts`, `cargo test -p uze-extensions`
- [x] 5.2 `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace --no-fail-fast`, `openspec validate --all --strict`
- [x] 5.3 `cargo check --target x86_64-pc-windows-msvc` and `--target aarch64-apple-darwin` for `uze-platform` and `uze-terminal`
