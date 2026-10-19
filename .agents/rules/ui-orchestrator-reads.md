---
trigger: glob
globs: "**/src/ui/orchestrator/**, **/src/ui/orchestrator.rs"
---

Nothing the workspace client draws waits on a repository. Every Git read
it makes (the badge, the sidebar timeline, a commit's account, the changes
overlay and its per-file diff) runs on a thread and answers through a
channel, and so do the two other unbounded operations: placing a new agent
(`git worktree add` plus the project's `setup`) and slot reconciliation.

Two tests hold it: `src/ui/orchestrator/` may not name `WorkspaceHost` at
all, save `reads.rs`, where the reads live; and every mention of it there
and in `src/ui/orchestrator.rs` must sit inside a `thread::spawn`.

Every answer carries the question it was asked, so one that arrives after
the viewer moved on is dropped rather than drawn. A new read is a
`spawn_*` in `orchestrator/reads.rs` and an `absorb_*` beside the existing
ones, never an inline call.
