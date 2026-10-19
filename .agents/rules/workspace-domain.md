---
trigger: glob
globs: "**/crates/uze-workspace/**"
---

`uze-workspace` is the workspace's domain: the agents UZE launches
(`task`), their checkouts (`checkout`, `worktree`), the conversation each
is in and carrying it across a launch (`conversation`, `continuity`), how
finished work lands (`landing`), what a project declares about all of that
(`declaration`: `agents.yaml`'s `workspace:` section, which `uze-core`
carries unread), the operator's approval of the commands that declaration
runs (`approval`), what an agent moved to another harness is told
(`briefing`), and the workspace client's own state.

It is built on `uze-core` and unrelated to `uze-terminal`, which owns the
panes and knows nothing of agents. `uze-core` never depends on it: that is
how the package manager works without the workspace.

Git is reached only through `crate::git`, the slot guard that refuses a
slot whose `.git` no longer names the project's repository; a direct
`uze_git::read`/`write` here fails the architecture suite.
