## Context

See proposal.md for why. What exists today, and what this design builds on:

- **The shim owns the launch.** It builds `RuntimeContext { cwd, home }`
  (`machine/harness_runtime.rs`), asks the integration for its
  `runtime_contribution` (extra args and env, fail-open by construction),
  composes it with the continuity plan, and already reads the agent's
  identity (`UZE_AGENT`, `UZE_AGENT_KEY`, `UZE_SHIM_PID`) to decide whether
  it owns the launch (`src/shim.rs`, `owned_identity`). Claude already
  receives `--add-dir`, Codex `-c agents=` through it.
- **Nothing reports what an agent edits.** UZE installs no hooks of its own;
  plugin hooks are machine-scoped and compiled per harness, and the embedded
  `uze` plugin may not gain an executable capability without the operator's
  trust (`package/acquisition.rs`, `trust.rs`).
- **The code surface opens on any root.** `CodeView::opening(root, …,
  ContentMode::Diff)` reads everything from `view.root()`; `open_code_at`
  exists but is hard-wired to `Contents` (`src/ui/orchestrator/surfaces.rs`).
- **The sidebar draws an agent as two rows** (`render/sidebar.rs`
  `draw_tree`), measured separately by `agent_rows`, hits first-match-wins,
  `WorkspaceHit` is `Copy`. A `SelectTab` click closes the extension; the
  surface closes itself when the selected tab changes
  (`close_extension_left_behind`).
- **Every Git read the client makes runs on a thread** and is absorbed with
  the question it answered (`orchestrator/reads.rs`, `answers.rs`).
  `--numstat` parsing exists in `uze-extensions/src/code/changes.rs`.
- **Records live per project** under `state/projects/<id>/`, one file per
  agent where writes are frequent (`conversation_path`).

### What was measured (2026-10-10)

The Lab ran each harness's real binary against the synthetic provider, with
a hand-written observer and no UZE in the hook path. Experiments, kept as
evidence: `conformance/experiments/<vendor>/launch-hook.py`.

| Harness | Launch-scoped injection | Path in payload | Notes |
|---|---|---|---|
| Claude Code 2.1.293 (16/16) | `--settings <file or JSON>` with a `PostToolUse` group on `Write\|Edit\|MultiEdit\|NotebookEdit`; merges with the user's own hooks | `tool_input.file_path` (`notebook_path` for notebooks), absolute; payload carries `cwd` | No prompt headless or TUI. MultiEdit not offered on this version; NotebookEdit deferred behind ToolSearch. |
| Codex 0.161.0 (32/32) | `-c 'hooks.PostToolUse=[{hooks=[{type="command",command=…,timeout=10}]}]'` plus `-c 'hooks.state={"/<session-flags>/config.toml:post_tool_use:0:0"={trusted_hash="sha256:…"}}'` | `tool_input.command` holds the `apply_patch` text; paths from `*** Add/Update/Delete File:` and `*** Move to:`, verbatim (relative ones against payload `cwd`) | Without the trust flag exec skips the hook silently. The hash is `codex/trust.rs::identity_hash`. Any `-c` makes the TUI run embedded instead of on the shared daemon (already true: UZE passes `-c agents=`). A refused patch fires no PostToolUse. |
| OpenCode 2.0.24 (29/31) | `OPENCODE_CONFIG_CONTENT='{"plugins":["/dir"]}'` (or `OPENCODE_CONFIG=<file>`); plugin dir with `package.json`, V2 API `ctx.tool.hook("execute.after", …)`, no `@opencode-ai/plugin` import; merges with the user's plugins | `input.path`, verbatim (may be relative, no cwd in event) | **Per-launch scoping holds only with `--standalone`.** The default shared service keeps the plugins and environment of whichever launch started it: a later injected launch loads nothing, and a plugin loaded first observes uninjected launches. `OPENCODE_CONFIG_DIR` replaces the user's config: unusable. |
| Antigravity CLI 1.3.1 (13/13) | `agy --add-dir <dir>` with `<dir>/.agents/hooks.json` (named hook, matcher `*`); the added dir is treated as a customization root | `toolCall.args.TargetFile`, absolute as the model wrote it | Hooks are gated server-side (`json-hooks-enabled`, on when signed in; API key since 1.1.25); headless `--print` runs no hooks. The added dir becomes a workspace root (visible to the model, likely writable without the outside-workspace prompt). `--continue` without the flag does not revive it. |

