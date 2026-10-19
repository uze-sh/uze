---
trigger: glob
globs: "**/crates/uze-git/**"
---

`uze-git` is the one transport for speaking to the Git binary: the spawn
convention and the `read`/`write` entry points. It carries no domain and
reports Git's exit code rather than classifying it: a non-zero exit is an
answer for `diff`, `rebase` and `rev-parse --verify` and a failure
elsewhere, and only the caller knows which. Two callers with two exit-code
conventions is what it replaced, and a repository write lock cannot be
complete if a module spawns Git around it.
