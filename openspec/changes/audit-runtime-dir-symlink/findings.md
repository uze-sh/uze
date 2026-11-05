Audited commit: 6f32ffe0 · Date: 2026-11-05 · Found by: `mutation-audit`, while triaging a mutation audit of `uze-platform`, then reading the code · Advisory: GHSA-fxhj-rcfq-8qh6

## PLAT-1 - A symlink planted in a shared temp directory can narrow the mode of a directory the victim owns

- **Severity**: Low. Narrowing only: no disclosure and no write. The chmod
  succeeds only on the victim's own directories. A shared or served directory
  stops being reachable by others.
- **Status**: CONFIRMED by a probe as one uid. Not run with a real second account.
- **Threat actor**: another local user of the machine.
- **Preconditions**: `XDG_RUNTIME_DIR` unset or unusable, the ordinary case on
  WSL, containers and CI.
- **Location**: `crates/uze-platform/src/endpoint/unix.rs`, `usable`, and the
  same order at line 62 of that file; `crates/uze-platform/src/fs.rs`,
  `create_private_dir`.
- **Attack**: another local user plants a predictable name such as
  `/tmp/<namespace>-runtime-<uid>` or `/tmp/uze-ssh-<uid>` as a link to a
  directory the victim owns. The victim's next run strips that directory's group
  and other bits, then refuses the link.
- **Evidence**: `create_private_dir_all(link)` returned `Ok` and the target went
  from 755 to 700. `user_socket_directory` with the planted name returned `None`,
  and the target was still narrowed to 700.
- **Root cause**: `usable` calls `fs::create_private_dir_all(candidate)` first and
  only then `private_directory`, which refuses a symlink. `create_private_dir`
  uses `is_dir`, `metadata` and `set_permissions`, and all three follow symlinks.
  `private_directory`'s own doc warns about this, but by then the chmod has
  happened.

## Defenses that held

- `private_directory` refused the link, so no socket was ever placed through it.
- `unpredictable_directory` uses a plain `mkdir`, which adopts nothing already there.
