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
  `mcp.json`, `agents/` and `AGENTS.md`. Trust (`package/trust.rs`) asks only
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
  stay ruled out (ADR-041). The resident tier is WebAssembly only.
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
| **Domain** | finding `extensions/*/extension.kdl` in a package, parsing it and its views with the bounded KDL decoder (D22), checking it, grants, running programs confined | `uze-core` (`capability/extension`, `machine/confine`), `uze-application` (read model, `ExtensionRunner`) |
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
against the answer or the selection, and in the manifest they only ever
produce whole strings; views follow D23.

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
when the declaration names them, `event`, `pipe`, `store` and
`preferences`.

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

### D13. Three tiers behind one manifest, chosen per surface

A surface's `feed` says how it is fed, and the tier is what that feed
costs:

| Tier | `feed` | Resident | Answers | Extra grant |
|---|---|---|---|---|
| declared | `bind` or `compose` | no | data bound by the manifest, or a catalog tree (D15) | — |
| cells and media | `cells`, `image`, `sprite` | no | a cell grid with regions, an image, a sprite sheet | — |
| resident | `resident` | yes, one component per extension | catalog trees or cells, every frame it asks for | `resident` |

- Tiers share the manifest, the grants record, the keymap, the slots and
  the sanitizer. Moving a surface up a tier is changing its `feed`, not
  rewriting the extension.
- The first two tiers keep the contract of D4: argv, one question, one
  answer, nothing left running. The third is D19.
- `kind: interactive` is retired: the resident tier is what it reserved.

*Alternative:* one tier, resident for all. Rejected: most extensions are a
list over a CLI, and making each one a compiled component would trade the
`sh` author for nothing.

### D14. Slots: the places an extension may occupy

| Slot | Where | Size | Notes |
|---|---|---|---|
| `view` | the full frame, opened like `code` | the frame less the host's attribution strip | layouts `sidebar` or `board`, as before |
| `section` | a collapsible block of the workspace sidebar | sidebar width, rows the host grants | the timeline's place |
| `status` | a compact row in the sidebar's header | one row, sidebar width | no workspace status bar exists (`render.rs:57-67`), and one would shorten every pane; the operator orders segments |
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
- A `view` keeps a one-row strip the host draws (`id · plugin@marketplace`)
  that no answer can cover, so a full-frame surface cannot pass for an
  agent's pane or for UZE.
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
workspace client anything but "open a space" (ADR-056;
`crates/uze-terminal/src/protocol.rs:300-313`). The bridge adds the channel
the codebase already uses for work labels: a pane process proves itself with
`UZE_AGENT`/`UZE_AGENT_KEY` and writes a record the client polls.
- `uze agent report <event> [fields]`, verified by the agent key, appends to
  a bounded per-agent journal named in `UzeHome`; the client's one-second
  watch (`src/ui/orchestrator/reads.rs:575-617`) reads it. Where a harness
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
has, `submit_notice` gated by `may_submit` (`absorb.rs`, `agents.rs:269-278`,
answering Busy, Typed or Unknown), never the operator's key forwarder. What
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
  agent, time-boxed, never machine-wide: an event or a pipe may send only
  the manifest's **fixed text**, or a declared skill with arguments from a
  **closed set** the manifest lists. Nothing from a source's answer, an
  event payload, a pipe payload or the repository becomes prompt text.
  Each write is rate-limited per agent, attributed in the band or a notice,
  and pausable with the host's pause key, which a resident surface cannot
  capture. Any change of the extension's digest revokes it, whatever D5
  says of grant sets.

*Alternative:* no agent actions. Rejected: the gsd-band's value is its next
action button, and every harness that allows it (mods, OpenCode, the Codex
app-server, Antigravity sidecars) is used for it. *Alternative:* a queue for
a busy pane. Rejected by ADR-059.

### D17. Reasons to refresh, and pipes

D10 is superseded by this decision; tasks 7.x are folded into group 15.
- `every: <duration>` runs the sources it names on an interval, with a floor
  of 5 seconds, only while a surface that shows them is visible.
- `paths.changed: [<paths>]` runs them when a declared path inside the
  checkout changes, from the client's existing one-second watch
  (`reads.rs:575-617`) extended with a bounded digest of the declared
  paths' modification times. It covers paths Git ignores.
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
- **Pipes.** `uze ext pipe <extension> <pipe> [payload]`, another
  extension's `pipe` action, or a hook sends a named message. Every pipe
  carries its sender (`operator` from a TTY outside an agent pane,
  `agent:<id>`, `extension:<identity>`, `hook`). A pipe may only refresh
  sources (the payload in `input.pipe`); it never runs an action, so no
  confirmation, write or agent action is reachable through one. A pipe
  from another extension is accepted only if the receiver names that
  sender in its manifest. A pipe to an extension with no open surface marks
  its sources stale and wakes nothing.

