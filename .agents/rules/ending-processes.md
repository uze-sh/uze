---
trigger: model_decision
description: Read before writing code that signals, kills or ends a process or a process group, in Rust or in a shell script.
---

A process group is ended with `kill(2)` directly (`Tree::end` in
`crates/uze-platform/src/process/unix.rs`), never by shelling out to
`kill` with a negative pid. procps-ng `kill` parses a negative pid by its
first digit only, so any child pid starting with `1` became
`kill(-1, SIGKILL)`: every process the user owns. Until 2026-09-01 that
killed the whole WSL login session mid-test (`[process exited with code
9]`). Rust code signals through `libc::kill`; a shell script that truly
needs a group kill writes `kill -- -<pid>`.

A session that still dies mid-run shows it in `journalctl` as a kill by
signal (`user@1000.service: ... status=9/KILL` at the same second as
`Session N logged out`), not as memory pressure.
