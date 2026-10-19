---
trigger: model_decision
description: Read before adding, moving or changing the shape of anything UZE persists, under $UZE_HOME or inside a repository's .git.
---

Everything under `$UZE_HOME` belongs to exactly one tier, decided by **what
deleting it costs**, never by which module wrote it:

| tier | where | deleting it costs |
|---|---|---|
| bytes | `store/` | the packages, until they are acquired again |
| record | `state/` | the operator: nothing else knows it |
| authored | the root: `config.toml`, `keys.json`, `theme-overrides.json`, `themes/` | the operator's own text |
| generated | `runtime/`, `shims/` | nothing: it is produced again |
| remembered | `cache/` | nothing: it is observed again |

A thing must not sit in a tier that claims a different cost than it has.
Generated harness content once lived at `state/attachments/`, one letter
from `attachments.json`, the ledger of who owns what inside it: the answer
to "can I delete this" was opposite for each.

**Only records declare a shape.** Generated and remembered things are
produced or observed again when they cannot be read. Authored files carry
no shape either: they grow by additive keys, an unknown value reads as the
default, and one that does not parse is reported and never written over.
A new document declares its tier by where `UzeHome` puts it and, if it is a
record, its shape and ladder by implementing `uze_document::Shaped`.
**Every path UZE owns is named in `UzeHome`**:
`every_path_uze_owns_is_named_in_the_map` in
`tests/architecture/layering.rs` fails the build over a path composed
anywhere else. `uze-terminal` is the one sanctioned exception (the
`terminal-runtime` rule).

Inside a repository's Git directory, named in the workspace's worktree
module:

- the `/.worktrees/` line in `.git/info/exclude` (the operator's file, of
  which UZE owns that one line);
- `.git/worktrees/<admin>/uze-checkout.json`, saying UZE made that
  checkout: a *record*, deleted by Git with the worktree it describes;
- `refs/uze/shelf/<task>`, the unfinished work an agent left when it ended,
  a commit that describes itself by its trailers: a record, and the only
  place that work is;
- `refs/uze/sync/<target>`, where UZE fetches the target to: *remembered*,
  fetched again when missing, and never meeting the shelf.
