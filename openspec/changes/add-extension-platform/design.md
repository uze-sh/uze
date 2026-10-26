## Context

See proposal.md for why. What the design has to work with:

- **The contract types already hold.** An extension answers with a
  `view::View` or a `view::Section`, receives `Command` and `ViewHit`, and
  reaches the machine only through `uze_extensions::Host`
  (`crates/uze-extensions/src/lib.rs`). `tests/architecture/layering.rs`
  forbids `uze_core`, `uze_application`, `std::process`, `std::fs` and
  `crossterm` in `crates/uze-extensions/src`.
- **Everything past the contract is hand-wired per extension.** Each
  extension has its own model fields (`code`, `architect`, `spec`,
  `timeline`, `code_measures`, ... in `src/ui/orchestrator.rs`) and its own
  `ExtensionHit` variant (`Code`, `CodeTimeline`, `Architect`, `Spec`,
  `SpecSummary`). The `spec` extension already reads OpenSpec, Spec Kit,
  Superpowers and GSD (`crates/uze-extensions/src/spec.rs`). Each has its own
  outcome enum and `follow_*` (`src/ui/orchestrator/session.rs`), its own
  channels and `spawn_*`/`absorb_*` pairs, and its own `render_extension`
  branch. It has its own remembered place map (in memory only), and its
  keymap scopes and actions are hardcoded in `crates/uze-keys`: `Scope` and
  `Action` are closed macro enums.
- **The registry is metadata.** `ExtensionRegistry::builtin()` lists
  `BuiltinExtension` records, and the Extensions screen is static.
- **Plugins are discovered by layout.** `delivery/engine.rs`
  `package_resources_at` walks a package for `skills/`, `hooks.json`,
  `mcp.json`, `agents/` and `AGENTS.md`; extensions are not among them
  (*Discovery*). Trust (`package/trust.rs`) asks only
  when a source crosses the trust boundary (remote Git, embedded), and it
  records nothing: "not a permission system".
- **Process building blocks exist** in `machine/subprocess.rs`:
  `with_process_group`, `wait_with_timeout` (kills the group), `read_bounded`,
  and the libc `kill_process_group`. No helper clears the environment; see
  `acquisition/git.rs` `run_as` for the pattern.
- **YAML is read by `noyalib` 0.0.x**, in `uze-core` only, including
  third-party agent frontmatter. `schemars` exists only as an optional
  dev-server dependency.
- **What UZE observes of an agent today is its pane.** Whether an agent is
  working is read off its pane's paints and its prompt submissions
  (`src/ui/orchestrator/tests/activity.rs`); no harness reports a turn, a
  tool call or an approval to UZE. UZE does write into an agent's pane
  (`src/ui/orchestrator/session/pane_input.rs`) and does deliver portable
  hooks to every harness (archived `add-portable-hooks`).
- **Images are already drawn.** `src/ui/graphics.rs` draws a
  `view::Picture` as half blocks everywhere and as Sixel where the terminal
  says it can; the `code` surface previews images through it.
- **Cells are already a medium.** `view::Medium::Drawing` and
  `shared/canvas.rs` paint the architect's diagrams in cells coloured by
  `Role`, not by colour value.

### Evidence (2026-10-23)

A survey of each harness's extension surface and of adjacent tools, read
from their source and docs, with the projects that use them best. Only what
shaped a decision below is kept here.

| Ecosystem | Draws in its TUI | What authors reach for | Isolation |
|---|---|---|---|
| Claude Code mods | panes, a band above the prompt, rewrites of transcript rows; Box/Text/Button/Markdown/Code/Input/Select, a 512×256 cell `Raster` at ~60fps, images, a real-time `Client` module with raw input | the session (prompt, commands, tools, model), fs, exec, network, a reactive state model and a cross-session store | none beyond a V8 isolate |
| OpenCode V2 | fixed regions (sidebar, composer, footer, a panel), dialogs, pages, toasts, a layered keymap; images | server and TUI halves joined by typed RPC; an 80-type event stream | none, in process |
| Codex | nothing: a fixed status line, syntax themes, and pets (sprites chosen by agent state, assets only) | 14 hook events; the app-server JSON-RPC protocol, on which full alternative clients are built | — |
| Antigravity | a script status line; web "UI sidecars" in the desktop app only, behind flags | 6 hook events; an agent API for sidecar daemons | declared agent permissions |
| Zellij | ANSI text plus host-drawn semantic components (table, list, button, input, ribbon) themed by role | ~80 subscribable events, workers, pipes that launch their target, ~270 host commands | WASM, 17 permissions asked at first use, events buffered meanwhile |
| Raycast | a fixed catalog (List, Grid, Detail, Form, ActionPanel, MenuBarExtra) | typed preferences, intervals with a 1-minute floor, host-animated loading | review only |
| k9s, lazygit | actions over the host's UI | argv with context variables, typed inputs, `confirm`, `dangerous` | — |

What the survey says, in the order the decisions below answer it:
1. No harness isolates plugin code; UZE's grants are the gap no one fills.
2. Where the TUI lacks a canvas, authors leave for a local web server
   (OpenCode's visualiser, plannotator, ensemble; every creative Codex UI).
3. The best-loved projects watch agents (claude-hud, ccstatusline,
   flightdeck, agents-monitor) and must scrape transcripts to do it.
4. A rich catalog with no drawing (Raycast) and semantic components beside
   an escape hatch (Zellij) both work; raw colour and raw ANSI are what
   break theming.
5. Animation belongs to the host for stateless code (Raycast `isLoading`,
   Codex pets), and to the plugin only when it is resident.

## Goals / Non-Goals

**Goals:**
- One host path for every extension, with `code` and `architect` on it and
  no observable change to either.
- A declared extension that is data plus stateless programs, with zero
  resident processes when idle.
- A catalog wide enough that a list-over-a-CLI tool never needs to draw, and
  tiers above it for what a catalog cannot say (D13).
- A security model where the operator's consent, the manifest and the host's
  enforcement are the same list.

- An author can build, without leaving the terminal for a web page, what
  operators already build on the harnesses: the ten plugins of the gallery
  (D20) run on UZE with no code of UZE's written for any one of them.
- One extension works the same whichever harness its agents run.

**Non-Goals:**
- Rewriting a harness's own transcript or prompt rows (what Claude Code's
  mods can do to `AssistantMessage` or `ToolUse`). UZE does not draw the
  harness's output; it draws around it.
- Raw terminal output from an extension at any tier: no ANSI, no escape
  sequences, no colour values outside an image.
- Native code in UZE's process: dynamic libraries and native interpreters
  stay ruled out (ADR-041). The only embedded language is Rhai, in its own
  crate (D19, D27).
- Remembering a view's place across runs of the client. The built-ins don't,
  and adding it is a persisted-state question of its own.
- Distributing extensions outside plugins and marketplaces.
- Detecting malicious intent. The model bounds what any extension can do;
  it does not judge what one means to do.

## Decisions

### D1. Split the work into three layers that already exist

| Layer | Owns | Crate |
|---|---|---|
| **Declaration** | the manifest and wire types as plain serde data, the field-reference grammar, the catalog's names, the reserved ids | new leaf crate `uze-contract` (serde only: no I/O, no domain, no rendering) |
| **Domain** | finding `extensions/*/extension.kdl` in a package, parsing it and its views with the bounded KDL decoder (D22), checking it, grants, running programs confined | `uze-core` (`capability/extension`, the runner beside `machine/subprocess.rs`), `uze-platform` (confinement, one implementation per platform), `uze-application` (read model, `ExtensionRunner`, the view compiler service) |
| **Presentation** | the generic `Extension` contract, the declared-extension engine (manifest + answers → `View`/`Section`, view state), the catalog's view types | `uze-extensions` |

The leaf crate is what lets `uze-core` (check, install, grants) and
`uze-extensions` (drawing) share one definition of the manifest without either
reaching the other. That is the same move as `uze-theme` and `uze-document`.
This changes a rule that holds today: `uze-extensions` depends on no UZE
crate (ADR-041, AGENTS.md, the `extensions` rule). The amendment is narrow
and lands in the commit that creates `uze-contract`, never after: the only
UZE crate `uze-extensions` may depend on is `uze-contract`, a leaf with no
I/O. In that same commit the layering suite admits exactly that edge and a
new rule keeps `uze-contract` free of I/O, `uze_*` dependencies and
rendering libraries; `crate-layering.mmd` gains the crate.

*Alternatives:* define the manifest in `uze-extensions`. Rejected: it would
make `uze-core` depend on presentation. Define it in `uze-core` and mirror it
in `uze-extensions`. Rejected: two definitions of one schema drift.

### Discovery

An extension is found by its own path, never as a routed package resource
(A5, resolved by `extension-view-engine`):
`uze_core::capability::extension::discover(plugin_dir)` lists every
`extensions/<id>/extension.kdl` (directory entries only, id order), and
`uze_application::Extensions::installed()` is its only consumer, reading a
linked marketplace's plugin from its working tree and every other from the
Store. `delivery/engine.rs::package_resources_at` walks `skills/`,
`mcp.json`, `agents/` and `hooks.json` and nothing else, so the Router,
exposure, receipts and the integrations never see an extension.
`Extensions::check(plugin_dir)` compiles each one for `uze agent plugin
check`; change 4 makes it check sources, actions, events and `wants` too
(preferences in change 6), with the decoder's refusals (unknown node,
repeated property, wrong case, type annotation), and the grants record
(D5) is read beside it. VS Code reads `contributes` from the manifest and
Zed reads `extension.toml` and nothing else; both find an extension by a
fixed path, which is what `discover` does.

### D2. The generic contract: `Extension` plus opaque jobs

Rewritten 2026-10-26 to describe what `extension-view-engine` and
`generic-extension-host` built (A1–A3 resolved). The host speaks to every
extension, built-in or declared, through `contract::Extension` alone
(`crates/uze-extensions/src/contract.rs`), by the slot a surface stands in
(`src/ui/orchestrator/declared.rs`).

- **Surfaces.** `identity()` is what the host attributes; `surfaces()` lists
  `SurfaceInfo { id, slot, title }`; `view`, `section`, `command`, `hit`
  and `scroll` answer by surface id. The gestures a board or an editor
  needs (`open`, `enter`, `typing`, `paste`, `focus`, `missed`, `close`,
  `layout`, `hover`, `drags`, `drag`, `pan`, `scroll_to`, `text`) each have
  a default that does nothing (A1, H4).
- **Opaque jobs.** `jobs()` hands the host `Job { run: FnOnce(&dyn Host)
  -> Opaque, fallback: FnOnce() -> Opaque }`; the host runs each on a
  thread in `orchestrator/answers.rs::schedule_engine_jobs` with
  `WorkspaceHost`, answers the fallback when the thread dies, and routes
  the answer to the same engine's `absorb`
  (`orchestrator/answers.rs::absorb_engine_answer`), which returns an
  `Outcome` (a write that failed after its surface closed is still said;
  the `orphaned` hook was dropped because the extension knows whether its
  surface is open). `lanes()` says whether the extension's jobs run one at
  a time (`Serial`, the default: a save ahead of the read after it) or
  together (`Parallel`).
- **Outcome** is one enum: `Stay`, `Close`, `Notice`, `OpenPath { project,
  target }`, `OpenCommit`, `Copy`, `Show`, `Failed`.
- **Place and memory.** `place()`/`resume()` carry where the viewer was, per
  surface; `memory()`/`restore_memory()` carry what outlives every surface
  (a cache, a measure), kept by the host while the client runs.
- `ExtensionHit { slot, hit }`: the slot names the extension and the
  surface, so no per-extension variant exists.
- **The host's side.** One engine per extension, shared by its slots, keyed
  by `Owner::{Builtin(id), Declared(Identity)}`; the `Hosted` trait
  (`replace`, `set_data`, `inspect`, `forget`) is what the host asks beyond
  the contract, empty for a built-in. The architect's artifact read is
  `Host::artifact_source` (A2); the code timeline is a `section` of
  `CodeExtension` and its commit popup is `Outcome::OpenCommit` (A3).

What this change adds to the contract: nothing. A declared extension that
runs scripts asks the host through a `Job`, exactly as `code` asks for a
diff: the question is a declaration id and an input (D3), never argv or
script text.

*Alternative:* an enum of request kinds in the contract. Rejected: every new
extension would widen a shared enum, which is the closed-set problem again.

### D3. The declared-extension engine lives in `uze-extensions`