### D18. Store and preferences

- **Store**: a key/value document per (extension identity, project),
  bounded at 1 MiB, unless the manifest declares `store: global` and
  consent says so. A program receives it in `input.store` when its
  declaration names it, and any answer may carry a `store` patch in the
  envelope (D4); the host applies patches in arrival order, last write per
  key wins. It is extension state, so its path is named in `UzeHome` in the
  tier the `persistence-tiers` rule gives state an operator would miss, and
  it is removed with the plugin.
- **Preferences**: typed fields the manifest declares (`text`, `number`,
  `toggle`, `choice`, `secret`), edited on the Extensions screen, passed in
  `input.preferences` only when the declaration names them, and readable by
  bind templates (`{pref.name}`) except a `secret`.
- **Secrets** are kept by `uze-platform` in the OS keychain where there is
  one, else in a per-extension `0600` file outside every path an agent's
  pane is told about; never in `state/extensions.json`. A secret is never
  shown again, logged, piped, part of a cache key, or sent to an agent. An
  answer from a run that received a secret may not patch the store or carry
  an `agent.prompt` (tainted), and its strings are checked for the secret
  before they are drawn.

### D19. The resident tier: a WebAssembly component

A resident surface is fed by one component per extension, built against a
WIT world UZE publishes:

- **Exports**: `start(context)`, `event(event) -> redraw?`,
  `input(input) -> redraw?` (keys and pointer while its surface is focused),
  `render(slot, size) -> tree | cells`, `stop()`. The WIT carries every
  type: the catalog tree, a cell, the key and pointer encoding, the event
  variant (agent events, `every`, timer ticks, pipes), the store.
- **Imports** are the grants:
  - `run-program` under the grants of the program it runs, through the same
    rate and concurrency limits as declared sources;
  - `fetch` only with `net`;
  - `read-checkout` only with `read:checkout`, resolving every path beneath
    the checkout root with no symlink followed;
  - `agent-prompt` only with `agent:prompt`, and always raising the host's
    confirmation unless `agent:prompt:auto` holds for that agent, with the
    D16 rules;
  - `store`, `clock`, `timer`, `notify`.

  A component that imports what its grants do not cover is refused at
  enable time; there is no ambient WASI filesystem, network or environment.
- **Budgets**: fuel per call, a memory ceiling, at most one render per frame
  the host asks for and 30 per second; a component that exceeds a budget is
  stopped and its surface shows why. It runs off the drawing thread; a
  render that misses its frame keeps the last one.
- **Confinement** is reported per import: the component's own reach is
  enforced on every platform; `run-program` is enforced only where D6 is,
  and the Extensions screen says so when it is imported.
- **Input**: a focused resident surface receives raw keys and pointer
  except the host's escape chord and the pause key. This amends the
  extensions rule that an extension knows nothing of the keyboard, for
  this tier only.
- **The author compiles; UZE only loads.** A plugin ships the built
  component (`extensions/<id>/<name>.wasm`), stored and digested like any
  other bytes; the digest is checked on the same in-memory buffer that is
  compiled, and any change of it asks again, whatever D5 says of grant
  sets. One file runs on every platform and architecture. UZE never builds
  an extension at install; the check validates the component's imports
  against its grants, and the Skill names the toolchains (`cargo
  component`, `jco`, TinyGo) without UZE depending on any.
- **Where it lives**: a dedicated crate, `uze-extension-runtime`, depending
  on `uze-contract` and `uze-platform` only, wrapped by an `Extension`
  adapter on the host side. Neither `uze-core` (the package manager, which
  the shim uses) nor `uze-application` (which does not depend on
  `uze-extensions`) can host it.
- **Runtime and HTTP**: owed (see *Product decisions owed*). `wasmi` (an
  interpreter, pure Rust, the one Zellij embeds) and `wasmtime` (a JIT,
  larger, faster) are the candidates; `fetch` needs a production HTTP/TLS
  client, which the workspace does not have outside telemetry.

