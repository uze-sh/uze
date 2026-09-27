## Context

See proposal.md for why. What the design has to work with:

- **The contract types already hold.** An extension answers with a
  `view::View` or a `view::Section`, receives `Command` and `ViewHit`, and
  reaches the machine only through `uze_extensions::Host`
  (`crates/uze-extensions/src/lib.rs`). `tests/architecture/layering.rs`
  forbids `uze_core`, `uze_application`, `std::process`, `std::fs` and
  `crossterm` in `crates/uze-extensions/src`.
- **Everything past the contract is hand-wired per extension.** Each
  extension has its own model fields (`code`, `architect`, `timeline`,
  `code_measures`, ... in `src/ui/orchestrator.rs`) and its own `ExtensionHit`
  variant (`Code`, `CodeTimeline`, `Architect`). Each has its own
  outcome enum and `follow_*` (`src/ui/orchestrator/session.rs`), its own
  channels and `spawn_*`/`absorb_*` pairs, and its own `render_extension`
  branch. It has its own remembered place map (in memory only), and its
  keymap scopes and actions are hardcoded in `crates/uze-keys`: `Scope` and
  `Action` are closed macro enums.
- **The registry is metadata.** `ExtensionRegistry::builtin()` lists
  `BuiltinExtension` records, and the Extensions screen is static.
- **Plugins are discovered by layout.** `delivery/engine.rs`
  `package_resources_at` walks a package for `skills/`, `hooks.json`,
  `mcp.json`, `agents/` and `AGENTS.md`. Trust (`package/trust.rs`) asks only
  when a source crosses the trust boundary (remote Git, embedded), and it
  records nothing: "not a permission system".
- **Process building blocks exist** in `machine/subprocess.rs`:
  `with_process_group`, `wait_with_timeout` (kills the group), `read_bounded`,
  and the libc `kill_process_group`. No helper clears the environment; see
  `acquisition/git.rs` `run_as` for the pattern.
- **YAML is read by `noyalib` 0.0.x**, in `uze-core` only. `schemars` exists
  only as an optional dev-server dependency.

## Goals / Non-Goals

**Goals:**
- One host path for every extension, with `code` and `architect` on it and
  no observable change to either.
- A declared extension that is data plus stateless programs, with zero
  resident processes when idle.
- A catalog wide enough that a list-over-a-CLI tool never needs to draw.
- A security model where the operator's consent, the manifest and the host's
  enforcement are the same list.

**Non-Goals:**
- The long-lived interactive protocol (free drawing, editors, custom input).
  `kind: interactive` is reserved and refused.
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
| **Domain** | finding `extensions/*/extension.yaml` in a package, parsing it with the YAML reader `uze-core` already has, checking it, grants, running programs confined | `uze-core` (`capability/extension`, `machine/confine`), `uze-application` (read model, `ExtensionRunner`) |
| **Presentation** | the generic `Extension` contract, the declared-extension engine (manifest + answers → `View`/`Section`, view state), the catalog's view types | `uze-extensions` |

The leaf crate is what lets `uze-core` (check, install, grants) and
`uze-extensions` (drawing) share one definition of the manifest without either
reaching the other. That is the same move as `uze-theme` and `uze-document`.
The layering rules are updated to admit exactly this crate into
`uze-extensions`, and a new rule keeps the crate itself free of I/O,
`uze_*` dependencies and rendering libraries.

*Alternatives:* define the manifest in `uze-extensions`. Rejected: it would
make `uze-core` depend on presentation. Define it in `uze-core` and mirror it
in `uze-extensions`. Rejected: two definitions of one schema drift.

### D2. The generic contract: `Extension` plus opaque jobs

```rust
pub trait Extension {
    fn id(&self) -> &str;
    fn surfaces(&self) -> &[SurfaceDecl];            // view / section, title, applies_when
    fn view(&self, surface: SurfaceId, space: Size) -> Option<View>;
    fn section(&self, surface: SurfaceId) -> Option<Section>;
    fn command(&mut self, surface: SurfaceId, command: Command, space: Size) -> Outcome;
    fn hit(&mut self, surface: SurfaceId, hit: ViewHit, space: Size) -> Outcome;
    fn scroll(&mut self, surface: SurfaceId, target: ScrollTarget, direction: ScrollDirection);
    fn event(&mut self, event: &Event);
    fn jobs(&mut self) -> Vec<Job>;                  // work that needs the Host
    fn absorb(&mut self, answer: JobAnswer);
    fn place(&self) -> Place;                        // opaque, handed back on resume
    fn resume(&mut self, place: Place);
}
pub struct Job { pub run: Box<dyn FnOnce(&dyn Host) -> JobAnswer + Send> }
pub struct JobAnswer(Box<dyn Any + Send>);
pub enum Outcome { Stay, Close, Copy(String), OpenPath { .. }, Toast { title, detail }, Notice(Span) }
pub struct ExtensionHit { pub extension: ExtensionSlot, pub surface: SurfaceId, pub hit: ViewHit }
```