Common to all four: the hook process inherits the launch environment; the
user's own hooks keep running and their files are byte-identical; a launch
without the injection runs nothing extra; **a shell write (`echo >`,
`sed -i`) never reaches a file-tool hook**.

**Git trace2** (`GIT_TRACE2_EVENT=<existing dir>`, one file per git process,
`def_repo.worktree` absolute, `start.argv`) reached the agent's shell-tool
git in all four harnesses, and on the OpenCode shared service followed each
launch's own environment. Noise: harness housekeeping git — Claude/Codex
`-c core.hooksPath=/dev/null …` calls and scratch repos under `/tmp`,
OpenCode ~42 snapshot traces of the project per turn. The directory must
exist beforehand, or git writes a single file. Under Codex `workspace-write`
the trace directory must be a writable root
(`-c 'sandbox_workspace_write.writable_roots=[…]'`), which also lets the
agent write there. `GIT_TRACE2_ENV_VARS=UZE_AGENT` stamps the agent id into
each file (`def_param`), verified on git 2.43.

**Kernel observation** (Linux only), from a scratch POC: a seccomp filter on
write-intent syscalls (`open*` with write flags, `creat`, `rename*`,
`unlink*`, `truncate`; `io_uring_setup` refused so runtimes fall back) caught
writes from `sh`, `sed -i`, `python3`, Node (sync and libuv async), Bun and
Git, at negligible cost. Seccomp user-notify fails with `EBUSY` on WSL2
because WSL's `/sbin/init` already holds a listener for every process;
`ptrace` + `SECCOMP_RET_TRACE` works but requires `no_new_privs` (measured:
`sudo: The "no new privileges" flag is set`) and excludes `gdb -p`/`strace`.
macOS (Endpoint Security entitlement) and Windows (ETW, admin) have no
unprivileged equivalent. Rejected: not cross-platform.

## Goals / Non-Goals

**Goals:**
- Exact, per-agent attribution on Linux, macOS and Windows, for every
  harness, through what each launch already controls.
- Correct counts for an isolated agent, a sibling repository edited through
  file tools, a file that was already dirty, a commit made elsewhere, two
  agents in one repository.
- No cost to sessions UZE did not launch; no write into a harness's own
  configuration; no write into a repository UZE does not own.

**Non-Goals:**
- A shell write into a repository where the agent never runs Git and never
  uses a file tool. Closing it needs either kernel observation (rejected
  above) or polling repositories UZE knows (rejected: misses unknown
  repositories, costs N `git status` per pass, cannot attribute between
  concurrent agents).
- Filtering the code surface to the agent's files.
- Non-Git directories.

## Decisions

### 1. Git measures; witnesses only nominate

Every count comes from `git diff --numstat` in the repository, against the
agent's baseline, restricted to attributed paths. A witness only adds a
repository (and, for the observer, a path). One way of computing a number;
a wrong nomination costs a row with no counts, never a wrong count.

### 2. Two witnesses, both added at the launch boundary

When `owned_identity()` resolves to an agent, the shim:

- adds `GIT_TRACE2_EVENT=<trail>/git` and `GIT_TRACE2_ENV_VARS=UZE_AGENT`
  to the exec'd environment — vendor-neutral, no integration involved;
- passes `RuntimeContext.observe: Option<Observe { inbox, handler }>` and the
  integration answers through its contribution with the observer for that
  one process.

`GIT_TRACE2_EVENT` joins `STAMPED_VARIABLES`
(`crates/uze-terminal/src/launch.rs`): the server is routinely started from
inside an agent's pane, and every plain pane would otherwise trace into that
agent's trail.

### 3. The portable edit-observation contract

In `uze-core::delivery`, vendor-neutral, declared like `SessionContinuity`:

- `edit_observation() -> EditObservation::{Supported, Unsupported(reason)}`,
  Unsupported by default so a fifth harness is safe before anyone looks;
