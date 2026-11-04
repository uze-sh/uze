Audited commit: 7b8138a6 · Date: 2026-11-04 · Found by: `mutation-audit`, then reading the code

## SCRIPT-3 - Padding reserves by the padding's length, and the failure aborts the process

- **Severity**: Medium (availability). No grant is needed, but the script has to run.
- **Status**: CONFIRMED through the crate's API, in a debug build on Linux with
  `overcommit_memory=0`. Not reproduced through the real runner, on Windows or
  on macOS. The consent path was not traced.
- **Threat actor**: a malicious plugin author whose extension runs.
- **Location**: `crates/uze-script/src/primitives.rs`, `pad`. It checks the
  requested length in characters against the string bound, then reserves
  `(len - have) * padding.len()` bytes, although it writes `len - have`
  characters. Both factors can reach the 1 MiB bound.
- **Proof**:
  ```
  let p="x"; while p.len < 512*1024 { p += p; } let t="ab"; t.pad(1048576, p); #{data:t.len}
  ```
  ```
  memory allocation of 549754765314 bytes failed
  rc=134
  ```
- **Impact**: a failed allocation calls `handle_alloc_error`, which aborts and
  cannot be caught. `Script::call` runs on a thread of the host process, so the
  abort should take down the `uze` process hosting the extension. Where a huge
  allocation succeeds, an untouched reservation and no abort are expected;
  this was not tested.
- **Related**: same class as SCRIPT-2 in `audit-bounded-extensions`, whose table
  judged `pad(len, str)` only on the empty-padding hang.