- **Opaque jobs replace the typed `spawn_*`/`absorb_*` pairs.** An extension
  hands the host a closure that needs a `Host` and gets back the value that
  closure produced. The orchestrator gets one pair,
  `spawn_extension_job`/`absorb_extension_answer`, and `WorkspaceHost` still
  appears only inside `thread::spawn`
  (`the_workspace_client_reaches_for_git_only_from_a_thread` keeps holding).
  `code`'s diff, refresh, file and measure reads, and `architect`'s artifact
  read, become jobs. Each answer still carries the question it answers, so
  staleness stays the extension's to judge, as it is today.
- `ExtensionHit` becomes a struct. The per-extension variants existed only to
  say "whose row 3", which the slot and surface now say.
- `Outcome` is one enum. `code`'s `Copy` and `architect`'s `OpenPath` are its
  first members.
- `Place` replaces `Remembered.code_places`/`architect_places` with one map
  keyed by (extension, checkout).
- The registry becomes a factory (`ExtensionRegistry::open(id, context) ->
  Box<dyn Extension>`) over built-ins plus the installed declared extensions,
  which the host supplies as data from `uze-application`.

*Alternative:* an enum of request kinds in the contract. Rejected: every new
extension would widen a shared enum, which is the closed-set problem again.

### D3. The declared-extension engine lives in `uze-extensions`

`declared/` implements `Extension` from a `Manifest` (leaf crate) and holds
all view state: selection, folds, scroll, subject, mode, menus, confirmation.
A job it hands the host is always "run program P with input I". The host
fulfills it through a new `Host::run_program(ProgramRequest) ->
ProgramAnswer`. That method is the grant that did not exist, stated as
narrowly as ADR-048's: run *a declared argv of this extension*. It is never a
free-form command. Field references (`{name}`) are resolved by the engine
against the answer or the selection, and they only ever produce whole strings.

### D4. The program contract reuses the hook contract's shape

Details are in `specs/extension-program-contract`. The runner is a new
`uze-core` function next to `subprocess.rs`'s helpers. It composes
`with_process_group`, `read_bounded` on both streams, `wait_with_timeout`,
an allowlisted environment, and confinement (D6). Ceilings are 10 s and
1 MiB of stdout / 64 KiB of stderr by default. A manifest may lower them
and never raise them. The runner is the only place in the workspace that
spawns an extension program. An architecture rule sanctions it by name, the
way `uze-git` and `acquisition::git` are sanctioned.

Answers are remembered in `cache/extensions/<id>/`, keyed by (checkout,
source, input digest). That is the *remembered* tier: deleting it costs one
run.

### D5. Consent is enabling, and it is recorded

Trust today answers "may these bytes be installed" and records nothing. An
extension needs a different question: "may this code run with these grants".
So consent to an extension is **enabling** it, separate from installing the
plugin:

- The grants the operator accepted are a **record** at
  `state/extensions.json`, per extension id: the accepted grants, the digest
  they were accepted at, and enabled/disabled. It declares its shape through
  `uze_document::Shaped`, and its path is named in `UzeHome`.
- Every source asks, local ones included (unlike `trust.rs`), because what is
  consented to is execution on this machine, not provenance.
- An update is compared by **grant set**, not by digest. Only a widening
  holds the extension. That keeps a linked local marketplace editable
  without a prompt on every save.
- Where consent is asked:
  - `uze install`, on a TTY, prompts after the plugin installs, using the
    prompt style of `PromptingAuthority`.
  - A non-interactive install leaves the extension disabled and reports it.
  - The TUI Extensions screen has Enable/Disable with the grants listed.
  - `uze ext enable|disable|list` from the CLI.
- Built-in extensions are part of the binary and need no grants.

*Alternative:* extend `trust.rs`'s `executable_capabilities` so installing
is the consent. Rejected for two reasons. Trust is skipped for local sources.
And holding a widened update would need a stored grant, which `trust.rs` is
deliberately without.

### D6. Confinement: Landlock on Linux, Seatbelt on macOS, disclosed elsewhere

This is the precedent other agent CLIs set for running untrusted helpers.