- `observe_contribution(ctx) -> HarnessRuntimeContribution`: the args/env
  that make this one process call the handler on pre- and post-edit events;
- `edit_paths(event_bytes, cwd) -> Vec<PathBuf>`: the vendor payload read
  into absolute paths (table above).

Each integration's contribution is Generated Native: the vendor's own hook
mechanism, scoped by the vendor's own launch surface.

### 4. The handler is the `uze` binary

Hooks call `"<canonical uze>" agent trail <harness> <pre|post>` (hidden,
agent-audience, classified `Budgeted` in `command_performance.rs`). It reads
stdin, calls the integration's `edit_paths`, appends one JSONL line
`{ts, event, paths}` to the inbox, on `pre` snapshots each existing path's
content as a blob into the trail's own object directory
(`GIT_OBJECT_DIRECTORY=<trail>/objects`), and exits 0 in every case. Release
`uze --version` measured 10 ms.

*Why not `sh` + `sed`:* Claude runs hooks under Git Bash or PowerShell on
Windows, Codex has a separate `commandWindows`, Antigravity documents no
shell; a quoted absolute path plus three words is the one command line all
of them read identically (`uze_platform::shell::quote`). `sed` on JSON is
Unix-only, breaks on escaped quotes, and cannot take a pre-image.

*Why ADR-040 does not apply:* "no `uze` on the hook execution path" exists
because a *delivered* hook must outlive UZE being moved or removed. This
hook exists only inside a process this very binary exec'd. The exception is
written down as an ADR at archive.