`declared/` implements `Extension` from a `Manifest` (leaf crate) and holds
all view state: selection, folds, scroll, subject, mode, menus, confirmation.
A job it hands the host is always "answer declaration D of extension E with
input I". The host fulfills it through a new `Host::answer(Question) ->
Answer`, where a question names the extension's identity, a declaration id
and the declared input; the `ExtensionRunner` in `uze-application` resolves
the declaration from the compile the operator consented to (D5), and only
then knows whether it is a `script`, a `run` or a `fixture`, so what runs is
what was accepted, never what the engine carries. The engine never holds
argv or script text. There is no separate `ProgramHost`: a built-in has no
manifest, so there is no declaration to find, and an architecture test says
only `declared/` calls `answer`. That method is the grant that did not
exist, stated as narrowly as ADR-048's: answer *a declaration of this
extension*. It is never a free-form command. Field references (`{name}`)
are resolved by the engine against the answer or the selection, and in the
manifest they only ever produce whole strings; views follow D23.

### D4. The program contract reuses the hook contract's shape

Details are in `specs/extension-program-contract`. The runner is a new
`uze-core` function next to `subprocess.rs`'s helpers. It composes
`with_process_group`, `read_bounded` on both streams, `wait_with_timeout`,
an allowlisted environment, and confinement (D6). Ceilings are 10 s and
1 MiB of stdout / 64 KiB of stderr by default. A manifest may lower them
and never raise them. The runner is the only place in the workspace that
spawns an extension program. An architecture rule sanctions it by name, the
way `uze-git` and `acquisition::git` are sanctioned.

Answers are remembered in memory for the session, by the host's runner
(the `ExtensionRunner` of `uze-application`, never `uze-extensions`), keyed
by (identity, source, checkout, input digest); nothing is written to disk,
so there is no cache path to name and nothing a restart could leave stale.
Reopening a surface in one session costs no run; the first open of a
session costs one (decided 2026-10-26, M2: `uze-extensions` may not use
`std::fs`, no event invalidates a disk cache across restarts, and D28
measured 0.45 ms per script answer).

Modes and envelope members arrive with the change that can honour them:
`json` and `data`/`refresh`/`toast`/`notice` with change 4; `lines` and
`store` with change 5 (programs, the store); `view` (compose), `notify`
and `sound` with change 6; `cells`, `image` and `sprite` with change 7.

A declaration names its output mode:
- `json` (the default): a list of objects or an object, for `bind`.
- `lines`: one item per line, tab-separated fields named by the
  declaration, so a `sh` author needs no `jq` (the UX finding).
- `view`: a catalog tree, for `compose` (D15).
- `cells`, `image`, `sprite`: the tier-two answers (D13), each bounded in
  size before it is decoded:
  - `image`: `{format, data}` with `format` one of `png`, `jpeg`, `gif`,
    `webp`, `bmp`, `ico` (the decoders `code/picture.rs` already has, moved
    to `shared/`) and `data` base64, or `{file}` naming a packaged file;
    pixels are capped from the slot's geometry, not from the operator-file
    ceiling, and decoded off the drawing thread;
  - `sprite`: a packaged sheet plus `{frame: [w, h], columns, states:
    {name: {frames, ms, loop}}, fallback}`; the host scales it to the slot
    and composites alpha on the theme's ground;
  - `cells`: rows of `{ch, role}` plus `regions: [{rect, action, item}]`.

Every answer may be wrapped in an envelope, `{data, store, refresh, toast,
notice, notify, sound}`, where `data` is the answer of its mode and the rest
are requests the host applies under the extension's grants (`notify` needs
`notify`). A bare answer is `data` alone.

The input object carries, besides the declared `input` fields:
`context` (`checkout`, `branch`, `work`, `surface`, `extension`, `slot`,
`size` in cells, and `agent` for a surface on an agent's pane), and, only
when the declaration names them, `event`, `store` and
`preferences`.

### D5. Consent is enabling, and it is recorded

Trust today answers "may these bytes be installed" and records nothing. An
extension needs a different question: "may this code run with these grants".
So consent to an extension is **enabling** it, separate from installing the
plugin. Settled 2026-10-26 (S4, S5, 0.2.2, R-A10, R-S14):

- **Identity.** An extension's canonical identity, for every machine-facing
  key, is `plugin@marketplace/id`, the marketplace by the name registered
  in `marketplaces.json` (a marketplace re-registered under another URL is
  another identity). It keys the grants record, the session cache (D4), the
  store (D18) and the keymap namespace, whose `keys.json` name is
  `ext.<plugin>@<marketplace>/<id>.<action>` (D8). The chrome keeps showing
  `id · plugin@marketplace`, which is the `Display` of
  `uze_contract::Identity`. A bare `id` is never a key: a same-named
  extension from another plugin is a stranger, with no record, store, cache
  or keys in common, and a change of plugin or marketplace is a new
  identity consented to from nothing.
- **The record** is `state/extensions.json`, shape 1, per identity: the
  accepted grants, the digest and time they were accepted at, the resolved
  `exec:` paths (change 5), whether confinement was enforced when accepted
  (`enforced_at_accept`), and the projects it is enabled in (`enabled_in`,
  each identified as ADR-055 identifies a project). It declares its shape
  through `uze_document::Shaped`, and its path is named in `UzeHome`.
- **Consent is per machine; enabling is per project.** Consent is given once
  per (identity, digest), because the code is the same in every project.
  Enabling is per project: an extension runs only in a project the operator
  switched it on in; a plugin a project declares offers its extensions
  there, and the first open of the workspace in that project shows the
  toast → dialog, as the project's `approved-commands` do; a plugin
  installed at machine scope appears in every project, disabled until
  switched on there. There is no machine-wide enable.
- **The digest is of the bytes that run.** It is the SHA-256 of the whole
  `extensions/<id>/` tree (manifest, views, scripts, packaged programs,
  fixtures) over the bytes the loader read into memory once, bounded; what
  compiles and runs is what was digested, never a re-read of the disk.
- Every source asks, local ones included (unlike `trust.rs`), because what is
  consented to is execution on this machine, not provenance.
- **An update is compared by grant set** where every accepted grant is
  enforced on this machine, and **by digest** where any is not (an `exec:`
  without OS confinement, a general-purpose runner anywhere) or where
  `agent:prompt:auto` is held. Only a widening, or a digest change under
  the digest rule, holds the extension; a narrowing applies; the same set
  applies without asking, and the Extensions screen shows "updated
  <date>, same grants" with the new digest. A linked local marketplace is
  always by grant set, which keeps it editable without a prompt on every
  save. Optional pinning by digest is an operator preference owed to change
  8.
- Where consent is asked, and only there:
  - `uze install`, on a TTY outside an agent's pane, prompts after the
    plugin installs, using the prompt style of `PromptingAuthority`.
  - A non-interactive install leaves the extension disabled and reports it.
  - The TUI Extensions screen has Enable/Disable with the grants listed.
  - `uze status` and `uze inspect <plugin>` report each extension's state
    (enabled, needing consent, held by a widened update). Enabling is never
    a CLI verb run from an agent's pane (R-A10): the CLI refuses when
    `UZE_AGENT` is in the environment or `launched_by` points at a pane, the
    check `uze workspace allow` already makes (ADR-055), and there is no
    `--yes`/`--trust` flag for extensions.
- Built-in extensions are part of the binary and need no grants.
- **Grants arrive with their enforcement.** Change 4 accepts
  `read:checkout`, `clipboard` and `open_path` (the last two only from a
  gesture); the check refuses every other grant with "arrives in change
  N": `exec:` and `store` in 5, `agent:*`, `notify` and preferences in 6,
  `resident` in 7, `net` and `secret` in 8.

*Alternative:* extend `trust.rs`'s `executable_capabilities` so installing
is the consent. Rejected for two reasons. Trust is skipped for local sources.
And holding a widened update would need a stored grant, which `trust.rs` is
deliberately without.

### D6. Confinement: Landlock on Linux, Seatbelt on macOS, disclosed elsewhere

This is the precedent other agent CLIs set for running untrusted helpers.

Confinement is a concept in `uze-platform`, with one implementation per
platform and "unavailable" as one of them (AGENTS.md: a `cfg!` at a call site
is a missing concept there). The runner in `uze-core` asks it for a
confinement and never names a platform.

Settled 2026-10-26 (S1, S3, S6, S7, S9, S10, S13, S17, S18, 0.2.1, 0.2.3);
all of it arrives with change 5, since change 4 runs no program.

- **Linux**: Landlock, applied by a re-executed single-threaded helper
  (`uze __confine -- <argv>`) that installs Landlock, then seccomp, then
  `execve`, never in a `pre_exec` of the multi-threaded client. Reading is
  allowed on the checkout and its Git common directory, the extension's
  Store directory, the per-run scratch directory, system prefixes (`/usr`,
  `/bin`, `/sbin`, `/lib*`, `/etc`, `/opt`, `/nix/store`, `/snap`),
  `/proc/self`, `/proc/sys/kernel/{random,osrelease}`, `/dev/{null,zero,
  urandom,random}` and the prefix of each pinned `exec:` path; nothing of
  `$HOME` outside those prefixes, and no other process's `/proc/<pid>` (a
  harness's environment holds API keys). Nothing of the checkout is
  writable; the only writable paths are `/dev/null` and a per-run scratch
  directory the runner creates and removes. The rest of `$UZE_HOME` is
  denied. Network is denied by an allowlist of socket families in a
  seccomp filter the helper installs on itself before `execve`: without
  `net`, `socket` is refused for every family and `socketpair` only admits
  AF_UNIX, `connect`/`bind`/`listen`/`accept*`/`sendto`/`sendmmsg`/
  `recvmmsg`/`get|setsockopt` are refused, and `io_uring_*`, `ptrace` and
  `process_vm_*` are refused in every mode; with `net`, `socket` admits
  AF_INET and AF_INET6 only, so a pathname Unix socket (`docker.sock`, an
  ssh or gpg agent, the systemd user bus, the WSL interop socket) is never
  reachable, and Landlock ABI ≥ 4 allows TCP connect and denies bind. On
  WSL, AF_VSOCK is refused in `socket` and `socketpair`, `WSL_INTEROP` and
  `WSL_DISTRO_NAME` leave the environment, and `/run/WSL` and `/mnt/*`
  (drvfs) are outside the read ruleset. The filter's prologue checks the
  audit architecture and kills on a mismatch. On Landlock ABI ≥ 6 abstract
  Unix sockets and signals are scoped to the domain; below it, seccomp
  limits `kill`, `tkill`, `tgkill`, `pidfd_send_signal` and
  `rt_sigqueueinfo` to the program's own group or itself. Enforcement is
  recorded and shown **per right** (`truncate` below ABI 3, TCP below 4,
  `ioctl` below 5, scoping below 6) with `CompatLevel::HardRequirement` for
  what consent promised, never a silent best effort. A device a program
  needs (`nvidia-smi`) is declared on its `exec:` grant as `reads` paths
  (`/dev/nvidia*`, `/proc/driver/nvidia`), listed on the consent screen,
  with `IOCTL_DEV` on ABI ≥ 5.
- **Resources** (S13): the helper is a subreaper (`PR_SET_CHILD_SUBREAPER`);
  `RLIMIT_CPU` is twice the time limit, `RLIMIT_FSIZE` 64 MiB on the
  scratch directory, `RLIMIT_NOFILE` 256, `RLIMIT_AS` 2 GiB; never
  `RLIMIT_NPROC`, which is per uid and would throttle the agents. A fork
  bomb is contained by the subreaper, a double kill of the group and a cap
  of two concurrent runs in all; a pids cgroup is used only where the
  subtree is already delegated, detected and never required. The run's
  `cwd` is the checkout root and its `TMPDIR` the scratch directory.
- **`exec:git` in a hostile checkout** (S9): the runner injects
  `GIT_CONFIG_PARAMETERS` (`core.fsmonitor=`, `core.hooksPath=/dev/null`,
  `core.sshCommand=`, `credential.helper=`, `safe.directory=<root>`) and
  `GIT_CONFIG_NOSYSTEM=1`, as `acquisition/git.rs` does.
- **macOS**: `sandbox-exec` with a generated profile: `(deny default)`,
  `(deny network*)`, no `(allow system-socket)`, no mach-lookup beyond the
  Codex base list; with `net`, `(allow network-outbound (remote tcp))`,
  UDP to `*:53`, and the mach services DNS and TLS need, Unix sockets still
  denied; `(allow signal (target same-sandbox))`; `(allow process-fork)`
  with the same rlimits.
- **Windows**: not enforced in change 5 (disclosed, below); a Job Object
  with `JOB_OBJECT_UILIMIT_*` (clipboard, desktop, atoms), kill-on-close,
  `ActiveProcessLimit` 32, a process memory limit and breakaway denied,
  over a clean environment. A restricted token or dedicated user (Codex's
  `unelevated`/`elevated`, Claude Code's sandbox runtime) is its own later
  change, `extension-confinement-windows`.
- **Checked on every run** (S18): the helper verifies `RulesetStatus` is
  `FullyEnforced` (else it reports right by right), that
  `prctl(PR_SET_NO_NEW_PRIVS)` and `SECCOMP_SET_MODE_FILTER` returned, and
  that `sandbox-exec` exited 0; any failure returns an exit code of its own
  to the runner, which marks the run unenforced and, if the record says
  `enforced_at_accept: true`, **refuses** the run and asks for consent
  again rather than running "as you" by surprise.
- **Unavailable** (old kernel, Landlock disabled, a container that blocks
  `landlock_*`, native Windows, macOS without `sandbox-exec`): programs
  still run with argv-only execution, a scrubbed environment and bounds.
  An extension with `exec:` grants may still be enabled, from UZE's chrome
  only, after a separate consent step with the fixed text of *Product
  decisions owed* ("This machine cannot confine programs. `<program>` runs
  as you…"); the record keeps `enforced_at_accept: false`, the identity is
  re-asked by digest on every update (D5), and the Extensions screen,
  `uze status` and `uze doctor` show "runs as you". No setting silences it.
- **Dependency**: the `landlock` crate, published by the Landlock LSM
  maintainers from `landlock-lsm/rust-landlock`, pure Rust with no C. It is
  judged by the dependency policy before it is taken. The fallback is
  hand-written syscalls over `libc`, which the workspace already has. The
  seccomp filter is written over `libc` (`prctl` plus a fixed BPF program)
  rather than taking a seccomp crate.

**`exec:` is a dependency pinned at enable time** (S3, S7). The host
resolves each `exec:<name>` on its own search path
(`harness_search_path()`: absolute entries, no network mount, never the
checkout, `$UZE_HOME`, a temporary directory or a WSL `/mnt/*` path),
canonicalises it, records it with the consent and shows it on the consent
screen (`gh → /usr/bin/gh`). A name not found leaves the extension
enablable with the source marked "`gh` not found", and the host resolves
again only when the operator asks ("Resolve again" on the Extensions
screen), never on a run. A run executes the recorded path with the argv
rebuilt from the accepted manifest by declaration id, each `{field}` one
whole element (S8, `--` required before a substituted value); a packaged
script runs as `<interpreter path> <script>` from the `exec:` grant its
shebang names, so `env` never runs; a packaged native binary is listed on
the consent screen as "ships a program of its own" and is executable only
itself; a relative program path resolves with `realpath` inside the
extension's Store directory or is refused. A fixed list of general-purpose
runners in `uze-core` (`sh`, `bash`, `zsh`, `python*`, `node`, `deno`,
`bun`, `ruby`, `perl`, `env`, `xargs`, `npx`, `uvx`, `docker`, `ssh`,
`git`, `make`) is consented to as "can run anything" and, without
confinement, re-asked by digest on every update. `exec:uze` is refused by
the check; the harnesses' own names are refused by `uze-application`
asking the integration registry (core names no harness). Landlock grants
execute on system prefixes and on the directory of a pinned path outside
them (`~/.cargo/bin`, `~/.nvm/versions/node/*/bin`, `/opt/homebrew`), with
`no_new_privs`: what the OS enforces is reach, not which binary, because
every real CLI executes helpers (`git` → `git-core/*`, `gh` → `git`).

**A checkout is a Git worktree root the workspace already knows** (S6): the
root of a task's checkout or the project's root, never a string an
extension supplied, canonicalised through `uze-git`'s `rev-parse
--show-toplevel` and `realpath`, and never `/`, a volume root (`C:\`,
`/mnt/c`), `$HOME` or an ancestor of it (a dotfiles repository in `~` is
not a checkout: the screen says "your home directory is not a checkout"),
`$UZE_HOME` or anything above or below it, a temporary directory, or a
bare repository (its `.git`, directory or pointer file, must be inside).
The runner and `read_checkout` check it on every run or call, not only
when a surface opens, so a checkout moved or replaced by a link while the
client runs is caught. `read:checkout` opens the root and the worktree's
Git common directory, read-only; `context.checkout` is the same canonical
string; `read_checkout` resolves beneath the root without following links
(D19) and refuses `..`.

### D7. Everything an extension produces is sanitized once, at the boundary

A single function in the leaf crate strips C0/C1 controls (except `\n` in
multi-line content), ESC-introduced sequences and bidi override/isolate
characters, and bounds the length of a label or a cell. The engine applies it
when an answer is absorbed and when the manifest is loaded, so nothing
downstream sees raw bytes. An architecture test asserts that the declared
engine builds `Span`s only through the sanitizing constructor.

Settled 2026-10-26 (S14), for change 4, where third-party answers begin:
the function becomes a `Clean` newtype in `uze-contract` that sanitizes in
`Deserialize`, so an unclean string cannot be constructed from an answer or
a manifest; it also strips format (Cf), zero-width and tag characters
(U+E0000–E007F), U+2028/U+2029 and combining marks stacked beyond two, and
bounds length in display cells (`unicode-width`), not bytes. The
constructor-grep test becomes "the declared engine builds `Span`s only
through `Clean`".

### D8. Keymap: one variant, a second table per extension

Rewritten 2026-10-26 (A4, R-U7). `uze-keys` keeps `Action` closed, `Copy`
and `const`-listed (`ALL_ACTIONS` lists the host's actions only, so none
of its ~730 uses changes); it gains one variant,
`Action::Extension(ExtensionActionId)`, an interned number in a
process-wide table (`uze_keys::extension`: `intern(name) -> id`,
`name(id)`) that maps it to its `keys.json` name,
`ext.<plugin>@<marketplace>/<id>.<action>` (the canonical identity of D5
followed by the action id), and to the label and description the manifest
gave, sanitized. `Scope::Declared` stays the one sealed scope of every
declared surface; what varies per extension is a **second table**,
registered when an extension is enabled and dropped when it is disabled
(as `extension_switch::opened_by` already drops the switch), in two parts:

- surface actions, live only while that extension's surface is in front,
  resolved after `Global` and before `Scope::Declared`'s generic
  vocabulary;
- `open` actions (R-U7), live over the pane and every surface, resolved
  after `Global`, `Workspace` and `Surfaces`, so they never shadow the
  host.

A suggested key (`key` in the manifest) is bound only when
`Keymap::resolve` answers `Fallthrough` for it in the stack the action
lives in (`[Global, Workspace, Surfaces, Declared]` for a surface action,
`[Global, Workspace, Surfaces]` for `open`); since only the extension in
front is consulted, two extensions with the same suggestion never collide
with each other, and the host always wins. An `open` chord two extensions
suggest goes to the first by identity and the other stays unbound, said in
`uze doctor` and on the Extensions screen beside the action. The
operator's `keys.json` binds or unbinds `ext.*` names like any other and
beats the suggestion; a mnemonic conflict inside one extension drops that
one binding with a `Problem` from `load`, never the load; a name of an
extension not installed is a warning, as a newer uze's names are. A
surface action may hold a bare letter (the surface seals, as `i` opens the
inspector today); an `open` must carry a modifier or a function key (the
`load.rs` test "a bare key the pane would never receive"). An action that
declares `confirm` (or is an agent action, change 6) is destructive: the
check refuses a bare-letter suggestion for it, which derives "destructive
never holds an unmodified letter" from the declaration. The footer, the
action index and item menus print the chord `chord_for` resolves, or offer
the action by pointer when there is none. `label()`/`description()` of an
`Extension(id)` come from the table.

Built-in extensions keep their own scopes and actions.

*References:* VS Code evaluates rules bottom-up with the user's file
appended last (host → extension → operator here, `when` is the scope);
Helix's sealed submodes are `Scope::Declared`; lazy.nvim's silent
last-wins is what is refused; Zellij plugins contribute no keys and
intercept input only as residents, which is the resident tier.

### D9. The catalog grows in `view.rs`, drawn by `src/ui/widget`

*Superseded in its list by D15; the placement rule stands.*

New view types:
- `Content::Markdown`, `Code { language }`, `Table`, `Details`, `Progress`,
  `Chart { Bar | Sparkline }`, `Log`, `Stack`
- a navigator item `badge`
- `RowMark::Status(Role)`

`code::markdown` moves to `uze-extensions/shared/markdown`, whose second
consumer is the declared engine (a third is `release_notes.rs`). Each new type
gets a renderer in `src/ui/extension_view.rs` built from the widget
vocabulary. Bar and sparkline glyphs become `uze_theme::Symbol`s, since no
glyph is written inline. Charts are only drawn from numeric fields; a
non-number is a notice, not a panic.

### D10. Lifecycle events come from what the host already observes

*Superseded by D17 and D16.*

No filesystem watcher and no new dependency:
- `checkout.changed` is derived from the change badge's periodic read
  (a digest of the status it already computes).
- `agent.started`/`agent.finished` come from the agent-activity signal.
- `work.named` comes from the work label.
- `install.converged` comes from the TUI's lifecycle operations.
- `rebase.paused` comes from the delivery flow.
- `theme.changed` comes from the theme switch.

Events are coalesced per extension and per frame.

### D11. No `uze ext`: an extension is a plugin capability

An extension ships as part of a plugin, so it is installed, updated,
removed, reported and checked by the verbs plugins already have. There is
no `uze ext` noun.

| Need | Where | Classification in `command_performance.rs` |
|---|---|---|
| see extensions and their state | `uze status`, `uze inspect <plugin>` (existing) | unchanged, Budgeted |
| enable, disable, accept grants | the Extensions screen; the `uze install` prompt on a TTY | unchanged |
| validate, run tests, update snapshots | `uze agent plugin check <path>` (existing), with `--run` and `--update-snapshots` | unchanged, JustifiedSlow |
| scaffold | `uze agent plugin create --extension <id>` (existing verb, new flag) | unchanged |
| editor support | `uze agent plugin lsp` (new leaf, started by an editor) | JustifiedSlow (long-running) |

Two capabilities of the first draft leave this change, each until a gallery
entry needs it, because none does today and each carried a security finding:
- **commands an extension adds to the CLI** (`uze ext <id> <command>`;
  S11's raw output and M7's mixed scopes);
- **pipes** pushing data in from outside (D17; R-S6's confused deputy).

The finding text stays in *Unresolved review findings*, moot while the
capability is out.

### D12. A dedicated Skill, `uze:extension`, linked from `uze:author`

`uze:author` is the plugin packaging loop (marketplace → scaffold → check →
install → publish). Writing an extension has its own subject matter: the
catalog, bindings, the program contract, grants and what confinement costs,
and the check loop against real answers. Folding that in would roughly double
`author`. So `author` gains one step ("adding an extension? use
`uze:extension`") and the new Skill carries the rest. Its examples are the
openspec board and the scaffolded one.

### D13. Three tiers behind one manifest, chosen per surface

A surface's `feed` says how it is fed, and the tier is what that feed
costs:

| Tier | `feed` | Resident | Answers | Extra grant |
|---|---|---|---|---|
| declared | `bind` or `compose` | no | data bound by the manifest, or a catalog tree (D15) | — |
| cells and media | `cells`, `image`, `sprite` | no | a cell grid with regions, an image, a sprite sheet | — |
| resident | `resident` | yes, the extension's script kept alive | catalog trees or cells, every frame it asks for | `resident` |

- Tiers share the manifest, the grants record, the keymap, the slots and
  the sanitizer. Moving a surface up a tier is changing its `feed`, not
  rewriting the extension.
- The first two tiers keep the contract of D4: argv, one question, one
  answer, nothing left running. The third is D19.
- `kind: interactive` is retired: the resident tier is what it reserved.

*Alternative:* one tier, resident for all. Rejected: most extensions are a
list over a CLI, and making each one a resident script would trade the
`sh` author for nothing.

### D14. Slots: the places an extension may occupy

| Slot | Where | Size | Notes |
|---|---|---|---|
| `view` | the full frame, opened like `code` | the frame less the host's attribution strip | layouts `sidebar` or `board`, as before |
| `section` | a collapsible block of the workspace sidebar | sidebar width, rows the host grants | the timeline's place |
| `drawer` | a card docked on the right of the agent area, toggled by its strip button or `alt+k` | 45% of the agent area, 44 to 72 columns, draggable; the pane keeps at least 40 | laid beside the pane, so the pane and its PTY narrow on open; not modal: a click lends it the keys, Esc gives them back; one open per space (added by `extension-view-engine`) |
| `status` | a compact row in the sidebar's header | one row, sidebar width | no workspace status bar exists (the sidebar layout in `src/ui/render.rs`), and one would shorten every pane; the operator orders segments |
| `band` | a fixed number of rows (1 to 6, declared in the manifest) along the top or bottom of an agent's pane | pane width × declared rows | reserved in `compute_layout`; never resized by an answer |
| `overlay` | a picker, form or dialog over the frame | host-chosen | modal, under host chrome, opened by a gesture or the `open` action |
| `notify` | an OS notification or the terminal bell | — | rate-limited, prefixed with the host's attribution |

- The host owns geometry. The program receives its slot's size in its
  context (D4) and answers what fits; what still does not fit is folded,
  not cropped. An empty answer from a `band` or `status` draws nothing and
  keeps the space (bands) or frees it (status).
- A `band` belongs to one agent: its pane's. Its rows are fixed so an answer
  never resizes the agent's terminal (a resize is a SIGWINCH and hides real
  activity for `AGENT_REDRAW_GRACE`); mouse and selection coordinates of the
  pane are offset by the band.
- A `view` and a `drawer` keep a one-row strip the host draws
  (`id · plugin@marketplace`) that no answer can cover, so a surface cannot
  pass for an agent's pane or for UZE.
- No slot reaches a harness's own output. Rewriting a transcript row is a
  non-goal.

### D15. The catalog in families, bound or composed

The catalog is the vocabulary of `view.rs`, grown in families. Every node is
a meaning the host draws in the active theme; colour is a `Role` (the
twelve of `view.rs`), glyphs are `Symbol`s, emphasis is `strong`, `muted`,
`code` or `struck`. `Span.color: Option<Rgb>` is refused for declared
extensions.

Nodes are named below as concepts; a view spells them in PascalCase
(`Stack`, `Progress`, D22), and a compose answer's `type` uses the same
names.

| Family | Nodes |
|---|---|
| layout | `stack` (vertical), `row` (horizontal, weighted), `split` (two panes, a draggable edge), `tabs` (host-held choice), `gap`, `rule` |
| text | `text` (spans with role and emphasis), `markdown`, `code` (language), `log` (lines with a tone) |
| data | `list` (flat, grouped, tree), `table` (named columns; a cell may hold `text`, `badge`, `marks` or `pips`), `details` (key/value), `grid` (tiles) |
| measure | `progress` (single or segmented, each segment with a weight, a ratio and a role), `gauge`, `pips` (done of total), `chart` (bar, sparkline over a series in the answer or the store), `heatmap` |
| status | `badge`, `marks` (several role-coloured marks in a row), `chip`, `activity` (a mark the host animates while a named agent or run is busy), `elapsed` (a duration the host ticks) |
| media | `image`, `sprite` (D13; formats in D4) |
| cells | `cells` (a grid of `{ch, role}` with clickable `regions`), tier 2 and above |
| input | `button`, `toggle`, `select`, `field` (text), `form` (typed fields), `picker` (fuzzy list the host filters) |

As built so far: `extension-view-engine` shipped `Stack`, `Row`, `Tabs`/`Tab`,
`Text`, `Log`, `List`, `Table`, `Progress`/`Segment`, `Pips`, `Badge`,
`Marks`/`Mark`, `Activity`, `Elapsed`, `Toggle` and `Button`;
`extension-catalog-growth` added `Rule`, `Details`/`Entry`, `Markdown`,
`Code`, `Gauge` (with `warning`/`danger` thresholds), `Chart` (`sparkline`
and `bar`), `Grid`, and two the table did not name, `Stat` (a figure with
its label and delta) and `Kbd` (a key, as the workspace draws its own).
Still to come: `Split`, `gap` as a node, grouped and tree `List`, `Heatmap`,
`Chip`, the inputs past `Toggle`/`Button`, media and `cells`.

Feedback is not a node. A composed tree or a reply *requests* `toast`,
`notice`, `notify` or `sound` (a cue from a fixed list) through its answer
(D4), and a confirmation comes only from an action's declaration; the host
draws all of them as chrome, attributed.

Two ways to build a surface from it:
- **bind**: a view file (D22) names nodes and fills them from a source's
  data under the binding rules of D23. No logic.
- **compose**: a source answers a tree of catalog nodes as JSON (output mode
  `view`, D4). The logic lives in the program; the host validates the tree
  against the catalog and a node cap when the answer is absorbed, sanitizes
  every string, and draws it.

Composed interaction is closed by construction:
- every interactive node names a declared action and an `item`: an object
  from the same answer that becomes that action's selection, so `{field}`
  references in the action resolve against it exactly as against a
  navigator's selected item;
- the action's confirmation shows the resolved arguments, never only the
  manifest's body;
- a click within 300 ms of a re-layout of that node is ignored.

Animation is the host's for every stateless tier: `activity`, `elapsed`,
`sprite` and segmented `progress` transitions are driven by the host's
clock, so no program runs to make something move. A composed view is built
into a `View` once per (answer digest, size) and memoized (M5 of the
second review).

Compose arrives with change 6 (decided 2026-10-26, with R-S11): `bind`
plus `each` over `Segment` already draws `gsd-band`'s per-phase bar, and
no entry of change 4's gallery needs a tree the manifest cannot express.

The catalog grows by the rule `view.rs` already states, made concrete: a
node joins when a gallery entry (D20) needs it. D9's list is superseded by
this table.

*Alternative:* bind only. Rejected: the gsd-band board needs a per-phase
segmented bar and a status per row whose count the manifest cannot know
ahead, which a template cannot express without becoming an expression
language. *Alternative:* compose only. Rejected: a list over a CLI should
not need a program that prints UI.

### D16. The agent bridge

**Reporting channel.** Nothing today lets a process in a pane tell the
workspace client anything but "open a space" (ADR-056; `OpenSpace` in
`crates/uze-terminal/src/protocol.rs`). The bridge adds the channel
the codebase already uses for work labels: a pane process proves itself with
`UZE_AGENT`/`UZE_AGENT_KEY` and writes a record the client polls.
- `uze agent report <event> [fields]`, verified by the agent key, appends to
  a bounded per-agent journal named in `UzeHome`; the client's one-second
  watch (`src/ui/orchestrator/reads.rs`) reads it. Where a harness
  sandbox denies `$UZE_HOME` (ADR-056's advice), the journal is unwritable
  and the bridge reports that agent's events as unavailable.
- The reporter is delivered **per launch**, through
  `IntegrationPort::runtime_contribution` (`extra_args`/`extra_env`), not as
  a portable hook: a portable hook is installed for every session of the
  harness, including outside UZE (ADR-040). The portable vocabulary
  (`PreToolUse`, `PostToolUse`, `Stop`, `SessionStart` in
  `capability/hook.rs`) grows where a harness offers more: turn start and
  approval asked.
- The reporter is observational and fails open: it never blocks a turn.

**Events are claims.** Any process in the agent's pane inherits its key and
can report. So an `agent.*` event is drawn and may refresh sources, but
nothing that writes to a pane, the clipboard or a path is ever released by
one. The paint heuristic still gives turn start and end where no hook
exists.

**Events and their payload.** `agent.session.started`,
`agent.turn.started`, `agent.turn.finished`, `agent.tool.started`,
`agent.tool.finished` (with a call id, start and end time and outcome),
`agent.approval.asked` and `agent.approval.resolved`, each carrying the
agent's id, harness and checkout. A tool event carries the tool's name and a
checkout-relative path when it has one, never its input or output, after
the credential scrub of `project/memory/credentials.rs`. Subscribing to
agent events or reading the `agents` source needs `agent:observe`.

**Read model.** `agents` is answered by the workspace client, which owns
the activity heuristic (`src/ui/orchestrator/agents.rs`), not by
`uze-application`: id, harness, state (`working`, `idle`, `waiting`,
`unknown`), work label, checkout, branch, turn start, and usage in a typed
shape where the harness reports it. Its scope is the workspace's open
spaces, so `jump` can cross checkouts.

**Writing to an agent is typing.** UZE writes through the path it already
has, `submit_notice` gated by `may_submit` (`absorb.rs`,
`orchestrator/agents.rs::may_submit`, answering Busy, Typed or Unknown),
never the operator's key forwarder. What
it writes reaches the harness's input, where `!`, `/`, `#`, `@` and a `y`
in an open approval are actions. So:
- the text is filtered strictly: no control, Cf, zero-width or tag
  characters; no leading `!`, `/`, `#` or `@` unless it is the
  integration's own spelling of a delivered skill;
- what was shown is what is sent, byte for byte, under one cap for both,
  the full text shown scrolled;
- a write is refused, with the reason, when `may_submit` is not `Ready`,
  when the harness is not the pane's foreground process, or when an
  approval is pending. Nothing is queued (ADR-059): the extension may try
  again on a later event.

**Actions.**
- `agent.prompt`: a text, or a skill invocation. The integration spells it
  through a new method beside `invocation_prefix` (documented as
  presentation-only today) and `packaged_exposure_name`; a skill the
  harness lacks is refused before anything is written. Skills a harness has
  natively (not delivered by UZE) are named by the integration's own list.
- `agent.restart` (the harness's new conversation) always confirms, since it
  ends the operator's conversation. `agent.interrupt` is deferred: no
  integration has the concept yet.
- Every agent action names its `target`: `self` (the band's agent, or the
  agent of the item), or a field reference to an agent id from the
  `agents` source. There is no broadcast.

**Grants.**
- `agent:prompt`: from a gesture, after the host shows the exact text, the
  target agent and the extension.
- `agent:prompt:auto`, asked on its own after the rest, per project and per
  agent, for 24 hours from acceptance, never machine-wide: an event or a
  timer may send only the manifest's **fixed text**, or a declared skill
  with arguments from a **closed set** the manifest lists. Nothing from a
  source's answer, an event payload or the repository becomes prompt text.
  Each write counts against the rates of *Product decisions owed*
  (decided 2026-10-26, enforced by the host in `submit_notice`, never by
  the extension), is attributed in the band or a notice, and pausable with
  the host's pause key, which a resident surface cannot capture and which
  resets every counter until resumed. A "turn" is the bridge's
  `agent.turn.finished`, or the paint heuristic's end where no reporter
  exists; while the pane is not `Ready` nothing is sent or queued
  (ADR-059); the excess is dropped with an attributed notice. Any change of
  the extension's digest revokes it, whatever D5 says of grant sets.

*Alternative:* no agent actions. Rejected: the gsd-band's value is its next
action button, and every harness that allows it (mods, OpenCode, the Codex
app-server, Antigravity sidecars) is used for it. *Alternative:* a queue for
a busy pane. Rejected by ADR-059.

### D17. Reasons to refresh, and pipes

D10 is superseded by this decision; tasks 7.x are folded into group 15.
- `every: <duration>` runs the sources it names on an interval, with a floor
  of 5 seconds, only while a surface that shows them is visible.
- `paths.changed: [<paths>]` runs them when a declared path inside the
  checkout changes, from the client's existing one-second watch (the
  watch in `src/ui/orchestrator/reads.rs`) extended with a bounded digest
  of the declared paths' modification times. It covers paths Git ignores.
- `checkout.changed` comes from that same watch, per checkout an open
  surface is on.
- `agent.*` comes from the bridge (D16), not from repaints.
- **Delivery.** An event mapped to a source reaches its program in
  `input.event` (D4). A declaration chooses `deliver: latest` (the default:
  events are coalesced per extension in a 250 ms window and the program
  sees the last) or `deliver: each` (each event runs the program once, in
  order, bounded per window; the excess is counted and reported in the
  next input as `dropped`). A built-in source, `events`, answers the last
  N events the extension may see, so a log can be drawn without a store.
- **Pipes are out of this change** (D11). If a gallery entry needs data
  pushed from outside, they return with the rules the second review set:
  a sender on every message, refresh only, named senders for messages from
  other extensions.

### D18. Store and preferences

Settled 2026-10-26: the store arrives with change 5 (`gpu-meter` is the
first entry that needs it), preferences with change 6 (`agents-hud`'s
`{pref.*}`), and the `secret` type with `net` in change 8, since a secret
without a network call has no use.

- **Store** (R-S13): a key/value document per (extension identity, project),
  bounded at 1 MiB, unless the manifest declares `store: global` and a
  consent of its own says so. A program receives it in `input.store` when its
  declaration names it, and any answer may carry a `store` patch in the
  envelope (D4); the host applies patches in arrival order, last write per
  key wins. It is extension state, so its path is named in `UzeHome` in the
  tier the `persistence-tiers` rule gives state an operator would miss, and
  it is removed with the plugin.
- **Preferences**: typed fields the manifest declares (`text`, `number`,
  `toggle`, `choice`; `secret` from change 8), edited on the Extensions
  screen, passed in `input.preferences` only when the declaration names
  them, and readable by bind templates (`{pref.name}`) except a `secret`.
- **Secrets** (R-S8) are kept by `uze-platform` in the OS keychain where
  there is one, with no new crate: Windows Credential Manager through the
  `windows-sys` already in the tree, macOS Keychain through the system's
  `security` by path or the framework, and on Linux a `0600` file under
  `state/extensions/secrets/` (Secret Service would bring `zbus`), outside
  every path an agent's pane is told about; never in
  `state/extensions.json`. A secret is never shown again, logged, piped,
  part of a cache key, or sent to an agent. An answer from a run that
  received a secret may not patch the store or carry an `agent.prompt`
  (tainted), and its strings are checked for the secret before they are
  drawn.

### D19. The resident tier: a Rhai script that keeps its state

A resident surface is fed by the extension's script (D27) kept alive for as
long as the surface is open: the same engine, the same grants, with a scope
that survives between calls.

- **Entry points** the script defines: `start(context)`, `event(event)`,
  `input(input)` (keys and pointer while its surface is focused), `render(slot,
  size)` answering a catalog tree or cells, and `stop()`. `event` and `input`
  answer whether to redraw. Timer ticks and `every` arrive as events.
- **Host functions are the grants**, registered per extension and nothing
  else: `read_checkout` and `list_checkout` with `read:checkout` (paths
  resolved beneath the checkout root, no link followed), `run_program` under
  the grants of the program it runs and the same rate and concurrency limits
  as declared sources, `fetch` with `net`, `agent_prompt` with `agent:prompt`
  (always raising the host's confirmation unless `agent:prompt:auto` holds for
  that agent, with D16's rules), and `store`, `clock`, `timer`, `notify`. A
  script calling what its grants do not cover finds no such function, and the
  check refuses a script that names one.
- **No way out of the engine.** The engine is built raw with only the
  packages UZE chooses; `import` has no module resolver (a script cannot load
  another file), `eval` is disabled, and `print` and `debug` go to the
  inspector, never to a terminal.
- **Budgets**: a maximum number of operations per call, maximum call depth,
  expression depth and string, array and map sizes, and a progress hook that
  stops a call past its deadline; at most one render per frame the host asks
  for and 30 per second. A script that exceeds a budget is stopped and its
  surface shows which. It runs on the job pool (D28), never on the drawing
  thread; a render that misses its frame keeps the last one.
- **Confinement** is the engine's, on every platform: a script reaches only
  the functions registered for it. `run_program` is confined only where D6
  is, and the Extensions screen says so when it is used.
- **Input**: a focused resident surface receives raw keys and pointer except
  the host's escape chord and the pause key. This amends the extensions rule
  that an extension knows nothing of the keyboard, for this tier only.
- **What runs is what was consented to.** Scripts are packaged with the
  plugin, stored and digested like any other bytes; UZE compiles them at
  load and never builds anything at install.
- **Where it lives**: a dedicated crate, `uze-script`, depending on
  `uze-contract` and `uze-platform` only, wrapped by an `Extension` adapter
  on the host side. Neither `uze-core` (the package manager, which the shim
  uses) nor `uze-application` (which does not depend on `uze-extensions`)
  hosts it.
- **Dependency** (decided 2026-10-26): `rhai`, published by the
  `rhaiscript` organisation (13.5M downloads, 4.4M in the last 90 days;
  1.27.0 released 2026-10-21, about one release every one to three months),
  MIT or Apache-2.0, MSRV 1.66, pure Rust: with `default-features = false,
  features = ["std", "sync"]` it brings about twenty crates and no C
  (`ahash`, `smartstring`, `smallvec`, `thin-vec`, `num-traits`,
  `once_cell`, `bitflags`, a proc macro over `syn`). It does not forbid
  `unsafe` and uses it in five places (dynamic downcasts and function
  registration), which the PR states. `sync` is required because a `Job`
  is `FnOnce + Send` on the pool (D28). The engine is `Engine::new_raw`
  with the packages UZE chooses, `set_max_operations`,
  `set_max_call_levels`, `set_max_expr_depths`,
  `set_max_string_size`/`array`/`map`, `set_max_modules(0)`, a
  `DummyModuleResolver`, `disable_symbol("eval")`,
  `set_strict_variables(true)`, `on_print`/`on_debug` to the inspector and
  `on_progress` as the deadline; `unchecked`, `internals` and `debugging`
  are never enabled. Rhai bounds values, not memory: the per-extension
  measure of D28 is the memory bound. `uze-script` is the only crate that
  names `rhai`, and the architecture suite says so. Alternatives: Lua,
  Luau and QuickJS compile C; `starlark-rust` and `rune` have one
  maintainer and no operation counter; WebAssembly was rejected in D27.
- **`fetch`** is owed with the change that ships `net` (change 8, when a
  gallery entry needs it; none of the ten does), and ships without a TLS
  crate: no pure-Rust TLS stack passes the dependency rule (`rustls` needs
  `ring` or `aws-lc-rs`, both C; `native-tls` is OpenSSL on Linux), so
  `fetch` is a host function over `uze_platform::tools::curl()`, the
  system's `curl` by path with `-q` and a scrubbed environment, the one
  HTTP client UZE already trusts for its updater, run through the program
  runner under the run's bounds and the `net` grant.

*Alternatives:* a WebAssembly component (see D27). A resident process
speaking JSON over stdio: confined only where the OS confines, and a process
per extension.

### D20. The gallery

Ten reference extensions, each recreating a plugin operators use on a
harness, live as fixture plugins and are the acceptance suite of the
platform. Each names the primitives it needs; together they cover every
family, slot and tier. The openspec board is dropped as a proof: the
built-in `spec` extension already reads OpenSpec, Spec Kit, Superpowers and
GSD.

| Reference | Recreates | Exercises | Tier |
|---|---|---|---|
| `gsd-band` | luizfbalves/claude-gsd-band | `band` (6 rows), composed segmented `progress`, `table` with `marks` and `pips`, `paths.changed`, `every`, `agent.prompt` of a native skill with `target: self` | declared |
| `agents-hud` | claude-hud, ccstatusline | `status`, the `agents` source, `activity`, `elapsed`, `gauge`, `{pref.*}` | declared |
| `flightdeck` | claude-flightdeck, opencode-agents-monitor | `view` and `section`, agent events with `deliver: each`, the `events` source, `tabs`, `table`, `focus_agent` | declared |
| `gpu-meter` | opencode gpu-sidebar | `section`, `every`, `chart` sparkline over the store, `exec` | declared |
| `chimes` | peon-ping, opencode-notifier | agent events, `notify` and `sound` requested in an answer | declared |
| `jump` | zellij room | `overlay` `picker` over `agents` across spaces, `open`, a global key, `focus_agent` | declared |
| `plan-review` | plannotator | `view`, `markdown`, `form`, `agent.prompt` of the review on a gesture | declared |
| `pets` | Codex pets, claude-code-tamagotchi | `band`, `sprite` by agent state, `store` | cells and media |
| `screen` | opencode-mobile-use | `section`, `image` on `every`, `open` to a full-size `overlay` | cells and media |
| `snake` | claude-code-mods snake | `view`, `cells` every frame, raw keys, `timer`, `store` | resident |

What the recreations knowingly lose, so the gap is a decision and not a
surprise: plannotator's answering of the approval gate (the bridge reports,
it does not decide); peon-ping's voice packs (cues are a fixed list); the
original band's 11 rows (6 here); a device screen at 4 fps (5 seconds
here). Not recreated: tdd-guard and ctx-handoff are hooks and skills, which
plugins already deliver; prismantis rewrites a transcript (non-goal);
reels streams video at a cost the gallery does not need to prove.

### D21. The manifest is a schema with worked examples, and runs without a TUI

- The compose wire grammar is written once, as the JSON Schema generated
  from `uze-contract` (task 3.5); the manifest, views and tests are described
  by the catalog the LSP serves (D26), with one complete worked
  example per slot and per feed in the reference page and the Skill. What a
  reviewer had to invent (how a surface names its source, a band's edge and
  rows, a target, the `open` action, a timeout) is fixed there.
- Running one declaration against canned inputs and printing what the host
  would draw is a test (D26), run by `uze agent plugin check`; there is no
  separate simulate command.
- A packaged script with an interpreter shebang declares the interpreter
  as `exec:<name>`; consent names it as "can run anything", per S3.

### D22. Manifests, views and tests are KDL v2

Decided by the operator on 2026-10-23, after three reviews of a KDL
proposal (see *Third review*). Nothing an author writes evokes the web, so
no one reaches for CSS, a module or a web framework.

- **One format per extension.** The manifest is
  `extensions/<id>/extension.kdl`, its views are `.kdl` files named by their
  surfaces (`view="board.kdl"`), and its tests are `tests/*.test.kdl` (D26).
  TOML was chosen first for its ready completion; once the LSP became
  required work (D26), a TOML manifest was only the odd file out, and
  longer (about 25 lines against 17 for the gsd-band manifest).
- **Naming.** Catalog nodes, the components and primitives a view draws, are
  PascalCase (`Stack`, `Row`, `Text`, `Button`, `Progress`, `Segment`), as
  in Slint, QML and SwiftUI and unlike HTML. Everything else is lowercase:
  a view's directives (`template`, `use`, `lookup`), the manifest's nodes
  (`source`, `action`, `surface`, `wants`) and test steps (`given`,
  `press`). Properties are kebab-case (`min-width`, `applies-when`,
  `refresh-on`). KDL is case-sensitive, so the check refuses the wrong case
  with a suggestion (`stack` → `Stack`).
- **A view's node names are exactly the catalog's** (D15) and its
  properties only those each node accepts. `compose` keeps
  JSON (D4): KDL is never on the path of an answer.
- **KDL v2 only.** No v1 fallback; the check recognises v1 spellings (bare
  `true`, `null`) and says how to write them in v2. Authors are told that
  Zellij layouts and some editors' grammars are still v1.
- **Parsing is bounded and never in the drawing process's hot path.** The
  `kdl` parser recurses without a depth limit (400 nested braces overflow a
  thread's stack, uncatchably) and has an open exponential case with
  slashdash (kdl-rs#177). So a manifest, a view or a test is parsed only by
  `uze-core` (tests by `uze agent plugin check`), at check, install, enable and a
  linked extension's reload, after:
  - a size cap (64 KiB per file);
  - a KDL-aware pre-scan (strings, raw strings, comments, `/-`) refusing
    nesting deeper than 32;
  - parsing on a worker thread with an explicit stack size and a time
    limit, the view refused on either.
- **The decoder is written by hand** over `KdlDocument` with spans, not
  through the crate's experimental serde bridge, which mis-decodes single
  nodes and loses positions. It refuses type annotations, repeated
  properties (KDL keeps the rightmost, so `target=self target=all` would
  resolve silently), unknown nodes, unknown properties and unexpected
  argument counts (which catches `text {id}` and two siblings on one line),
  each with file, line, column and a suggestion.
- **What the client draws from is a compiled view**: the validated tree,
  stored in the generated tier keyed by the view's digest. `uze-extensions`
  and `src/ui` never see KDL.
- Every decoded string passes the sanitizer (D7) after escapes are applied,
  including raw and multi-line strings; ids and names are matched exactly
  against the catalog and the declared sets, with S15's charset.
- A manifest's `id` is the name of its directory under `extensions/`, so
  the files and the identity cannot disagree; while sources are fixtures
  only (`extension-view-engine`), every source names one.
- The dependency is `kdl` with `default-features = false, features =
  ["span"]`, in `uze-core` (`capability/extension`), never in `uze-contract`,
  `uze-extensions` or `src/ui`. The PATH shim is the same binary, so the
  cost there is binary size only: no KDL code runs on the shim's path.
  Judged by the dependency policy: one maintainer (the
  spec's author), Apache-2.0, no C, MSRV below the workspace's, and a
  duplicate `winnow` until its next release.

*Alternatives:* YAML with JSON Schema for views (completion everywhere on
day one, a hardened parser) — rejected for a verbose tree, fragile
indentation and `{field}` colliding with flow maps. A TOML manifest
(completion through Taplo on day one, a mature parser on the file that
holds grants) — rejected for being a second format beside views and tests
that the LSP covers anyway; the cost accepted is no completion on the
manifest until the LSP ships, which is why it ships with the views
(split 3).

### D23. Binding rules

One set of rules, the same in every view, with no expressions:

- **References.** `{field}`, `{a.b}`, `{.}` (the whole current item) and
  `{^.field}` (the enclosing item of a nested `each`). Scopes: the current
  item, `form.`, `pref.` (never a secret), `context.`, `ui.` (D24), and a
  source's name when a view reads more than one (`sources = ["a", "b"]`).
- **Types.** A property that is exactly one reference keeps the JSON type
  of what it refers to (number, bool, list); a property with text around a
  reference is a string, interpolating scalars only. `{{` and `}}` are
  literal braces.
- **Where interpolation may go.** Display text only. Identity-bearing
  properties (`id`, `action`, `target`, `slot`, `view`, `each`, `use`) are
  literals, checked statically.
- **Repetition.** `each="list"` names one list field: no filter, sort or
  slice (the host's `filter` and the program cover those). `each` may sit on
  a container or on a repeated child (`segment`). Nesting is capped at 2,
  and expansion counts nodes against the node cap of compose (task 13.3),
  folding with a notice when reached.
- **Presence.** `when="field"` and `unless="field"` test presence: absent,
  `null`, `false`, `""`, `0` and `[]` are false. No negation inside, no
  and/or, no comparison against answer data; nesting gives "and".
- **Host state.** `show="ui.tab=phases"` compares a host-held UI value (D24)
  with a literal that the same view's `tabs` or `select` declares, checked
  statically; `show-any` takes several such pairs for "or".
- **Templates.** `template "name" { … }` and `use "name"`: no parameters,
  resolved against the innermost item; the graph of `use` must be acyclic,
  refused at check otherwise.
- **Lookup tables.** `lookup "tone" { success "done"; warning "partial"; … }`
  declares a mapping from a value to a role, a label or a symbol, used as
  `role="{tone:state}"`. Keyed by locale, the same mechanism carries labels
  in several languages (`lookup "t" locale="pt" { … }`), so programs stop
  emitting role names and built-in sources become usable. `{t:field}` takes
  the entry for the field's value; `{t::key}` the entry for a literal key,
  which is how a view's own labels are written (`"{t::phases}"`).
- **One condition per node.** A node carries at most one of `when`,
  `unless`, `show` and `show-any`; `show-any` lists its pairs separated by
  commas. Nesting gives "and".
- **Missing fields.** A missing field that is a whole property hides the
  node; inside a mixed string it draws empty and is a check warning against
  the fixtures (D26).
- **Text.** A `Text` with `truncate` is one line cut with an ellipsis; one
  without it is a paragraph that wraps, breaking a word longer than a row
  after its last `/`, `_`, `.` or `-` before cutting it at the cell.
- D3's and D15's "a reference is a whole value" is replaced by these
  rules, and the manifest keeps the original, stricter form: whole-value
  references only.

### D24. UI state the host holds, and input a shell can read

- **Declared state.** `id` on `tabs`, `toggle`, `select`, `field` and a
  list's selection makes its value host state, readable in the view as
  `ui.<id>`. `show`, `show-any` and a list's `filter="ui.<id>"` act on it
  with no program run. The host keeps it with the view's place.
- **Forms live in the view.** A `form` node holds its `field`s; an action
  bound to its submit receives `form.<id>` values. A field's id may carry
  the current item's key (`id="comment.{id}"`), so a form can repeat a field
  per item.
- **Actions declare what they read**: `with = ["ui.tab", "form.*"]`. Only
  those reach the program.
- **Input without `jq`.** A declaration may substitute declared flat fields
  into argv (`run = ["./bin/x.sh", "--", "{event.tool}"]`, each a whole
  argument, S8 applied) or receive them as `UZE_IN_<NAME>` environment
  variables. The JSON object on stdin stays, for programs that want it.
- **Two-step actions.** An action may `run` a program and then send
  `agent.prompt` with a field of its answer (`text = "{answer.prompt}"`),
  under D16: from a gesture, the confirmation shows the final text; never
  unattended, since that text is not fixed.

### D25. Live development is a linked extension, not a mode

There is no `uze ext dev` and no development mode. Authoring already has a
path in UZE: a plugin in a **linked marketplace**, whose reads follow the
working tree rather than commits (`marketplace-subdirectory`,
`plugin-authoring`). An extension installed from one is the extension under
development; nothing about it runs differently from an installed one.

The author's loop, end to end:

1. `uze agent plugin create --extension <id>` scaffolds the extension inside
   a linked marketplace (creating one if there is none): manifest, a view, a
   fixture and a passing test. It installs the plugin into the current
   project, and the operator enables it from UZE's chrome, never from an
   agent's pane.
2. The author edits the files; the workspace reloads the extension on save.
3. The inspector, opened by a key on any extension's surface, shows why the
   surface looks as it does and lets the author provoke what has not
   happened yet.
4. `uze agent plugin check` runs the same tests headlessly for CI (D26).

- **Reload on save, for linked extensions only.** The workspace polls the
  directories of extensions installed from linked marketplaces every 250 ms
  (they are a handful of small files; no watcher, no new dependency) and
  reloads what changed, debounced by 150 ms. An extension from any other
  marketplace cannot change under the workspace, so nothing watches it.

  | Changed | The host |
  |---|---|
  | a view | recompiles it (D22) and redraws from the last answers, running no program |
  | the manifest | rechecks and re-registers slots, keys and events, keeping the view's place; a widened grant set waits for consent in the chrome |
  | a script or program | invalidates its answers and reruns the visible sources |
  | a fixture | redraws from it, when the source is fed by it |

- **The same grants and confinement as any extension.** A linked extension
  is consented to by grant set (D5), so a save asks nothing unless it widens
  the set.
- **An error never blanks a surface**, for every extension, linked or not:
  the surface keeps its last good draw and shows the error over it, with
  file, line, column and the suggestion. For an installed extension this is
  the same guarantee against a broken update.
- **Fixtures are data, not a mode.** A source may declare `fixture
  "fixtures/board.json"` beside its `script` or `run`. A source with only a
  fixture is fed by it (the only kind before the runner exists, split 1);
  for a source with both, the inspector of a linked extension switches it to
  the fixture, and the switch lasts until switched back or the session
  ends. An action whose answer would run nothing yet (no runner, or a
  fixture-fed source) is recorded in the inspector with the arguments it
  would receive. An extension from a non-linked marketplace never reads its
  fixtures at runtime; they serve its tests.
- **The inspector is the workspace's, for every extension**, as a drawer:
  - *runs*: each run's input, output, stderr, exit, duration and the bound
    hit, rerunnable with edited input on a linked extension;
  - *events*: received, coalesced or dropped, and, on a linked extension,
    *emit* to deliver a synthetic one (marked synthetic everywhere it is
    shown);
  - *state*: `ui.*` values and the store;
  - *grants*: what was denied and why, and whether confinement is enforced;
  - *tree*: the compiled view and why a node is absent (`when` false, field
    missing, `show` not matched);
  - *cost*: runs per minute, p95 and memory (D28);
  - on a linked extension, the slot's size and the theme can be changed, fake
    agents injected, and a session's real events recorded into a fixture.

  An operator uses the same drawer to see why an installed extension is
  empty or slow.

*Alternative:* a `uze ext dev <path>` session mode loading a loose folder.
Rejected: a second lifecycle (not stored, not pinned, session-only, marked
`dev`) beside the one linked marketplaces already give, and an extension
that would behave one way in development and another once installed.

### D26. Tests without a TUI

- `uze agent plugin check <path>` runs `tests/*.test.kdl` headlessly, on the host's
  renderer with a test backend (the way UZE tests its own surfaces):

  ```kdl
  test "redeploy confirms and runs with the right service" {
      given source="recent" answer="fixtures/recent.json"
      size 38 8
      theme "dark"
      select "api"
      press "r"
      expect-confirm "Redeploy api?"
      accept
      expect-run "redeploy" args="--" "api"
      expect-refresh "recent"
      snapshot "board-dark.snap"
  }
  ```

- Snapshots are cell grids per slot, size and theme; `--update-snapshots`
  rewrites them.
- `given … answer=` uses a recorded answer, so a test runs nothing and works
  in any CI; `--run` executes the programs under confinement, and only
  where confinement is enforced unless the operator says otherwise (S12).
- Testing is checking: a plugin with a failing test does not pass
  `uze agent plugin check`, which is the same gate an install runs.
- **The LSP is required work**, not a later nicety: `uze agent plugin lsp` is
  generated from the catalog in `uze-contract` and gives completion, hover
  and the check's diagnostics for manifests, views and tests, since no
  editor completes KDL from a schema.
- **KDL is highlighted inside UZE.** The `code` surface's editor and
  preview, and fenced `kdl` blocks in Markdown, colour KDL through the same
  `shared/highlight.rs`. `two-face`'s bundle (bat's grammars) has no KDL, so
  a KDL v2 Sublime grammar is vendored into `crates/uze-extensions/grammars/`
  with its licence beside it, exactly as `PowerShell.sublime-syntax` is, and
  mapped to `.kdl` (which covers `extension.kdl`, views and `.test.kdl`). A
  test proves it compiles under `fancy-regex` (the crate keeps onig, which is
  C, out) and colours the v2 constructs (`#true`, raw `#"…"#` and
  multi-line strings, slashdash comments, type annotations). If no
  upstream v2 grammar compiles under `fancy-regex`, UZE writes its own,
  small, from the v2 grammar.
- Every gallery entry (D20) ships its tests; they are the acceptance suite
  and the worked example of how to test.
- As built (`extension-tests-and-lsp`): steps are `click`, `press`,
  `press-button`, `expect-text`, `expect-no-text`, `expect-absent … because=`,
  `expect-action` and `snapshot`; a gesture is found in what was drawn and
  routed through the renderer's hit map and the keymap, as the workspace
  routes it. A snapshot is text plus one colour token per cell, never RGB.
  The language server is `lsp-server` with the protocol's few messages
  written by hand, and its diagnostics are the check's own path run over
  the editor's unsaved text.

### D27. A source or action is answered by a script first, a program when it must

Decided by the operator on 2026-10-23. Views are KDL whatever answers them;
what changes is who answers "give me the data" or "do this":

| Answer | Declared as | For | Confinement | Consent |
|---|---|---|---|---|
| a **Rhai script**, called once per question, keeping no state | `script "board.rhai"` | the default: parsing checkout files, computing fields, building a compose tree | the engine's (D19), on every platform | the plain grants (`read:checkout`, `net` …) |
| a **program** (argv) | `run "./bin/board.sh"` | wrapping a CLI (`gh`, `kubectl`, `nvidia-smi`, `openspec`), or a quick script in the author's own language | D6's, where the platform offers it | `exec:` grants asked apart, as their own step after the rest, with the pinned path shown; confined: "`gpu-meter` runs `nvidia-smi` on this machine. It can read this checkout and nothing else of yours, write nothing, and reach no network."; where confinement is unavailable, the fixed text of *Product decisions owed* ("This machine cannot confine programs. `nvidia-smi` runs as you, with everything you can reach: your files, your keys, the network. Enable `gpu-meter` from `acme@github` only if you would run its code yourself."); a general-purpose runner adds "`python3` can run anything; this grant is the same as running any code as you." (0.2.3) |
| a **fixture** | `fixture "fixtures/board.json"` | before a script exists, the inspector, and tests (D25, D26) | — | — |

- One language, embedded, with no build. The author writes a `.rhai` file;
  UZE compiles it at load and recompiles a linked extension's on save. There
  is no toolchain to install, no SDK and no binary to publish.
- Script and program answer the same contract: the input object and the
  envelope of D4. A script's `answer(question)` returns a map; a program
  prints the same JSON. A source moves from one to the other without its
  view or tests changing.
- A script that wraps a CLI calls `run_program`, which runs the program
  under D6 and its `exec:` grant: wrapping a CLI is never more confined than
  the program it runs, and consent says so.
- The host registers fast native primitives (lines, glob, regex, JSON, front
  matter, KDL) so a script orchestrates and the heavy work runs in Rust
  (D28).
- The check, the scaffold and the Skill lead with scripts; the program path
  is documented as fully supported, for wrappers and for authors who want
  their own language. Neither is deprecated.
- Consequences for the split: `uze-script` and the stateless script runner
  arrive in change 4, confined everywhere from their first commit; the argv
  runner arrives with OS confinement in change 5, so `exec:` is never
  offered before confinement or its explicit disclosure exists; change 7
  adds the resident entry points of D19 and the media.

*Alternative:* WebAssembly components, compiled by the author in any
language. Rejected after measuring and weighing it: every language needs its
own toolchain and build step, interpreted languages ship their interpreter
inside the component (tens of megabytes), UZE would maintain a WIT world, a
runtime and a build command, and the component model is still moving. What
it would add over Rhai is native speed for heavy computation, which no
gallery entry needs; native primitives cover the heavy parts. It can return
if an extension needs that. *Alternative:* programs only. Rejected: it makes
OS confinement, which varies by platform and holds most open findings, the
gate for every extension, including the many that run no program at all.
*Alternative:* Lua or Luau, or an embedded JavaScript engine. Rejected by the
dependency rule: each compiles C on the release matrix.

### D28. A performance budget, from measurements

Measured on 2026-10-23 on the operator's machine (WSL2, 16 CPUs, release
build, warm file cache) with `rhai` 1.27.0 and `kdl` 6.7.1 and the gallery's
`gsd-band` (the blueprint's `board.rhai` and `board.kdl`) over a realistic
`.planning/` (6 phases, one with 37 files). Rhai is the scripting engine
D27 chose; the program rows are the same answer written for
`sh`, `node` (26.12) and `python3` (3.15), each run as a process per
question.

| Per question | p50 | Against an embedded script |
|---|---|---|
| embedded script (`answer()`: 2 reads, 6 directory listings, the board) | 0.45 ms | — |
| of which file I/O (the same calls in Rust) | 0.05 ms | — |
| of which a bare call into the interpreter | 0.002 ms | — |
| `python3` process | 18.6 ms | ×41 |
| `node` process | 24.2 ms | ×54 |
| `sh` process (its own `ls`/`grep` forks) | 55.4 ms | ×120 |
| the process floor (`/bin/true`) | 0.56 ms | — |

| Per extension loaded (engine with grants, compiled script, parsed manifest and view) | |
|---|---|
| memory, 1 / 20 / 50 extensions | +292 KiB / +7.4 MiB / +11.8 MiB (about 0.25–0.37 MiB each) |
| one idle `node` process, for comparison | 45 MiB |
| one idle `python3` process, for comparison | 11.4 MiB |
| startup: engine 0.19 ms, script 0.15 ms, `extension.kdl` 0.29 ms, view 0.47 ms | about 1.1 ms, so 55 ms for 50 loaded eagerly |
| binding the compiled view to an answer (77 nodes; a prototype binder without lookups, sanitizing or layout) | 0.03 ms |

| One `agent.turn.finished`, 15 extensions × 5 agents | |
|---|---|
| 75 runs, one thread | 41.9 ms |
| 75 runs, a pool of two threads | 29.7 ms |
| 15 runs, answers shared across agents on one checkout | 8.0 ms |
| the same 75 as `node` processes (projected) | about 1.8 s of CPU |

Not measured: drawing to the terminal (ratatui, Sixel), the KDL pre-scan and
sanitizer, a cold file cache, Windows (where processes cost more, so
programs get slower and scripts do not), and contention with running agents.

What the budget takes from it:
- **Answers are shared.** One question (extension, source, checkout, input
  digest) runs once and feeds every surface and band that asked it; bands on
  five agents of one checkout are one run, not five.
- **One pool.** Extension jobs run on a global pool of two threads (the
  workspace's `jobs = 2`), with a serial lane per extension, so a burst of
  events never competes with the agents for more.
- **Lazy loading.** A script and a view compile on first use and are cached
  by digest; enabling fifty extensions does not cost fifty compiles at
  startup.
- **Native primitives.** The interpreter is about seven times the I/O of the
  answer it gives; the host registers fast primitives (lines, glob, regex,
  JSON, front matter, KDL) so a script orchestrates and the heavy work runs
  in Rust.
- **Programs pay per question.** The check warns when a program-backed
  source has an `every` under 30 seconds, and the inspector shows runs per
  minute.
- **Targets**, checked by a performance test in the style of
  `command_performance.rs`: an embedded answer at p95 under 20 ms, binding
  under 1 ms per visible surface, under 1 MiB per loaded extension, and the
  storm above under 50 ms with sharing. The inspector and `uze doctor` show
  each extension's runs per minute, p95 and memory, and flag those over
  target.

## Candidate ADRs

- **Extension logic is an embedded Rhai script first, a confined program
  when it must be, and Rhai is the only resident form (D19, D27).** This answers
  ADR-041's deferred question: the mechanism is chosen, and dynamic
  libraries and native resident interpreters are ruled out.
- **Extensions may act on agents through the bridge.** Writing into an
  agent's prompt gives an extension the agent's reach; the grants, the
  gesture rule and the unattended safeguards are the decision.
- **The `rhai` dependency.** A scripting engine inside UZE's process,
  chosen by provenance, with the sandbox settings D19 states.
- **KDL v2 for manifests, views and tests, with UZE's own bounded
  decoder.** A new third-party parser on extension input, with the guards it
  needs and an LSP to maintain. Recorded as ADR-060 when `extension-view-engine`
  was archived.
- **Consent to extension code is enabling, recorded per grant set.**
  Modelled on ADR-055's stored approval of a project's commands (and, like
  `uze workspace allow`, refused from inside an agent's pane), and a
  deliberate exception to trust being "not a permission system".
- **Landlock/Seatbelt confinement and the `landlock` dependency.** A new
  external dependency with a long tail, and a platform-shaped guarantee.
- **A leaf crate for the extension manifest and wire types.** It widens what
  `uze-extensions` may depend on.

## Risks / Trade-offs

- [Confinement varies by platform, so the same grant means "enforced" on one
  machine and "declared" on another] → It is disclosed on every surface that
  asks for consent and in `uze doctor`. The conformance of the confinement
  itself is proven by a test that attempts each denied operation on Linux CI.
- [Third-party manifests and views are parsed by UZE] → Both are KDL (D22),
  so `noyalib` is off the extension path (it still reads agent
  frontmatter); they are parsed behind a size cap, a depth pre-scan and a
  timed worker thread, only at check, install, enable and a linked extension's reload, never
  by the client (D22).
- [The KDL ecosystem is thin: one maintainer, v1 still common, no schema
  completion] → v2 only with a v1 hint in the check; the decoder is UZE's
  own; the LSP is required work (D26).
- [Short-lived processes make a slow interpreter feel slow on every refresh]
  → Answers are cached until an event names them, and a surface keeps
  drawing its last answer while a run is in flight. Per-run durations go
  into tracing spans and `uze doctor`.
- [The catalog is the ceiling: a tool that needs something the catalog
  lacks cannot be built] → Composing (D15) and the cells tier lift most of
  it without a resident process; the resident tier (D19) is the escape
  hatch, and the gallery (D20) is the test of whether the ceiling is high
  enough.
- [`agent:prompt` gives an extension everything the agent can do, which is
  more than any other grant] → Consent says so in those words; a gesture
  always shows the exact text, which is the text sent; unattended prompts
  need their own per-project, per-agent grant, carry only fixed text or a
  closed set, are refused when the pane is not ready, and are attributed and
  pausable (D16). An extension with `agent:prompt` and `net` can exfiltrate
  through the agent; consent states the combination.
- [A composed tree or a cell grid can imitate host chrome or an agent's
  pane] → Feedback and confirmation are requested, never drawn by a node;
  a `view` keeps the host's attribution strip (D14, D15).
- [A reporter in every launch is a new thing UZE adds to each harness] →
  It travels through each integration's launch contribution, only inside
  UZE, fails open, and its absence on a harness (or under a sandbox that
  denies `$UZE_HOME`) is reported, not papered over (D16).
- [A resident script can burn CPU in the TUI process] → Operation and size
  budgets, a frame cap, and stopping on breach; it runs off the drawing
  thread.
- [Composed views from large answers are built on the render thread] →
  Building a tree into a `View` is a job (M4), memoized by answer.
- [Migrating `code` and `architect` onto the generic contract touches
  roughly 60 call sites in the orchestrator] → That is done first, alone,
  behind the existing orchestrator and TestBackend tests, before any declared
  extension exists, so a regression is attributable.
- [A job closure can capture anything the extension has] → Closures run with
  only `&dyn Host`. The layering rules still forbid `std::process`/`std::fs`
  in the crate, so a job cannot reach further than an inline call could.

## Migration Plan

This is purely additive for operators. No persisted state changes shape.
`state/extensions.json` is new at shape 1 (grants and preferences), the
per-extension store is new at shape 1 (change 5), and the answer cache
lives in memory for a session, so nothing of it is on disk. The per-agent
journal (D16) is new and disposable. The
reporter rides each launch's contribution, so removing it reverts agent
events to the paint heuristic and leaves nothing installed in a harness. The built-in extensions move behind the contract in the first
group of tasks, with no observable change. Rollback is reverting the change:
a plugin's `extensions/` directory is then inert bytes in the Store that no
harness reads.

## Diagrams

`docs/architecture/crate-layering.mmd` gains `uze-contract` and its
edges (from `uze-core` and `uze-extensions`). `containers.mmd` gains the
extension program as an external process the workspace client runs, the
script engine crate (`uze-script`) and its edges, and the
reporter as an edge from each agent's pane, through its journal, to the
workspace client. Both are validated with
`cargo test -p uze-extensions` and `make artifacts`.

## Unresolved review findings

Two reviews ran on 2026-09-27, one on architecture fit and one adversarial on
security and product. The plan was frozen with their findings recorded here.
A second round ran on the 2026-10-23 revision (*Second review*, below).
Each finding names the decision above that it invalidates. On 2026-10-26
every finding was triaged (*Triage*, below): each is resolved, with the
change that carries it, or deferred, with the reason. The sections after
the triage are the history as it was frozen; a marker such as **Open** in
them is historical, and the triage table is the current state.

### Triage (2026-10-26)

Changes: 1 `extension-view-engine`, 2 `extension-tests-and-lsp`, 3
`generic-extension-host`, 4 `extension-manifest-and-scripts`, 5
`extension-programs-and-confinement`, 6 `agent-bridge`, 7
`extension-media-and-resident`, 8 growth; "out" is a capability D11 or D17
removed; "done" is already built.

| Finding | Decision | Change | Where |
|---|---|---|---|
| A1 staleness, lanes, fallback, memory | done: `Lanes`, `Job { run, fallback }`, `memory()`; `orphaned` dropped | 3 | D2 |
| A2 architect's artifact read | done: `Host::artifact_source` | 3 | D2 |
| A3 code timeline | done: a `section` of `CodeExtension`, `Outcome::OpenCommit` | 3 | D2 |
| A4 keymap | one variant `Action::Extension(id)`, a second table per extension | 4 | D8 |
| A5 discovery | done: `capability::extension::discover`, consumed by `Extensions::installed` | 1 | *Discovery* |
| ADR-041 and AGENTS.md on `uze-contract` | done, amended 2026-10-23 | 1 | D1 |
| invariants "one `ExtensionHit` variant per surface" | done; the docs of `uze-extensions/src/lib.rs` and `registry.rs` are still stale | 4 | task 11.6 |
| invariants "UZE does not persist trust" | amended beside the grants record: trust is still never persisted; consent to run is another question | 4 | task 11.4 |
| layering substring checks | done: `workspace_dependencies` in `layering.rs` | 1 | — |
| answer cache on disk (M2) | in memory, per session, in the runner | 4 | D4 |
| `Host::run_program(argv)` | `Host::answer(identity, declaration, input)`; no `ProgramHost` | 4 | D3 |
| rendering cost (M4) | done for bind (memoized per generation, digest, space, ui; `Code`/`Markdown` doubly cached); compose memoized per (digest, size) | 6 | D15 |
| CLI shape `uze ext` (M7) | moot | out | D11 |
| "no process remains" (M8) | narrowed to the process group | 5 | spec program-contract |
| dry run of field references (M8) | done: `missing_fields` over fixtures | 2 | D26 |
| confinement in `pre_exec` | self-re-exec helper `uze __confine` | 5 | D6 |
| S1 socket families | family allowlist in seccomp, installed by the helper; AF_UNIX only via `socketpair`; AF_VSOCK refused on WSL | 5 | D6 |
| S2 `write:checkout` | removed; `edit:checkout` returns later, by gesture, only if a gallery entry needs it | out (8) | *Product decisions owed* |
| S3 `exec:` enforcement | a dependency pinned at enable; argv rebuilt from the accepted manifest; runners "can run anything" | 5 | D6 |
| S4 consent keyed by id | identity `plugin@marketplace/id` keys the record, cache, store and keymap; digest over the loaded bytes | 4 | D5 |
| S5 grant set vs digest | by grant set where enforced, by digest where not; optional pinning as a preference | 4 (rule), 5 (unenforced), 8 (pinning) | D5 |
| S6 checkout undefined | a worktree root the workspace knows, never `$HOME`, `/`, `$UZE_HOME`, tmp or bare; checked every run | 4 (`read_checkout`), 5 (runner) | D6 |
| S7 PATH hijack | realpath pinned and recorded at enable; never re-resolved on a run | 5 | D6 |
| S8 `-` arguments | `--` required before a substituted value; the check refuses `run` until 5 | 5 | task 22.5 |
| S9 `exec:git` | `GIT_CONFIG_PARAMETERS` and `GIT_CONFIG_NOSYSTEM` injected | 5 | D6 |
| S10 Landlock allowlist | the list of D6; enforcement recorded per right, `HardRequirement` | 5 | D6 |
| S11 `uze ext` raw output | moot | out | D11 |
| S12 check runs code | `--run` runs scripts in the engine (4); programs only with `--run` under enforced confinement, else `--checkout` or recorded answers (5) | 4, 5 | D26 |
| S13 resource limits | engine budgets, a pool of two, a serial lane (4); subreaper, rlimits, two concurrent runs, no `RLIMIT_NPROC` (5) | 4, 5 | D6, D19, D28 |
| S14 sanitizer gaps | `Clean` newtype sanitizing on deserialize, width in cells | 4 | D7 |
| S15 spoofing | id charset, `timeline` reserved, attribution `id · plugin@marketplace`, the strip on every third-party surface | 4 | D14, task 3.4 |
| S16 clipboard | the toast shows the copied text; multi-line or over 256 bytes confirms first | 4 | task 6.4 |
| S17 signals | seccomp limits signals to the group; Landlock scope on ABI ≥ 6; Seatbelt same-sandbox | 5 | D6 |
| S18 confinement fails silently | checked every run; refused when the record says it was enforced | 5 | D6 |
| R-A1 ADR-056/040 | amended with `uze agent report` | 6 | task 14.9 |
| R-A2 status bar | done | — | D14 |
| R-A3 queueing | done | — | D16 |
| R-A4 hook vocabulary | done | — | D16 |
| R-A5 reserve `spec` | `RESERVED_IDS` already holds `code`, `architect`, `spec`, `uze`; `timeline` joins | 4 | task 3.4 |
| R-A6 runtime placement, HTTP, raw keys | done (`uze-script`); `fetch` deferred; raw keys with the resident tier | 4, 7, 8 | D19 |
| R-A7 band resizes the PTY | fixed rows in `compute_layout` | 6 | D14 |
| R-A8 several images per frame | the painter holds N images per frame, per slot, decoded off-thread | 7 | task 17.2 |
| R-A9 duplicates | done | — | D9, D10 |
| R-A10 consent in a pane | enabling refused when `UZE_AGENT` or `launched_by` says a pane; no `--yes` | 4 | D5 |
| R-A11 compose rebuilt per frame | memoized | 6 | D15 |
| R-A12 `notify`/`sound` | `uze_platform::notify`, no crate: BEL and OSC 9/777 where the terminal says so, `notify-send`/`osascript`/PowerShell by path, `None` without a desktop; `sound` is BEL with a fixed cue name | 6 | task 14.10 |
| R-A13 stale citations | symbols instead of lines in this revision; the crate docs in change 4 | 4 | task 11.6 |
| R-A14 interrupt, `invocation_prefix` | done | — | D16 |
| R-S1 unattended prompts | fixed text or closed set, with the rates of 0.2.4 | 6 | D16 |
| R-S2 forgeable events | events are claims | 6 | D16 |
| R-S3 writing is typing | strict filter, byte equality, refuse unless `Ready` | 6 | D16 |
| R-S4 resident imports | `read_checkout` beneath the root, no link, no `..` (4); `run_program` reported as it is (5, 7) | 4, 7 | D6, D19 |
| R-S5 `agent_prompt` without gesture; `run_program` limits | confirmation unless `auto` (6); limits of D6 and D28 (5) | 5, 6 | D19 |
| R-S6 pipes | moot | out | D17 |
| R-S7 pane imitation, composed feedback | attribution strip on `view` and `section` (4) and `overlay` (6); feedback only through the envelope | 4 | D14, task 13.1b |
| R-S8 secrets | `secret` arrives with `net`; keychain per platform, no new crate | 8 | D18 |
| R-S9 tool summaries | `agent:observe`; name and relative path only | 6 | D16 |
| R-S10 nominal target | `target` required; no broadcast | 6 | D16 |
| R-S11 composed buttons | resolved arguments in the confirmation; a click within 300 ms of a re-layout ignored | 6 | D15 |
| R-S12 image decoding | 8 MiB per file before decoding, 32 MiB of decoded pixels per extension, one decode at a time per extension, off the drawing thread, cropped to the slot | 7 | D4 |
| R-S13 store across projects | per (identity, project); `store: global` with its own consent; 1 MiB | 5 | D18 |
| R-S14 digest TOCTOU, pause key, restart, notifications, fail-open | digest over the loaded bytes (4); the rest with the bridge (6) | 4, 6 | D5, D16 |
| R-U1 event payloads | done | 4 (`every`, paths), 6 (`deliver: each`, `events`) | D17 |
| R-U2 which agent | done | 6 | D14, D16 |
| R-U3 composed references | done | 6 | D15 |
| R-U4 store from a source | done | 5 | D4 |
| R-U5 bridge model | done | 6 | D16 |
| R-U6 `notify`/`sound` from an answer | done | 6 | D4 |
| R-U7 `open` and global keys | the global part of the second table; `jump` itself in 6 | 4 | D8 |
| R-U8 size in context, empty answers | done | 4 | D4, D14 |
| R-U9 media formats | done | 7 | D4 |
| R-U10 nesting and repetition | done | 1 | D15, D23 |
| R-U11 manifest grammar, test loop, shebangs | done | 2 | D21, D26 |
| Third review (KDL) | landed in D22/D26, ADR-060; the `kdl` fence inside a `.md` in the editor is cosmetic | 8 | task 23.8 |
| consent clicked through | leads with implicit rights and combinations | 4 | spec extension-security |
| `output: lines` | with programs | 5 | D4 |
| openspec board under confinement | moot (12.1 dropped) | out | D20 |
| trait lacks hover, drag, pan, `scroll_to` | done | 3 | D2 |
| `Outcome::OpenPath` typed | done: `OpenPath { project, target }` | 3 | D2 |
| events from the badge and repaints | `every`, `paths.changed`, `checkout.changed` per checkout (4); `agent.*` from the bridge (6) | 4, 6 | D17 |
| removal clause names `uze plugin remove` | the verb is `uze remove <plugin>` with its machine flag | 4 | spec plugin |
| `noyalib` on third-party input | moot since D22; the exception is in the `dependencies` rule | out | *Risks* |
| `rhai` review | accepted, `std`+`sync`, raw engine with bounds | 4 | D19 |
| HTTP/TLS client | deferred; `fetch` over the system's `curl` when a gallery entry needs it | 8 | D19 |
| store | deferred to `gpu-meter` | 5 | D18 |
| preferences, secrets | deferred to `agents-hud`; `secret` with `net` | 6, 8 | D18 |
| compose (`view` mode) | deferred; bind covers change 4's gallery | 6 | D15 |
| `overlay`, `status`, `band`, `notify` slots | deferred with `jump`, `agents-hud`, `gsd-band`'s band, `chimes` | 6 | D14 |

### Proposed split (decided 2026-10-26; first revised 2026-10-23)

Visual engine first. The first change runs no third-party code, so none of
the execution findings (S1–S18, R-S*) gate it and it can be unfrozen on its
own; each later change adds one reason to trust or one capability. The
engine implements the `Extension` contract of D2 from its first commit, so
migrating the built-ins later moves them onto it rather than rewriting it.

1. **extension-view-engine**: the starting catalog (the nodes `gsd-band` and
   a plain panel need), the bounded KDL decoder and compiled views, the
   binding rules, lookups and templates, host-held UI state (D22–D24), the
   `view`, `section` and `drawer` slots, data from fixtures only (actions are
   recorded in the inspector, never run), reload on save for linked
   extensions, the error overlay and a minimal inspector (D25), and KDL highlighting. It
   also lays the minimal generic path of D2 (the `Extension` contract, one
   slot map, one `spawn_extension_job`/`absorb_extension_answer` pair in
   `orchestrator/reads.rs`, one outcome follow-up), used by the engine only:
   no per-extension fields, channels or `ExtensionHit` variant are added for
   it, so split 3 moves the built-ins onto that path and writes nothing new.
   Proof: the `gsd-band` board drawn from a fixture, edited live.
2. **extension-tests-and-lsp**: tests in `uze agent plugin check` with
   recorded answers and cell snapshots, and `uze agent plugin lsp` (D26). Visual extensions can be written
   and tested end to end.
3. **generic-extension-host**: group 1, with `code`, `architect` and `spec`
   moved onto the path split 1 laid, the per-extension fields, channels and
   hit variants removed, and the invariant and AGENTS.md edits for
   `ExtensionHit`, with no new behaviour.
4. **extension-manifest-and-scripts**, the least that runs a third-party
   plugin end to end, in this order:
   1. the full `extension.kdl` in `uze-core`'s decoder and the check:
      `source { script | run | fixture, input, every, refresh-on, deliver }`
      (`run` decoded but refused by the check until change 5), `action {
      id, label, on, key, confirm, script | refresh | clipboard | open_path
      | open }`, `wants`, `events`; `preferences` refused until 6; the
      grants of D5 only (`read:checkout`, `clipboard`, `open_path`), every
      other refused with "arrives in change N"; `timeline` reserved;
   2. the grants record and consent: `state/extensions.json` per identity,
      consent per machine and per (identity, digest), enabling per
      project, the digest over the loaded bytes, enabling refused inside
      an agent's pane, non-interactive install leaves it disabled;
   3. the keymap's second table (D8, group 8);
   4. `uze-script` and `Host::answer` with the `ExtensionRunner`'s
      in-memory session cache (D3, D4, group 24);
   5. sources and actions in the declared engine (group 6): jobs,
      staleness by question, `applies-when`, item menus, the footer, a
      confirmation showing the resolved arguments, the minimal envelope
      (`data`, `refresh`, `toast`, `notice`), `clipboard` and `open_path`
      from a gesture only, the `Clean` sanitizer and the attribution strip
      on every third-party `view` and `section`;
   6. `every`, `paths.changed` and `checkout.changed` per checkout (group
      15; `deliver: each` and the `events` source wait for 6);
   7. the Extensions screen and `uze status`/`inspect` (groups 9, 10);
   8. docs: the ADR-041 amendment, the extensions rule, the invariants,
      citations by symbol, the diagrams with `uze-script`, a minimal
      `uze:extension` Skill.
   Third-party code runs from here on, confined on every platform. Proof:
   a plugin from a non-linked marketplace (cloned into the Store) with a
   `view` and a `section` fed by a Rhai script under `read:checkout`, one
   action with `refresh` and one with `confirm`, installed by `uze
   install`, consented to on the Extensions screen, opened from the tab
   strip, driven by key and pointer; and `gsd-band`'s board from
   `board.rhai` with `paths.changed: [.planning]` and `every`. Out, with
   the reason: programs and `exec:` (no confinement yet, 5); the store
   (`gpu-meter`, 5); preferences and secrets (`agents-hud`, 6; `secret`
   with `net`, 8); compose and the `view` output mode (bind covers the
   gallery, 6); the `overlay`, `status`, `band` and `notify` slots (6);
   `jump` (needs `agents`, 6); `fetch` and `net` (8); the resident tier
   and media (7); pipes and an extension CLI (8).
5. **extension-programs-and-confinement**: the argv runner with the argv
   rebuilt from the accepted manifest, `exec:` resolved to a pinned
   realpath, Landlock, seccomp and Seatbelt in `uze-platform` with the
   `uze __confine` helper and "unavailable" declared, the "runs as you"
   consent step, the digest rule for unenforced grants, the resource
   limits of D6, `output: lines`, the store (D18), `run_program` for
   scripts, and `--run` for programs in the check. Proof: `gpu-meter`.
   Windows's restricted token is its own later change
   (`extension-confinement-windows`).
6. **agent-bridge**: `uze agent report` and its journal (ADR-056 and
   ADR-040 amended), the `agent.*` events with `deliver: each` and the
   `events` source, the `agents` source, the `status`, `band`, `overlay`
   and `notify` slots, `uze_platform::notify`, preferences, compose (the
   `view` output mode, R-S11), the agent actions and `agent:prompt:auto`
   with the rates of 0.2.4 (ADR-059 amended), `focus_agent`. Proof:
   `gsd-band` with its next-step button, `agents-hud`, `flightdeck`,
   `chimes`, `jump`, `plan-review`.
7. **extension-media-and-resident**: `cells` with regions, `image`,
   `sprite`, several images per frame with the per-extension budget of
   R-S12, then the resident entry points of D19 on the runtime change 4
   brought, raw keys (the extensions rule amended), `timer`. Proof: `pets`,
   `screen`, `snake`.
8. **Growth**: `net` and `fetch` over the system's `curl`, the `secret`
   preference and the keychain, `edit:checkout` by gesture, pinning by
   digest as a preference, the `kdl` fence inside a `.md` in the editor,
   pipes and extension CLI commands (D11): each only when a gallery entry
   needs it, then whatever a new gallery entry needs.

### What each change touches

| Change | New code | Existing code it changes |
|---|---|---|
| 1 view engine | `uze-contract`; `uze-extensions/src/declared/`; `uze-core/src/capability/extension/` (bounded KDL decoder); the KDL grammar; reload of linked extensions; the inspector | `view.rs` and `extension_view.rs` grow; `uze-theme` symbols; a compile service in `uze-application`; the minimal generic path in the orchestrator; `layering.rs`; ADR-041 amended |
| 2 tests and LSP | tests in `uze agent plugin check`, `uze agent plugin lsp` | — |
| 3 generic host | — | the orchestrator's per-extension wiring for `code`, `architect`, `spec` (about 60 sites), behind the existing orchestrator and TestBackend tests |
| 4 manifest and scripts | `uze-script` and the stateless runner, the grants record, the full manifest in the check, the Extensions screen | `uze-keys` through a second table resolved after the host's, leaving `Action`'s ~730 uses untouched; `uze-contract` gains `Clean` |
| 5 confinement | the argv runner; confinement in `uze-platform`; the `uze __confine` helper; the store | — |
| 6 agent bridge | the journal, `uze agent report`, the `agents` source | each of the four integrations; `capability/hook.rs`; `compute_layout` for bands |
| 7 media and resident | the resident entry points in `uze-script` | the decoders leave `code/picture.rs` for `shared/`; `graphics.rs` holds several images per frame |

Each change carries, in the same commit as the code that needs it: reads off
the drawing thread through `orchestrator/reads.rs` (the
`ui-orchestrator-reads` rule), what `src/` needs from the domain as an
`uze-application` read model or service (never `uze_core::`), every new path
named in `UzeHome` with its persistence tier, visible surfaces built from
`src/ui/widget/` per the `design-system` skill, each new CLI leaf
classified in `command_performance.rs`, the `.mmd` diagrams updated and
drawn by `make artifacts`, and each new dependency judged by the
`dependencies` rule.

Order inside change 4: manifest and check, the grants record, the keymap's
second table (group 8), `uze-script` and `Host::answer` (group 24),
sources and actions (group 6), events (group 15), surfaces for the
operator (groups 9 and 10), docs (group 11). The keymap comes before the
engine's actions because an action without a key cannot be tested by a
`.test.kdl`'s `press`, and a key without an action does not exist. Each
ADR, rule and invariant amendment lands with the commit that creates the
fact it describes (0.9): ADR-041 with the commit that creates
`uze-script` (4); ADR-056, ADR-040 and ADR-059 with `uze agent report`
and `agent.prompt` (6); the extensions rule with two lines on `rhai` and
the question by declaration (4) and with raw keys (7).

### Product decisions owed

All five decided on 2026-10-26:

- **`write:checkout`** (decided 2026-10-26): removed. No tier writes the
  checkout. A later `edit:checkout` grant, added only when a gallery entry
  needs it, returns edits in the envelope and the host applies them on a
  gesture through ADR-048's grants, refusing the protected metadata list
  (`.git` and a worktree's resolved gitdir, `.agents`, `.claude`, `.codex`,
  `.opencode`, `.gemini`, `.vscode`, `.githooks`, `.envrc`, `.env*`,
  `AGENTS.md`, `CLAUDE.md`, `agents.yaml`, `agents.lock`); its consent says
  a project file is code the agents and the operator run. Under
  confinement the checkout is read-only; a program's only writable path is
  a per-run scratch directory. None of the ten gallery entries writes the
  checkout, so removing it costs nothing now (Codex keeps `.git`,
  `.agents`, `.codex` and `.aws` read-only inside any writable root; Claude
  Code's sandbox runtime denies every write by default).
- **Enablement scope** (decided 2026-10-26): consent is per machine and per
  (identity, digest); enabling is per project. One record,
  `state/extensions.json`, holds both: per identity, the accepted grants,
  the digest and time they were accepted at, the resolved `exec:` paths,
  whether confinement was enforced when accepted, and the projects it is
  enabled in. A plugin a project declares offers its extensions in that
  project; the workspace asks on first open there, as it asks for a
  project's commands. There is no machine-wide enable (VS Code trusts per
  folder; Claude Code's "don't ask again" writes the project's
  `settings.local.json`, never the user's; ADR-055 approves per project).
- **Unenforced consent** (decided 2026-10-26): an extension whose `exec:`
  grants cannot be enforced on this machine may still be enabled, from
  UZE's chrome only, after a separate step that says: "This machine cannot
  confine programs. `<program>` runs as you, with everything you can
  reach: your files, your keys, the network. Enable `<id>` from
  `<plugin@marketplace>` only if you would run its code yourself." The
  record keeps `enforced_at_accept: false`; such an identity is re-asked by
  digest on every update (S5); the Extensions screen, `uze status` and
  `uze doctor` show "runs as you". No setting silences it. In change 4
  there is no unenforced case: only scripts run, and the engine confines
  on every platform (Deno treats `--allow-run` as `--allow-all`; Codex
  Windows falls back from `elevated` to `unelevated` and lets the operator
  forbid the fallback; Claude Code's `allowUnsandboxedCommands` is
  explicit).
- **The `rhai` review and the HTTP client** (decided 2026-10-26): `rhai`
  1.27 accepted with `default-features = false, features = ["std",
  "sync"]`, in `uze-script` only, built raw with the bounds of D19; the
  HTTP client is deferred to change 8 and is the system's `curl` by path
  through `uze_platform::tools::curl()`, never a TLS crate (D19).
- **Unattended prompt rate** (decided 2026-10-26, numbers of change 6):
  per (extension, agent) one prompt per agent turn and at most one per
  60 s; per agent across extensions 3 per minute and 20 per hour; 10 s
  between any two unattended writes into one pane; each the fixed text or
  the member of the closed set, at most 512 bytes; `agent:prompt:auto`
  lasts 24 h from acceptance per project and agent, then the next attempt
  becomes a toast → dialog. `gsd-band`, the one entry that needs `auto`,
  sends one prompt per turn; 3 per minute covers three extensions on one
  pane without making the agent an automaton; 24 h is GitHub's sudo-mode
  window (Raycast's 1-minute floor for background intervals; Zellij and
  Claude Code have no limit, which is where the prompt-repeating HUDs came
  from).

Decided on 2026-10-23 by the operator: unattended prompts carry fixed text
or a closed set (D16); a busy pane refuses rather than queues (ADR-059);
`status` is a row in the sidebar's header, not a new status bar (D14).

### How the 2026-10-23 revision bears on the first findings

It resolves none of them outright. It changes these:
- *Events* (minor): D17 replaces the badge-derived, focused-only
  `checkout.changed` and per-frame coalescing; D16 replaces the repaint
  heuristic for `agent.*` where a harness reports.
- *`output: lines`* (UX): adopted in D4.
- *Enabling where confinement is missing*: the resident tier's own reach is
  confined on every platform; programs, including those it runs, are not.
- *`kind: interactive`*: retired in favour of the resident tier (D13).
- *M4*: compose (D15) makes it more pressing; memoized per answer and size.
- *S4*: the identity `plugin@marketplace/id` now keys the store and
  grants (D18); D4's cache path and D8's action names still
  use the bare id and must follow.
- *S12*: `uze agent plugin check` runs programs only with `--run`, and only where confinement is not
  enforced (D21).

### Second review (2026-10-23)

Three reviews ran on the revision: architecture fit, adversarial security,
and an author writing the ten gallery entries from the specs alone. Their
findings shaped D13–D21 as they now stand; each line says where it went.
Those marked **open** are not resolved by the text above.

Architecture:
- **R-A1** No channel from a hook to the workspace client; ADR-056 lets a
  pane only open a space. → D16 *Reporting channel* (journal polled by the
  watch, reporter per launch). **Open**: ADR-056 and ADR-040 amendments.
- **R-A2** No status bar exists; a new row shortens every pane. → D14
  (`status` in the sidebar header).
- **R-A3** Unattended queueing contradicts ADR-059; the writer is
  `submit_notice` behind `may_submit`, not `pane_input.rs`. → D16 (refuse,
  never queue).
- **R-A4** The portable hook vocabulary lacks turn start and approval, and
  a portable hook fires outside UZE. → D16 (per launch, vocabulary grows).
- **R-A5** The built-in `spec` extension was ignored (five `ExtensionHit`
  variants, `spec_places`, GSD already read). → Context, split 1, D20.
  **Open**: reserve `spec` with S15.
- **R-A6** (written for WebAssembly; holds for `uze-script`) The runtime cannot live in `uze-core` or
  `uze-application`; `fetch` needs an HTTP client; raw keys invert the
  extensions rule. → D19 (own crate, owed HTTP, rule amended).
- **R-A7** A band sized by its answer resizes the PTY (SIGWINCH, redraw
  grace); there is no pane frame. → D14 (fixed rows in `compute_layout`).
- **R-A8** Pictures exist only in full-frame views, one per frame; no
  regions; `Span.color` must be refused; decoders are private to `code`. →
  D4, D15, split 6. **Open**: several images per frame.
- **R-A9** D10/D17 and D9/D15 duplicate; proofs needed later splits. →
  D9/D10 superseded, split reordered.
- **R-A10** ADR-055 already stores consent; enabling must refuse in an
  agent's pane. → Candidate ADRs; D11 has no enabling verb. **Open**: D5
  text.
- **R-A11** Compose is rebuilt every frame and depends on size. → D15
  (memoized per answer and size).
- **R-A12** `notify` and `sound` have no platform concept. → split 5.
  **Open**: the concept in `uze-platform` and its dependency.
- **R-A13** Stale citations (orchestrator line ranges, `invariants.md`
  lines); task 2.1 is already done. **Open**: refresh the citations; drop
  2.1 but `release_notes.rs`.
- **R-A14** `agent.interrupt` has no integration concept; `invocation_prefix`
  is presentation-only. → D16 (interrupt deferred, new spelling method).

Security:
- **R-S1** (critical) Unattended prompts turn repository, event and pipe
  data into the operator's instruction, bypassing every other grant. → D16
  (fixed text or closed set; per project and agent; digest change revokes).
- **R-S2** (critical) Hook events are forgeable by anything in the pane. →
  D16 (*Events are claims*; writes gated on what the host observes).
- **R-S3** (critical) Writing to a pane is typing: `!`, slash commands, an
  open approval, invisible characters, shown ≠ sent. → D16 (filter, byte
  equality, refuse unless `Ready` and foreground).
- **R-S4** (high) Resident imports run in UZE's process: `read-checkout`
  follows links; enforcement claimed where `run-program` is not confined.
  → D19 (beneath-root resolution, a grant, per-import report).
- **R-S5** (high) The `agent-prompt` import had no gesture rule and
  `run-program` no limits. → D19.
- **R-S6** (high) Pipes were a confused deputy. → D17 (sender, refresh
  only, named senders).
- **R-S7** (high) A full-frame grid can pass for a pane and log keys;
  `confirm`/`toast` were composable. → D14 (attribution strip), D15
  (feedback is requested, not drawn).
- **R-S8** (high) Secrets in a plaintext record, laundered through store,
  answers, prompts, cache keys. → D18.
- **R-S9** (medium) Tool summaries widen what an extension reads. → D16
  (`agent:observe`, name and relative path only).
- **R-S10** (medium) Agent targeting was nominal. → D16 (`target`).
- **R-S11** (medium) Composed buttons chose arguments the confirmation did
  not show; click-jacking on re-layout. → D15.
- **R-S12** (medium) Image and sprite decoding fed by any extension. → D4
  (geometry caps, off-thread). **Open**: a per-extension memory budget.
- **R-S13** (medium) The store crossed projects. → D18.
- **R-S14** (low) Digest TOCTOU; unreachable pause key; `agent.restart`
  unconfirmed; unattributed notifications; a hook must fail open. → D16,
  D19, D14.

Authoring:
- **R-U1** Event payloads never reached programs; coalescing dropped them.
  → D17 (`input.event`, `deliver: each`, the `events` source).
- **R-U2** Which agent a band or action belongs to. → D14, D16 (`target`,
  `context.agent`).
- **R-U3** References from composed nodes. → D15 (`item`).
- **R-U4** Sources could not write the store. → D4 (envelope).
- **R-U5** The bridge's model and events were too thin. → D16.
- **R-U6** No path from an answer to `notify`, `sound`. → D4 (envelope).
- **R-U7** No way to open a surface; no global keys. → D20, split 3.
  **Open**: the `open` action and global keys in the input-keymap delta.
- **R-U8** No size in the context; empty answers undefined. → D4, D14.
- **R-U9** Media wire formats unspecified. → D4.
- **R-U10** Node nesting and bind repetition undefined. → D15.
- **R-U11** The manifest grammar was never written; no test loop without a
  TUI; interpreter shebangs. → D21.
- Kept as known losses: D20.

### Architecture blockers (invalidate D2, D8, D1/task 4.1)

- **A1. Staleness and job policy live in the orchestrator, not in the
  extension.** D2 is wrong about this. The orchestrator's read scheduling
  (today `orchestrator/answers.rs::schedule_engine_jobs`) keeps
  a save ahead of the re-read that follows it, runs one read at a time,
  keeps a measure cache that outlives the surface, drops answers by root,
  gives each read a typed fallback when a thread panics, and reports a write
  that failed after its surface closed. The contract needs to state:
  - serial vs. parallel lanes per slot;
  - a required fallback answer;
  - an `orphaned(answer) -> Option<Outcome>` hook;
  - extension memory that outlives a surface, separate from `Place`.
- **A2. The architect's artifact read is not a `Host` call.** It goes through
  `uze_application::project_artifacts` (today
  `WorkspaceHost::artifact_source` in `src/ui/extension_host.rs`). Either
  add a grant, or have the host resolve it into the open context.
- **A3. The code timeline is the orchestrator's `GitBadge.timeline`.** That
  read also feeds the badge and target evaluation, the section is drawn with
  no code view open, and the commit popup is the orchestrator's. Either
  decide how long a section lives, or leave the timeline out of group 1.
- **A4. The keymap design (D8) fails at runtime.**
  - `Action` is `Copy` with a `const` list (`crates/uze-keys/src/action.rs`,
    about 730 sites).
  - `Keymap::new` refuses a chord that reaches two actions in one scope, so
    two extensions sharing one `Scope::Extension` with the same suggestion
    fail the whole load.
  - Direction: a second table resolved after the host's, a per-slot scope,
    and interned numeric ids.
  - "Destructive never holds an unmodified letter" must follow from a
    declared confirmation.
- **A5. Discovery through `package_resources_at` (task 4.1) routes extensions
  into harness delivery.** That means Router, exposure, receipts and
  17 integration files, which contradicts "never to a harness". Direction:
  `capability::extension::discover`, consumed only by `uze-application`.

### Architecture majors

- **Documents that must be amended, and have no task yet:**
  - ADR-041 and AGENTS.md ("uze-extensions depends on no UZE crate");
  - `invariants.md`'s section "Every extension is hosted through the one
    contract" and AGENTS.md ("one `ExtensionHit` variant per surface"),
    which are checked by `invariants_citations.rs`;
  - `invariants.md`'s section "Remote executable capabilities cross an
    explicit consent boundary" (UZE does not persist trust decisions),
    which D5 contradicts.
- **Layering rules the plan misread.** They are substring checks, so admitting
  `uze-contract` (task 3.6) changes nothing. Moving `code/markdown.rs`
  (task 2.1) must move its sanctioned path in
  `no_chrome_glyph_is_written_where_it_is_drawn`.
- **The answer cache on disk (D4, task 6.3) is not allowed.**
  `uze-extensions` may not use `std::fs`, and the path would have to be named
  in `UzeHome`. Across restarts no event can invalidate it. Direction: an
  in-memory cache per session.
- **`Host::run_program` should take (extension, declaration id, inputs).**
  The runner rebuilds the argv from the accepted manifest it loaded, which
  closes the race with a widened update. Consider a separate `ProgramHost`,
  so built-ins are not handed the grant.
- **Rendering cost.** Building a `View` from a large answer, and highlighting
  `Content::Code`, would run on the render thread. They must be jobs or
  memoized, as `code` already does with `FileRequest::Colour`.
- **CLI shape (D11).** One `Command::Ext { args }` leaf cannot hold the four
  classifications, and the budgeted leaves need performance tests. `uze ext`
  mixes machine scope (`list`, `enable`, `disable`) with project scope
  (`<id> <cmd>`), against ADR-019.
- **Two scenarios that cannot be tested as written:**
  - "no process remains", which `setsid` escapes, so it narrows to the
    process group;
  - the check's dry run verifying every field reference, which needs
    `examples:` fixtures in the manifest and is best effort otherwise.
- **Confinement inside the TUI process.** Applying it in `pre_exec` in a
  multi-threaded process must stay async-signal-safe. Direction: a
  self-re-exec helper (`uze __confine -- argv`).

### Security criticals (invalidate D5, D6, parts of extension-security)

- **S1. AF_UNIX is not denied.** With no grants a program reaches
  - the systemd user bus (`StartTransientUnit` runs unconfined code),
  - `docker.sock`,
  - ssh/gpg agents,
  - the WSL interop socket.

  The filter also misses AF_NETLINK, AF_VSOCK, `io_uring` and `socketcall`.
  Direction: seccomp allowlists socket families; AF_UNIX is allowed only via
  `socketpair`; `io_uring_setup` is denied; the audit arch is checked; and on
  Landlock ABI ≥ 6, abstract sockets and signals are scoped.
- **S2. `write:checkout` is code execution later.** A program can write
  `.git/hooks`, `.git/config` (`core.fsmonitor`, `core.sshCommand`),
  `.envrc`, harness settings, `Makefile` or `AGENTS.md`. See *Product decisions
  owed*.
- **S3. `exec:` is not enforced at runtime.** A packaged script can exec
  anything reachable, and interpreters or runners (`sh`, `python3`, `node`,
  `env`, `xargs`, `git -c`, `npx`, `docker`, `ssh`) are unlimited. Direction:
  - `exec:` is a dependency declaration;
  - consent rests on filesystem and network rights;
  - general-purpose runners are flagged "can run anything";
  - execute rights are limited to the resolved binaries where Landlock is
    enforced.
- **S4. Consent keyed by extension id is inherited across plugins.**
  Direction: key the record, the cache and the keymap namespace by
  (marketplace, plugin, id), and treat a change of source as new consent.
- **S5. Grants-by-set lets a compromised maintainer swap code within the same
  grants.** Direction: apply it only to linked local marketplaces; a remote
  update shows the digest change, with optional pinning.
- **S6. "Checkout" is undefined and can be `$HOME`.** Direction: it must be a
  Git worktree root, never `$HOME`, `/` or an ancestor of `$UZE_HOME`.

### Security highs

- **S7. PATH hijack.** The risks are relative or empty entries, checkout-local
  bins, and WSL's `/mnt/c` resolving to Windows executables that run outside
  confinement. Direction: resolve every `exec:` name to an absolute realpath
  when the extension is enabled, record it in the consent, and refuse
  checkout and drvfs paths.
- **S8. Argument injection through values starting with `-`.** Direction:
  refuse such values unless the declaration opts in, and have check, the
  scaffold and the Skill teach `--`.
- **S9. `exec:git` in a hostile checkout honours its config and hooks.**
  Direction: inject `GIT_CONFIG_PARAMETERS` overrides the way
  `acquisition/git.rs` does.
- **S10. The Landlock allowlist is both too narrow and too loose.**
  - Too narrow: `/proc`, `/dev` nodes, a temporary directory, install prefixes
    such as `~/.nvm`, `/nix/store` and Homebrew, and a worktree's Git common
    dir are missing.
  - Too loose: `/etc` is readable in full.
  - Partial ABI: on ABI < 3, `truncate` is unrestricted; on ABI < 4, TCP is.
  - Direction: report enforcement per right, and never use best-effort mode
    silently.
- **S11. `uze ext <id> <cmd>` prints raw output.** That allows OSC 52, title
  changes, link spoofing and TIOCSTI. It also contradicts the sanitization
  requirement. Direction: pipe stdio, `setsid`, sanitize when stdout is a
  TTY, and offer `--raw` only when it is not. Consent, enabled state and
  confinement apply to CLI commands too.
- **S12. `check` runs untrusted sources without consent.** Direction: run
  programs only with `--run` and only under enforced confinement, or with
  `--checkout <path>` or canned answers.

### Security mediums

- **S13. Resource limits.** A fork bomb within the time limit, a `setsid`
  daemon surviving the kill, disk fill, and many extensions on one event are
  all possible. Direction:
  - a pids cgroup, or `RLIMIT_NPROC`/`CPU`/`FSIZE`/`AS`;
  - killing descendants through a subreaper or cgroup;
  - a global cap on concurrent runs and a debounce floor.
- **S14. Sanitizer gaps.** Cf and zero-width characters, Unicode tag
  characters, U+2028/2029 and stacked combining marks get through, and length
  is bounded in bytes rather than display width. Direction: a `Clean` newtype
  in `uze-contract` that sanitizes on deserialize, replacing the
  constructor-grep test.
- **S15. Spoofing.** Ids have no charset rule (`../`), built-in ids `code`,
  `architect` and `uze` are not reserved, and display names allow homoglyphs.
  Direction: ids must match `[a-z0-9-]`, those ids are reserved, and chrome is
  attributed as `id · plugin@marketplace`, never by display name.
- **S16. Clipboard mismatch.** Direction: the toast shows the exact copied
  text, and a multi-line copy asks first.
- **S17. Signals.** Below Landlock ABI 6 a program can kill the TUI or agents.
  Direction: seccomp limits `kill` to its own group.
- **S18. Confinement can fail inside an already-sandboxed parent or a
  container.** Direction: detect it on every run, and fall back to
  "unenforced" with disclosure, never silently.

### Third review: the KDL proposal (2026-10-23)

Three reviews ran on a proposal to write views (and first the manifest) in
KDL: ecosystem and alternatives, architecture and security, and an author
writing four gallery entries in it. Where each went:
- Parser recursion without a depth limit overflows a stack on a 1.6 KB file,
  and kdl-rs#177 makes 132 malformed bytes take 78 s → D22 (cap, pre-scan,
  timed worker, never in the client).
- The serde bridge is experimental, mis-decodes a single node and loses
  spans → D22 (hand-written decoder).
- Repeated properties resolve silently; `text {id}` and same-line siblings
  parse into something else → D22 (refusals with suggestions).
- No editor completes KDL from a schema, and Helix, Zed and Zellij are v1 →
  D22 (v2 only, v1 hint), D26 (LSP required).
- A KDL manifest loses ready-made completion on the file that holds grants
  → D22 first chose TOML; the operator then chose KDL for every file, with
  the LSP shipping alongside the views.
- `noyalib` stays for agent frontmatter; only the extension path leaves it
  → *Risks*.
- The binding rules had to be invented by the author (types, mixed
  interpolation, escapes, scopes, `unless`, lookups, i18n, forms, nested
  `each`) → D23, D24.
- `sh` programs needed `jq` to read input → D24.
- The no-logic boundary drifted with `each`, `when`, `show`, templates →
  D23 draws the line.
- KDL parses about twenty times slower than JSON → D22 (views compiled once;
  answers stay JSON).

### Product and UX

- **Consent will be clicked through.** It must lead with the implicit rights,
  such as "reads the whole checkout, `.env` included", and with the combined
  effect, such as "with `net`, can upload this project".
- **`sh` authors cannot produce correct JSON without `jq`.** Direction: add an
  `output: lines` or JSONL mode for list sources.
- **The openspec board as planned does not run under confinement.** Archive
  needs a write, and a node install sits under `~/.nvm`. Its proof (task 12.1)
  must run under Linux confinement in a worktree.

### Minor

- The D2 trait lacks hover, drag and pan, and `scroll_to`.
- `Outcome::OpenPath` needs a typed `open_at(path)` in the contract.
- `checkout.changed` taken from the badge fires only for the focused
  checkout, `agent.*` events come from a repaint heuristic, and "per frame"
  coalescing is not the same as "one run per burst".
- The plugin spec's removal clause should name `uze plugin remove`, because
  dropping a plugin from `agents.yaml` keeps it in the Store.
- `noyalib` 0.0.x now parses third-party input. Record the reason and the way
  out that the dependency policy demands.

### Findings by requirement

The specs carry only a pointer to the findings that touch each requirement.
The notes they carried when the plan was frozen are kept here verbatim.

- **extension-platform, "Built-in and declared extensions are hosted the same way":** A1 (staleness and job policy live in the orchestrator), A2 (the architect's artifact read is not a `Host` call), A3 (the code timeline belongs to the orchestrator's Git badge and outlives the code view). The migration cannot yet be done "with no change an operator can observe" as designed.
- **extension-platform, "An extension is declared in one manifest that carries no logic":** S15 (ids need a charset and built-in ids must be reserved) and the dependency-policy exposure of parsing third-party YAML with `noyalib` 0.0.x.
- **extension-platform, "The catalog is the vocabulary a surface is built from":** The scope is proposed to shrink to what the openspec board needs (a grouped list, markdown, progress, message, confirmation), with the rest added on a second consumer; and M4 (the render cost of `code` highlighting and tables).
- **extension-platform, "Actions are named, offered where they apply, and reachable by key and pointer":** A4 (see the input-keymap delta).
- **extension-platform, "Extensions react to lifecycle events they declared":** `checkout.changed` taken from the badge fires only for the focused checkout, `agent.*` events come from a repaint heuristic, and per-frame coalescing does not give "one run per burst". The event list and wording must be narrowed to what is observable.
- **extension-platform, "Extensions may add CLI commands under their own name":** S11 ("printed as the program wrote it" contradicts sanitization; consent, enablement and confinement must apply to CLI runs), M7 (one clap leaf cannot carry the performance classification, and ADR-019's machine/project split is mixed under `uze ext`).
- **extension-platform, "`uze ext` names extensions, and its own verbs are reserved":** M7 (machine-scoped verbs share a noun with project-scoped commands), S15 (reserve `code`, `architect` and `uze` as well), S4 (`uze ext <id>` is ambiguous when two plugins ship the same id).
- **extension-platform, "The operator can enable, disable and inspect each extension":** The owed decision on machine-wide vs. per-project enablement, and S4 (identity must be `plugin@marketplace/id`).
- **extension-program-contract, "A program is an argv the host runs once per question":** M8 ("no process remains" cannot be tested; `setsid` escapes the process-group kill, so it narrows to the group), S6 (the working directory must be a Git worktree root, never `$HOME`, `/` or an ancestor of `$UZE_HOME`), and S12 (the check's dry run executes untrusted programs).
- **extension-program-contract, "Input is one JSON document on stdin":** S8 (a substituted value starting with `-` is an argument-injection vector wherever inputs reach argv).
- **extension-program-contract, "Output is one JSON document on stdout":** UX (`sh` authors cannot produce correct JSON without `jq`; an `output: lines` or JSONL mode is proposed).
- **extension-program-contract, "Every run is bounded":** S13 (time and output bounds do not bound PIDs, CPU, disk or descendants; a global cap on concurrent runs and a debounce floor are missing).
- **extension-program-contract, "Nothing the workspace draws waits on a program":** A1 (job ordering, one read at a time, panic fallbacks and answers orphaned by a closed surface are orchestrator policy the contract does not yet express) and M4 (building views from large answers must not happen on the render thread).
- **extension-program-contract, "Answers are remembered until something says they changed":** M2 (a disk cache is not allowed from `uze-extensions` and cannot be invalidated across restarts; an in-memory cache per session is proposed, which changes this requirement's "rebuildable" wording).
- **extension-security, "An extension declares every grant it needs":** S2 (`write:checkout` is later code execution via `.git/hooks`, `.envrc` and harness settings), S3 (`exec:` is not enforced at runtime; interpreters and runners grant everything), S7 (bare names resolve on a hijackable `PATH`, including WSL `/mnt/c`).
- **extension-security, "Grants are consented to, recorded, and re-asked when they widen":** S4 (consent keyed by id is inherited by another plugin with the same id), S5 (grants-by-set lets a remote update swap code silently), the owed decision on per-project enablement, and consent wording that must lead with implicit rights (reading the whole checkout, `.env` included).
- **extension-security, "The host executes only what was declared, from where it was declared":** S7 (resolve `exec:` to an absolute realpath at enable time), S8 (argument injection through values starting with `-`), S9 (`exec:git` honours a hostile repo's config and hooks), and the need for the runner to rebuild the argv from the accepted manifest rather than receive it.
- **extension-security, "Programs run with a scrubbed environment and confined where possible":** S1 (AF_UNIX, AF_NETLINK, AF_VSOCK, `io_uring` and `socketcall` escape the network rule: systemd user bus, `docker.sock`, agents, WSL interop), S6 (the checkout is undefined and can be `$HOME`), S10 (the allowlist is too narrow for node, python and worktrees and too loose for `/etc`; partial Landlock ABIs), S13 (fork bombs, `setsid` daemons, disk fill), S17 (signals to other processes), S18 (confinement failing inside a sandboxed parent or container), and whether an extension may be enabled at all where confinement is missing.
- **extension-security, "Nothing an extension produces reaches the terminal unsanitized":** S11 (`uze ext` prints raw output, allowing OSC 52, title changes, links and TIOCSTI, which contradicts this requirement), S14 (Cf and zero-width characters, tag characters, U+2028/2029, stacked combining marks, and width bounded in bytes).
- **extension-security, "Host chrome is never drawable by an extension":** S15 (ids need a `[a-z0-9-]` charset; `code`, `architect` and `uze` must be reserved; attribution must be `id · plugin@marketplace`, never the display name, which allows homoglyphs).
- **input-keymap, "Extension actions are keymap actions in the extension's scope":** A4 (`Action` is `Copy` with a `const` list used at about 730 sites; `Keymap::new` refuses a chord reaching two actions in one scope, so two extensions sharing one scope with the same suggestion fail the whole load; a second table with per-slot scopes and interned ids is proposed; "destructive never holds an unmodified letter" must follow from a declared confirmation).
- **plugin, "A plugin may carry extensions":** A5 (discovering extensions as routed package resources sends them into harness delivery, contradicting "never to a harness"; dedicated discovery is proposed), and removal must say `uze plugin remove`, because dropping a plugin from `agents.yaml` keeps it in the Store.
- **plugin, "An extension is checked offline before it is installed":** S12 (the dry run executes untrusted sources without consent, unconfined where confinement is missing: run only with `--run` under enforced confinement, or against `--checkout` or canned answers) and M8 (verifying every field reference needs `examples:` fixtures and is best effort otherwise).