- **Linux**: Landlock, applied in the child between fork and exec. The
  filesystem is read-only on the checkout, the extension's Store directory
  and system paths (`/usr`, `/bin`, `/lib*`, `/etc`). Writing is allowed only
  in the checkout, and only with `write:checkout`. The rest of `$UZE_HOME` is
  denied. Network is denied via Landlock TCP rules (ABI ≥ 4) plus a small
  seccomp filter on `socket(AF_INET|AF_INET6)` unless `net` is granted. The
  ruleset is built before fork, so the child only restricts itself.
- **macOS**: `sandbox-exec` with a generated profile expressing the same
  three questions.
- **Unavailable** (old kernel, Landlock disabled, other platforms): programs
  still run with argv-only execution, a scrubbed environment and bounds. The
  consent screen, the Extensions screen and `uze doctor` say that grants are
  declared but not enforced here.
- **Dependency**: the `landlock` crate, published by the Landlock LSM
  maintainers from `landlock-lsm/rust-landlock`, pure Rust with no C. It is
  judged by the dependency policy before it is taken. The fallback is
  hand-written syscalls over `libc`, which the workspace already has. The
  seccomp filter is written over `libc` (`prctl` plus a fixed BPF program)
  rather than taking a seccomp crate.

### D7. Everything an extension produces is sanitized once, at the boundary

A single function in the leaf crate strips C0/C1 controls (except `\n` in
multi-line content), ESC-introduced sequences and bidi override/isolate
characters, and bounds the length of a label or a cell. The engine applies it
when an answer is absorbed and when the manifest is loaded, so nothing
downstream sees raw bytes. An architecture test asserts that the declared
engine builds `Span`s only through the sanitizing constructor.

### D8. Keymap: dynamic extension actions in one extension scope

`uze-keys` gains `Action::Extension(ExtensionActionId)`, named
`ext.<extension>.<action>` in `keys.json`, and a `Scope::Extension` that is
live when a declared extension's surface is in front. Registration happens
when the keymap is resolved, from the enabled extensions:

1. A suggested chord that resolves to anything in the live scope stack is not
   bound.
2. A chord the operator's file binds wins.
3. Collisions are reported in `uze doctor`.

Built-in extensions keep their own scopes and actions.

### D9. The catalog grows in `view.rs`, drawn by `src/ui/widget`

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

No filesystem watcher and no new dependency:
- `checkout.changed` is derived from the change badge's periodic read
  (a digest of the status it already computes).
- `agent.started`/`agent.finished` come from the agent-activity signal.
- `work.named` comes from the work label.
- `install.converged` comes from the TUI's lifecycle operations.
- `rebase.paused` comes from the delivery flow.
- `theme.changed` comes from the theme switch.

Events are coalesced per extension and per frame.

### D11. CLI

A named `Command::Ext { args: Vec<String> }` variant, not the existing
`External` shorthand catch-all:

| Command | Classification in `command_performance.rs` |
|---|---|
| `uze ext list` | Budgeted (reads the record) |
| `uze ext enable <id>` | JustifiedSlow (prompt) |
| `uze ext disable <id>` | Budgeted |
| `uze ext <id> <command> [args]` | JustifiedSlow (runs a foreign program) |
| `uze agent extension check <path>` | JustifiedSlow |

`uze agent plugin create --extension` scaffolds a working example: a manifest
plus a `sh` source that needs only `jq`-free JSON printing.

### D12. A dedicated Skill, `uze:extension`, linked from `uze:author`