*Alternative:* a resident process speaking JSON over stdio. Rejected: a
process is confined only where the OS can confine it, and it costs a
process per extension.

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
- `uze ext simulate <path> <declaration> [--event …] [--selection …]
  [--store …] [--size …]` runs one declaration under the host's confinement
  against canned inputs and prints what the host would draw, as text. It is
  the core of `uze agent extension check` and the author's test loop, and
  it runs only with `--run` where confinement is not enforced (S12).
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
  `uze-core` (tests by `uze ext test`), at check, install, enable and dev
  reload, after:
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
- The dependency is `kdl` with `default-features = false, features =
  ["span"]`, in `uze-core` (or behind a `uze-contract` feature only
  `uze-core` enables), judged by the dependency policy: one maintainer (the
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
  in several languages (`lookup "t" locale="pt" { … }`, `"{t:next}"`), so
  programs stop emitting role names and built-in sources become usable.
- **Missing fields.** A missing field that is a whole property hides the
  node; inside a mixed string it draws empty and is a check warning against
  the fixtures (D26).
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

### D25. Live development

- **Load from a folder.** `uze ext dev <path>` links an extension into the
  running workspace; `uze workspace --dev-extension <path>` opens one with
  it. A dev extension is not stored or digest-pinned, is marked `dev` in the
  Extensions screen and in its attribution strip, and lasts only the
  session.
- **The same grants and confinement as installed.** There is no dev mode
  that allows more. Consent is asked once per grant set and again when it
  widens. An agent may run `uze ext dev` from its pane; consent appears in
  UZE's chrome for the operator, never in the pane.
- **Hot reload per file**, debounced by 150 ms:

  | Changed | The host |
  |---|---|
  | a view | recompiles it (D22) and redraws from the last answers, running no program |
  | the manifest | rechecks and re-registers slots, keys and events, keeping the view's place |
  | a program | invalidates its answers and reruns the visible sources |
  | a component | restarts it; the store survives |

- **An error never blanks the surface.** It keeps the last good draw and
  shows the error over it, with file, line, column and the suggestion.
- **The inspector**, a drawer opened by a key on a dev extension:
  - *runs*: each run's argv, stdin, stdout, stderr, exit, duration and the
    bound hit, rerunnable with edited input;
  - *events*: received, coalesced or dropped;
  - *state*: `ui.*` values and the store;
  - *grants*: what was denied and why, and whether confinement is enforced;
  - *tree*: the compiled view and why a node is absent (`when` false, field
    missing, `show` not matched).
- **Making things happen.** `uze ext emit <extension> <event> [--data
  <file>]` delivers a synthetic event; the inspector changes the slot's
  size and the theme, injects fake agents, and records a session's real
  events into a fixture file.

### D26. Tests without a TUI

- `uze ext test <path>` runs `tests/*.test.kdl` headlessly, on the host's
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

- Snapshots are cell grids per slot, size and theme; `--update` rewrites
  them.
- `given … answer=` uses a recorded answer, so a test runs nothing and works
  in any CI; `--run` executes the programs under confinement, and only
  where confinement is enforced unless the operator says otherwise (S12).
- `uze agent extension check` runs the tests, so a plugin with a failing
  test is not accepted by the check.
- D21's `uze ext simulate` is one test step run from the command line.
- **The LSP is required work**, not a later nicety: `uze ext lsp` is
  generated from the catalog in `uze-contract` and gives completion, hover
  and the check's diagnostics for manifests, views and tests, since no
  editor completes KDL from a schema.
- Every gallery entry (D20) ships its tests; they are the acceptance suite
  and the worked example of how to test.

## Candidate ADRs

- **Extension loading is declared data plus confined, stateless programs,
  with WebAssembly components as the only resident form.** This answers
  ADR-041's deferred question: the mechanism is chosen, and dynamic
  libraries and native resident interpreters are ruled out.
- **Extensions may act on agents through the bridge.** Writing into an
  agent's prompt gives an extension the agent's reach; the grants, the
  gesture rule and the unattended safeguards are the decision.
- **The WebAssembly runtime dependency.** A large, long-lived dependency
  chosen by provenance.
- **KDL v2 for manifests, views and tests, with UZE's own bounded
  decoder.** A new third-party parser on extension input, with the guards it
  needs and an LSP to maintain.
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
  timed worker thread, only at check, install, enable and dev reload, never
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
- [A resident component can burn CPU in the TUI process] → Fuel and memory
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
per-extension store is new at shape 1, and the answer cache is new and
rebuildable. The per-agent journal (D16) is new and disposable. The
reporter rides each launch's contribution, so removing it reverts agent
events to the paint heuristic and leaves nothing installed in a harness. The built-in extensions move behind the contract in the first
group of tasks, with no observable change. Rollback is reverting the change:
a plugin's `extensions/` directory is then inert bytes in the Store that no
harness reads.

## Diagrams

`docs/architecture/crate-layering.mmd` gains `uze-contract` and its
edges (from `uze-core` and `uze-extensions`). `containers.mmd` gains the
extension program as an external process the workspace client runs, the
resident runtime crate (`uze-extension-runtime`) and its edges, and the
reporter as an edge from each agent's pane, through its journal, to the
workspace client. Both are validated with
`cargo test -p uze-extensions` and `make artifacts`.

## Unresolved review findings

Two reviews ran on 2026-09-27, one on architecture fit and one adversarial on
security and product. The plan was frozen with their findings recorded here.
A second round ran on the 2026-10-23 revision (*Second review*, below).
Each finding names the decision above that it invalidates. **None is resolved
yet, and implementation waits on all blockers and criticals.**

### Proposed split (pending decision, revised 2026-10-23)

This change is eight changes. Each ships the gallery entries (D20) whose
pieces it and the changes before it contain:

1. **generic-extension-host**: group 1, with `code`, `architect` **and
   `spec`** migrated, plus the ADR, invariant and AGENTS.md edits, with no
   new behaviour.
2. **extension-manifest-and-runner**: `uze-contract` and its schema, the
   KDL manifest, discovery, check, the bounded runner, output modes and the
   envelope, the grants record and consent. Confinement is disclosed as not
   enforced.
3. **declared-extensions**: the bounded KDL decoder and compiled views, the
   binding rules and host-held UI state (D22–D24), the engine (bind and
   compose), the dev loop, `uze ext test` and the LSP (D25, D26), the
   keymap with global keys, `applies_when`, the `view`, `section`, `status` and
   `overlay` slots, `every`, `paths.changed`, the store and preferences, and
   the catalog the entries need. Proof: `gpu-meter`.
4. **extension-confinement**: Landlock, seccomp and Seatbelt, with the
   dependency review.
5. **agent-bridge**: the reporter per launch and its journal, the normalized
   events with delivery modes, the `agents` and `events` sources, the `band`
   slot, `notify`/`sound` through `uze-platform`, and the agent actions.
   Proof: `gsd-band`, `agents-hud`, `flightdeck`, `chimes`, `jump`,
   `plan-review`.
6. **extension-media**: `cells` with regions, `image`, `sprite`, the
   decoders moved to `shared/`. Proof: `pets`, `screen`.
7. **extension-resident**: the WebAssembly tier, its crate, runtime and HTTP
   client. Proof: `snake`.
8. **Growth**: pipes and CLI commands, then whatever a new gallery entry
   needs.

Ordering errors inside the current tasks: the keymap (group 8) must come
before the engine's actions (6.4), and the ADR and invariant amendments (11.4)
must come before or with task 1.1.

### Product decisions owed

- **`write:checkout`**: remove it as a raw grant (the program returns edits,
  and the host applies them with a denylist for `.git`, shell and agent
  configuration, and instruction files), or keep it and have consent say it
  is equivalent to running any code as the operator.
- **Enablement scope**: machine-wide or per project. The Store is
  machine-wide, but plugins are project-declared, and a machine-wide enable
  runs an extension in every repository. (`agent:prompt:auto` is decided:
  per project and per agent, D16.)
- **What unenforced consent says**: the plan needs the exact wording for
  "this runs as you, with everything you can reach", and whether an extension
  may be enabled at all where confinement is missing.
- **The WebAssembly runtime and the HTTP client** (D19): `wasmi` or
  `wasmtime`, and which TLS client, judged by the dependency policy with
  their cost on build time and binary size.
- **Unattended prompt rate** (D16): the number per agent per window.

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
- *S4*: the identity `plugin@marketplace/id` now keys the store, pipes and
  grants (D17, D18); D4's cache path, D8's action names and D11's CLI still
  use the bare id and must follow.
- *S12*: `uze ext simulate` runs only with `--run` where confinement is not
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
- **R-A6** The WebAssembly runtime cannot live in `uze-core` or
  `uze-application`; `fetch` needs an HTTP client; raw keys invert the
  extensions rule. → D19 (own crate, owed HTTP, rule amended).
- **R-A7** A band sized by its answer resizes the PTY (SIGWINCH, redraw
  grace); there is no pane frame. → D14 (fixed rows in `compute_layout`).
- **R-A8** Pictures exist only in full-frame views, one per frame; no
  regions; `Span.color` must be refused; decoders are private to `code`. →
  D4, D15, split 6. **Open**: several images per frame.
- **R-A9** D10/D17 and D9/D15 duplicate; proofs needed later splits. →
  D9/D10 superseded, split reordered.
- **R-A10** ADR-055 already stores consent; `uze ext enable` must refuse in
  an agent's pane. → Candidate ADRs. **Open**: D5 and D11 text.
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
