# uze-workspace

The workspace's domain: what UZE does with the agents it launches. Built on
`uze-core`, which never depends on it: the package manager works without the
workspace because nothing in it can name anything here.

- `task`: the record of each agent UZE launched
- `checkout`, `worktree`: where an agent works, and the slots it is given
- `conversation`, `continuity`: the conversation an agent is in, and
  resuming it across a launch
- `landing`: how finished work reaches the target branch
- `declaration`: what a project declares about all of that (`agents.yaml`'s
  `worktrees` and `artifacts` sections, which `uze-core` carries unread)
- `client_layout`, `prompt_history`, `notifications`, `extensions`: the
  workspace client's own state under `UzeHome`

`uze-terminal`, which owns the panes, is unrelated to this crate and knows
nothing of agents.

```bash
cargo test -p uze-workspace
```