`uze:author` is the plugin packaging loop (marketplace → scaffold → check →
install → publish). Writing an extension has its own subject matter: the
catalog, bindings, the program contract, grants and what confinement costs,
and the check loop against real answers. Folding that in would roughly double
`author`. So `author` gains one step ("adding an extension? use
`uze:extension`") and the new Skill carries the rest. Its examples are the
openspec board and the scaffolded one.

## Candidate ADRs

- **Extension loading is declared data plus confined, stateless programs.**
  This answers ADR-041's deferred question: the mechanism is chosen, and
  dynamic libraries and resident interpreters are ruled out.
- **Consent to extension code is enabling, recorded per grant set.** This is
  UZE's first stored grant, and a deliberate exception to trust being "not a
  permission system".
- **Landlock/Seatbelt confinement and the `landlock` dependency.** A new
  external dependency with a long tail, and a platform-shaped guarantee.
- **A leaf crate for the extension manifest and wire types.** It widens what
  `uze-extensions` may depend on.

## Risks / Trade-offs

- [Confinement varies by platform, so the same grant means "enforced" on one
  machine and "declared" on another] → It is disclosed on every surface that
  asks for consent and in `uze doctor`. The conformance of the confinement
  itself is proven by a test that attempts each denied operation on Linux CI.
- [`noyalib` is `0.0.x` and now parses third-party input] → Manifest size is
  capped before parsing, the parser is configured to reject duplicate keys
  and alias expansion is bounded, and the fuzz corpus under `check` includes
  alias bombs. Replacing the YAML reader is out of scope but becomes more
  pressing, and is recorded as such.
- [Short-lived processes make a slow interpreter feel slow on every refresh]
  → Answers are cached until an event names them, and a surface keeps
  drawing its last answer while a run is in flight. Per-run durations go
  into tracing spans and `uze doctor`.
- [The catalog is the ceiling: a tool that needs something the catalog
  lacks cannot be built] → That is the price of no drawing. The catalog
  grows by the rule `view.rs` already states (a second consumer), and
  `kind: interactive` remains the named escape hatch for a later change.
- [Migrating `code` and `architect` onto the generic contract touches
  roughly 60 call sites in the orchestrator] → That is done first, alone,
  behind the existing orchestrator and TestBackend tests, before any declared
  extension exists, so a regression is attributable.
- [A job closure can capture anything the extension has] → Closures run with
  only `&dyn Host`. The layering rules still forbid `std::process`/`std::fs`
  in the crate, so a job cannot reach further than an inline call could.

## Migration Plan

This is purely additive for operators. No persisted state changes shape.
`state/extensions.json` is new at shape 1, and the answer cache is new and
rebuildable. The built-in extensions move behind the contract in the first
group of tasks, with no observable change. Rollback is reverting the change:
a plugin's `extensions/` directory is then inert bytes in the Store that no
harness reads.

## Diagrams

`docs/architecture/crate-layering.mmd` gains `uze-contract` and its
edges (from `uze-core` and `uze-extensions`). `containers.mmd` gains the
extension program as an external process the workspace client runs. Both are
validated with `cargo test -p uze-extensions` and `make artifacts`.

## Unresolved review findings

Two reviews ran on 2026-09-27, one on architecture fit and one adversarial on
security and product. The plan was frozen with their findings recorded here.
Each finding names the decision above that it invalidates. **None is resolved
yet, and implementation waits on all blockers and criticals.**

### Proposed split (pending decision)

This change is five changes:

1. **generic-extension-host**: group 1 only, plus the ADR, invariant and
   AGENTS.md edits, with no new behaviour.
2. **extension-manifest-and-runner**: `uze-contract`, discovery, check, the
   bounded runner, the grants record and consent. Confinement is disclosed as
   not enforced.
3. **declared-extensions**: the engine, the keymap and `applies_when`, with
   only the catalog the openspec board needs.
4. **extension-confinement**: Landlock, seccomp and Seatbelt, with the
   dependency review.
5. **Growth**: the rest of the catalog, the remaining events, CLI commands
   and the Skill, each when a second consumer needs it.

Ordering errors inside the current tasks: the keymap (group 8) must come
before the engine's actions (6.4), and the ADR and invariant amendments (11.4)
must come before or with task 1.1.

### Product decisions owed

- **`write:checkout`**: remove it as a raw grant (the program returns edits,
  and the host applies them with a denylist for `.git`, shell and agent
  configuration, and instruction files), or keep it and have consent say it
  is equivalent to running any code as the operator. The openspec archive
  action depends on this.
- **Enablement scope**: machine-wide or per project. The Store is
  machine-wide, but plugins are project-declared, and a machine-wide enable
  runs an extension in every repository.
- **What unenforced consent says**: the plan needs the exact wording for
  "this runs as you, with everything you can reach", and whether an extension
  may be enabled at all where confinement is missing.

### Architecture blockers (invalidate D2, D8, D1/task 4.1)

- **A1. Staleness and job policy live in the orchestrator, not in the
  extension.** D2 is wrong about this. `src/ui/orchestrator.rs:4238-4462` keeps
  a save ahead of the re-read that follows it, runs one read at a time,
  keeps a measure cache that outlives the surface, drops answers by root,
  gives each read a typed fallback when a thread panics, and reports a write
  that failed after its surface closed. The contract needs to state:
  - serial vs. parallel lanes per slot;
  - a required fallback answer;
  - an `orphaned(answer) -> Option<Outcome>` hook;
  - extension memory that outlives a surface, separate from `Place`.
- **A2. The architect's artifact read is not a `Host` call.** It goes through
  `uze_application::project_artifacts` (`src/ui/extension_host.rs:46`). Either
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
  - `invariants.md:616-627` and AGENTS.md ("one `ExtensionHit` variant per
    surface"), which are checked by `invariants_citations.rs`;
  - `invariants.md:342-350` (UZE does not persist trust decisions), which D5
    contradicts.
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
