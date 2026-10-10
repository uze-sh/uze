## Why

An agent UZE launched works in its own checkout, but nothing keeps it
there: it edits a sibling library, commits in a design-system repository,
fixes a script in the operator's dotfiles. The workspace today answers
"what did this agent change" only for the directory the agent was placed
in, so the rest of its work is invisible until the operator stumbles on a
dirty repository days later, with no way of telling which agent left it
there.

## What Changes

- **Each agent in the sidebar gains a level listing the repositories it
  touched**: its own checkout first, marked as home, then every other
  repository on the machine it changed, each with the lines added and
  removed that are the agent's. The agent row says how many other
  repositories there are and folds the level.
- **Clicking a repository opens the code surface on its changes**, rooted
  at that repository, without moving the selection off the agent.
- **Discovery and measurement are separate.** Git is the only thing that
  measures: line counts always come from `git diff` against what a file or
  repository held before the agent touched it. Witnesses only nominate
  repositories and paths, and never report counts.
- **Two witnesses, both injected at launch, both scoped to the one process
  UZE launched**, writing nothing into a harness's own configuration:
  1. **Git trace2** (harness-agnostic, every platform): the launch sets
     `GIT_TRACE2_EVENT` to the agent's trail, so every `git` the agent runs,
     in any repository on the machine, reports the repository and the
     command. A mutating command nominates the repository; a read-only one
     only marks it visited.
  2. **An edit observer per harness** (the vendor's own hook mechanism,
     normalized behind one portable contract): it reports each path the
     harness's file tools are about to write and have written. Measured in
     the Lab on all four harnesses (see design.md): Claude Code through
     `--settings`, Codex through `-c hooks.*`, OpenCode through
     `OPENCODE_CONFIG_CONTENT`, Antigravity CLI through `--add-dir`. The
     handler is the `uze` binary itself (`uze agent trail`), so it runs the
     same under every shell a harness may use, Windows included.
- **Attribution is exact to one agent**, because both witnesses are per
  launch: the agent's own isolated checkout, the files its observer named
  (measured against their content before the agent's first write), and
  the repositories it ran a mutating Git command in (measured from the
  moment it first ran Git there).
- **What an agent touched is a record**: nothing else knows it, so it lives
  under the project's `state/` directory beside the agent's conversation,
  is named in `UzeHome` and declares its shape. Baseline content is kept
  there too; nothing is ever written into a repository UZE does not own.

Out of scope: a shell write (`sed -i`, `echo >`) into a repository where
the agent never runs Git and never uses a file tool; non-Git directories;
filtering the code surface down to the agent's files.

Rejected, with the measurements in design.md: a hook in the embedded `uze`
plugin; reading harness transcripts; polling process working directories;
a baseline over every repository UZE knows (weak: misses every repository
it does not know, costs N `git status` per pass, and cannot attribute
between concurrent agents); kernel observation (no unprivileged mechanism
on macOS or Windows, seccomp user-notify refused on WSL2, ptrace breaks
`sudo` and debuggers); `sh`+`sed` hook handlers (Unix-only, cannot take a
pre-image).

## Capabilities

### New Capabilities
- `touched-repositories`: which repositories an agent UZE launched has
  changed, how they are witnessed, attributed and measured, where that is
  recorded, and how the sidebar shows them and opens one.

### Modified Capabilities
<!-- none: the code surface already opens on any root; the launch
     contribution and the persisted-state tiers are used, not changed -->

## Impact

- `crates/uze-core/src/delivery/integration.rs`: the edit-observation
  contract (`EditObservation`, the observer contribution, `edit_paths`).
- `crates/uze-core/src/machine/harness_runtime.rs`, `src/shim.rs`: the shim
  adds the trace2 environment and asks the integration for its observer
  when it owns an agent identity.
- `crates/uze-integrations/src/{claude,codex,opencode,antigravity}/`: one
  observer contribution and one payload reader each.
- `src/cli/agent.rs`, `src/command_performance.rs`: the hidden
  `uze agent trail <harness>` handler.
- `crates/uze-core/src/machine/home.rs`: `trail_dir` and its children.
- `crates/uze-terminal/src/launch.rs`: `GIT_TRACE2_EVENT` among the stamped
  variables.
- `crates/uze-git`: `read_with_env` beside `write_with_env`.
- `crates/uze-workspace/src/trail.rs` (new): record, fold, attribution,
  measurement; `uze-application` exposes a `TouchedRepositoryView`.
- `src/ui/orchestrator/render/sidebar.rs`, `reads.rs`, `answers.rs`,
  `surfaces.rs`, `session/mouse.rs`, `orchestrator.rs`: child rows, the
  per-repository read, the click and the fold.
- `conformance/`: the four `experiments/<vendor>/launch-hook.py` become a
  contract check per vertical.