OpenCode's observer is a JS plugin (V2 `ctx.tool.hook("execute.before" /
"execute.after")`) that appends the same JSONL line itself, or spawns the
handler.

### 5. Trail layout and tier

`UzeHome::trail_dir(project_id, agent)` → `state/projects/<id>/trails/<agent>/`:
`record.json` (`uze_document::Shaped`, shape 1), `inbox/` (handler JSONL),
`git/` (trace2 files), `objects/` (baseline blobs). All one tier, *record*:
nothing else knows which repositories an agent touched, and a pre-image
cannot be observed again. Discarded with the agent, as
`conversation::forget` does. The launch creates the directories so the
first write cannot fail.

### 6. Fold, nomination and the trace2 verb classifier

On the client's read thread (`uze-workspace::trail`): consume inbox lines
and trace files, delete what was consumed (volume: OpenCode alone writes
~42 traces per turn), tolerate a torn last line. A trace's
`def_repo.worktree` is classified by its argv verb: a mutating verb
(`commit add apply am cherry-pick checkout switch restore reset rebase merge
mv rm stash pull clone init worktree`) marks the repository *touched*; a
read-only one marks it *visited*, hidden until a measurement shows a
non-zero delta. Housekeeping calls (`-c core.hooksPath=/dev/null …`,
repositories that no longer exist) nominate nothing. Observer paths map to
their repository with `uze_git::repository::root`, nearest existing
ancestor for a deleted file.

### 7. Attribution and measurement

- *own*: an isolated checkout, everything since its fork point;
- *evidenced*: observer paths, `git diff --numstat <pre-image blob> --
  <path>` with `GIT_ALTERNATE_OBJECT_DIRECTORIES=<trail>/objects`; a
  post-only event falls back to the HEAD blob at first touch; a new file
  counts whole;
- *git-nominated*: baseline = HEAD plus dirty-path blobs at the first
  trace, stored the same out-of-repo way; the row says counts start there.

Needs `uze_git::read_with_env` beside `write_with_env`. Nothing is written
into the measured repository (verified with alternates in a scratch POC).

### 8. Read model and client

`uze-application` exposes `touched_repositories(project, agent) ->
Vec<TouchedRepositoryView { root, display, own, attribution, summary }>`,
called only from `spawn_touched_repositories` in `orchestrator/reads.rs` and
absorbed by `absorb_touched_repositories` keyed by agent id, paced like the
git badge. `draw_tree` draws child rows after the agent's second row and
`agent_rows` counts them. The agent row gains a trailing `+N repo` and the
disclosure mark. Hits: `OpenTouchedRepository { tab, index }` (pushed before
the row-wide hit) and `ToggleTouchedFold(tab)`; drag grouping and
`space_blocks` ignore them. The click opens `Diff` at the repository root
via `open_code_at(…, mode)`; if the agent's tab is not selected, it selects
it and opens on the server's confirmation. New `Symbol`s for home/other
repository; the fold set is an additive field in `client_layout`.

## Candidate ADRs

- **UZE observes an agent it launched through the launch boundary** — two
  witnesses, the `uze` binary as handler (the ADR-040 exception), Git
  measures and witnesses nominate.

## Risks / Trade-offs

- [Windows: each harness's hook shell must run `"C:\…\uze.exe" agent trail
  …` as written] → measured in Windows Sandbox before any harness is
  declared Supported there.
- [Codex trust: UZE answers Codex's hook review for its own hook, in one
  process, via `hooks.state`; a change to Codex's identity hash makes the
  hook skip silently] → a contract check per Codex version in the Lab;
  narrower than `--dangerously-bypass-hook-trust`, which trusts every hook.
- [OpenCode shared service breaks per-launch scoping] → see Open Questions.
- [Antigravity's server-side hook gate] → Unsupported for the observer when
  off; Git still witnesses.
- [Antigravity's added dir is visible to and writable by the agent] → it
  holds only the generated `hooks.json`; regenerated per launch.
- [trace2 volume and housekeeping noise] → fold deletes consumed files; the
  verb classifier; fixture test over the corpus the experiments captured.
- [A replaced `uze` binary mid-session] → `agent trail`'s ABI stays trivial
  (stdin in, JSONL out) and tested as a stable surface.
- [Codex `workspace-write` needs the trail as a writable root] → granted for
  `<trail>/git` only.
- [Two agents writing the same file both show it] → each is measured from
  its own pre-image; acceptable.

## Migration Plan

Additive: a new record kind, a new stamped variable, a new optional field
in `client_layout`, a hidden CLI verb. An older build ignores `trails/`;
deleting it loses only the list of touched repositories. Rollback is
removing the contributions; nothing in any harness's configuration needs
undoing.

## Open Questions

- **Codex self-trust.** Recommended: accept `hooks.state` for UZE's own
  generated hook, one process at a time. Needs the operator's sign-off: it is
  UZE answering a vendor safety review on the operator's behalf, for a hook
  the operator already trusts by trusting UZE.
- **OpenCode scoping.** (a) launch workspace agents with `--standalone`
  (scoping holds by construction; the agent leaves the shared service), or
  (b) a machine-wide inbox keyed by `sessionID` that the fold maps to the
  agent through the conversation record (the plugin exists in the service
  only if a UZE launch started it). Recommended (a), after checking that
  UZE's session read-back (`opencode api GET /api/session`,
  `opencode/session.rs`) still sees a standalone session.
- **Antigravity gate detection** at launch (`/hooks` listing or log line),
  so Unsupported is reported rather than silently empty.
- **Pre-image timing.** Each harness's pre-event must fire before the write
  and carry the path; Codex's pre payload should carry the full patch.
- **The residual gap's size.** The design assumes agents run Git where they
  edit through the shell. Measure on real sessions before deciding whether a
  nudge in the projected `AGENTS.md` region is worth it.
- **The embedded trust boundary.** `PackageSource::Embedded` crosses the
  trust boundary (`package/acquisition.rs`) on the ground that "the
  operator trusted the binary, not necessarily every capability a future
  revision of an embedded snapshot might declare". The embedded bytes ship
  inside the binary the operator already chose to run, so authorship is
  not what is in question; what an executable capability adds is *reach*:
  a hook runs inside every session of a harness, not only when `uze` is
  invoked. That argues for a one-time notice of reach (at `uze setup` or
  the first workspace open), not for the third-party trust question. The
  rule is dormant today (the one embedded plugin is Skill-only). Not
  decided here: this change does not route through the plugin either way,
  because machine-wide reach and the portable wrapper's limits (32 KB,
  `jq`) are the reasons it does not. Revisit as its own decision (ADR)
  before any embedded plugin gains a hook.
