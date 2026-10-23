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

A declaration names its output mode:
- `json` (the default): a list of objects or an object, for `bind`.
- `lines`: one item per line, tab-separated fields named by the
  declaration, so a `sh` author needs no `jq` (the UX finding).
- `view`: a catalog tree, for `compose` (D15).
- `cells`, `image`, `sprite`: the tier-two answers (D13), each bounded in
  size before it is decoded.

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
| `view` | the full frame, opened like `code` | the frame | layouts `sidebar` or `board`, as before |
| `section` | a collapsible block of the workspace sidebar | sidebar width, rows the host grants | the timeline's place |
| `status` | a segment of the workspace's status bar | one row, a width the host grants | ordered by the operator, never by the extension |
| `band` | along the top or bottom edge of an agent's pane, inside its frame | pane width, at most 6 rows | per pane; the pane's harness is unaware of it |
| `overlay` | a picker, form or dialog over the frame | host-chosen | modal, under host chrome, opened only by a gesture |
| `notify` | an OS notification, the terminal bell, a sound cue | — | rate-limited, attributed |

- The host owns geometry. A slot answers with what fits the size it was
  handed; a section or band that does not fit is folded, not cropped.
- A `band` is how the gsd-band recreation (D20) sits beside the agent it is
  about without the harness. It never covers the pane's cells: the pane is
  laid out shorter.
- No slot reaches a harness's own output. Rewriting a transcript row is a
  non-goal.

### D15. The catalog in families, bound or composed

The catalog is the vocabulary of `view.rs`, grown in families. Every node is
a meaning the host draws in the active theme; colour is a `Role`, glyphs are
`Symbol`s, emphasis is `strong`/`muted`/`code`.

| Family | Nodes |
|---|---|
| layout | `stack` (vertical), `row` (horizontal, weighted), `split` (two panes, a draggable edge), `tabs`, `gap`, `rule` |
| text | `text` (spans with role and emphasis), `markdown`, `code` (language), `log` (lines with a tone) |
| data | `list` (flat, grouped, tree), `table` (named columns, sortable), `details` (key/value), `grid` (tiles) |
| measure | `progress` (single or segmented, each segment with a weight, a ratio and a role), `gauge`, `pips` (done of total), `chart` (bar, sparkline), `heatmap` |
| status | `badge`, `marks` (several role-coloured marks in a row), `chip`, `activity` (a mark the host animates while a named agent or run is busy), `elapsed` (a duration the host ticks) |
| media | `image` (bytes or a packaged file; Sixel or half blocks), `sprite` (frames per state; the host plays them) |
| cells | `cells` (a grid of `{ch, role}` with clickable `regions`), tier 2 and above |
| input | `button`, `toggle`, `select`, `field` (text), `form` (typed fields, validation messages from the program), `picker` (fuzzy list the host filters) |
| feedback | `toast`, `notice`, `confirm`, `notify`, `sound` (a named cue, not audio bytes) |

Two ways to build a surface from it:
- **bind**: the manifest names nodes and fills them with field references
  into a source's data (`{name}`), as in the first draft. No logic.
- **compose**: a source answers a tree of catalog nodes as JSON (output mode
  `view`, D4). The logic lives in the program; the host validates the tree
  against the catalog, sanitizes every string, and draws it. Every
  interactive node names a declared action and the whole-value arguments it
  passes, so a composed tree can only invoke what the manifest declared.

Animation is the host's for every stateless tier: `activity`, `elapsed`,
`sprite` and segmented `progress` transitions are driven by the host's
clock, so no program runs to make something move.

The catalog grows by the rule `view.rs` already states, made concrete: a
node joins when a gallery entry (D20) needs it, and leaves nothing behind
that only one extension uses.

*Alternative:* bind only. Rejected: the gsd-band board needs a per-phase
segmented bar and a status per row whose count the manifest cannot know
ahead, which a template cannot express without becoming an expression
language. *Alternative:* compose only. Rejected: a list over a CLI should
not need a program that prints UI.

### D16. The agent bridge

**Events.** UZE delivers a reporting hook of its own to every harness, as a
portable hook through each integration, on the events each harness offers
(turn start and end, tool call before and after, approval asked, session
start). The hook reports to the workspace client of the agent it runs under
and nothing else; the client normalizes it into `agent.turn.started`,
`agent.turn.finished`, `agent.tool.started`, `agent.tool.finished`,
`agent.approval.asked` and `agent.session.started`, each carrying the
agent's id, harness, checkout and, for tools, the tool's name and a bounded
summary. Where a harness lacks an event, the bridge says so per harness and
the paint heuristic still gives turn start and end.

**Read model.** A built-in source, `agents`, answers the agents of the
workspace: id, harness, state, work label, checkout, branch, and usage where
the harness reports it. It is what the claude-hud and flightdeck recreations
read instead of scraping transcripts.

**Actions.**
- `agent.prompt`: a text, or a skill invocation (`skill` plus arguments)
  that the agent's integration spells in its harness's dialect, written into
  the agent's pane as if typed.
- `agent.interrupt` and `agent.restart`: the harness's own interrupt and
  new conversation, through the integration.

Safeguards:
- `agent:prompt` and `agent:control` are grants, consent to which says they
  give the extension what the agent can do.
- From a gesture, the host shows the exact text before sending and names the
  extension.
- **From an event or a pipe**, only under the stronger grant
  `agent:prompt:auto`, consented to separately. Unattended prompts are
  rate-limited per agent, never sent while the operator is typing in that
  pane or while a turn is running (they queue, bounded), and each one is
  marked in the pane's band or frame as sent by the extension. The operator
  can pause every unattended prompt with one key.
- Only agents whose checkout the surface is open on are reachable.

*Alternative:* no agent actions. Rejected: the gsd-band's value is its next
action button, and every harness that allows it (mods, OpenCode, the Codex
app-server, Antigravity sidecars) is used for it.

### D17. Reasons to refresh, and pipes

- `every: <duration>` runs the sources it names on an interval, with a floor
  of 5 seconds, and only while a surface that shows them is visible.
- `paths.changed: [<paths>]` runs them when a declared path inside the
  checkout changes, from a bounded periodic digest of their modification
  times; no watcher, no new dependency. It also covers paths Git ignores.
- `checkout.changed` is computed per checkout an open surface is on, not
  only the focused one.
- `agent.*` comes from the bridge (D16), not from repaints.
- **Pipes.** `uze ext pipe <extension> <name> [payload]`, another
  extension's action, or a hook sends a named message to an extension. The
  manifest maps a pipe to sources it refreshes (the payload is their input)
  or to an action. A pipe to an extension with no open surface wakes nothing
  resident: it marks its sources stale, and the next surface to open reads
  them. A pipe payload is data: sanitized, bounded, and never a reason to
  write into an agent unless `agent:prompt:auto` was granted.
- Events are coalesced per extension in a window (250 ms by default): a
  burst runs each named source once.

### D18. Store and preferences

- **Store**: a per-extension key/value document, bounded (1 MiB), that
  programs read in their input and write in their answer (`store` field);
  the host applies the write. It is keyed by the extension's identity
  (`plugin@marketplace/id`, S4), lives in the tier its deletion costs (the
  `persistence-tiers` rule decides which), and is removed with the plugin.
- **Preferences**: typed fields the manifest declares (`text`, `number`,
  `toggle`, `choice`, `secret`), edited on the Extensions screen, passed in
  the program's input only when the declaration names them. A `secret` is
  never shown again after entry, never logged, and never in a pipe.

### D19. The resident tier: a WebAssembly component

A resident surface is fed by one component per extension, built against a
WIT world UZE publishes:

- **Exports**: `start(context)`, `event(event) -> redraw?`,
  `input(input) -> redraw?` (keys and pointer while its surface is focused),
  `render(slot, size) -> tree | cells`, `stop()`.
- **Imports** are the grants. `run-program` (the D4 runner, under the same
  grants), `fetch` (only with `net`), `store`, `clock` and `timer`,
  `read-checkout` (read-only, inside the checkout), `agent-prompt` (only
  with `agent:prompt`), `notify`. A component that imports what its grants
  do not cover is refused at enable time; there is no ambient WASI
  filesystem, network or environment.
- **Budgets**: fuel per call, a memory ceiling, at most one render per frame
  the host asks for and a frame rate cap (30 per second); a component that
  exceeds a budget is stopped and its surface shows why. The component runs
  off the drawing thread; a render that misses its frame keeps the last one.
- **Confinement** is the component model's, on every platform, including
  where Landlock or Seatbelt are absent; the programs it runs through
  `run-program` keep the D6 confinement.
- **The author compiles; UZE only loads.** A plugin ships the built
  component (`extensions/<id>/<name>.wasm`), stored and digested like any
  other bytes, so what was consented to is what runs. One file runs on every
  platform and architecture. UZE never builds an extension at install; the
  check validates the component's imports against its grants, and the Skill
  names the toolchains (`cargo component`, `jco`, TinyGo) without UZE
  depending on any.
- **Runtime**: owed (see *Product decisions owed*). `wasmi` (an interpreter,
  pure Rust, the one Zellij embeds) and `wasmtime` (a JIT, larger, faster)
  are the candidates; the dependency policy decides between them.

*Alternative:* a resident process speaking JSON over stdio. Rejected: a
process is confined only where the OS can confine it, and it costs a
process per extension.

### D20. The gallery

Ten reference extensions, each recreating a plugin operators use on a
harness, live as fixture plugins and are the acceptance suite of the
platform. Each names the primitives it needs; together they cover every
family, slot and tier.

| Reference | Recreates | Exercises | Tier |
|---|---|---|---|
| `gsd-band` | luizfbalves/claude-gsd-band | `band`, composed segmented `progress`, `marks`, `pips`, `paths.changed`, `every`, `agent.prompt` with a skill | declared |
| `agents-hud` | claude-hud, ccstatusline | `status`, the `agents` source, `activity`, `elapsed`, `gauge` | declared |
| `flightdeck` | claude-flightdeck, opencode-agents-monitor | `view` and `section`, agent events across harnesses, `tabs`, `table` | declared |
| `gpu-meter` | opencode gpu-sidebar | `section`, `every`, `chart` sparkline, `exec` | declared |
| `chimes` | peon-ping, opencode-notifier | agent events, `notify`, `sound` | declared |
| `jump` | zellij room | `overlay` `picker`, a host action to focus an agent | declared |
| `plan-review` | plannotator | `view`, `markdown`, `form`, `agent.prompt` with the review | declared |
| `pets` | Codex pets, claude-code-tamagotchi | `band`, `sprite` by agent state, `store` | cells and media |
| `screen` | opencode-mobile-use | `section`, `image` refreshed on `every` | cells and media |
| `snake` | claude-code-mods snake | `view`, `cells` every frame, raw keys, `timer`, `store` | resident |

Not recreated, and why: tdd-guard and ctx-handoff are hooks and skills,
which plugins already deliver; prismantis rewrites a transcript (non-goal);
reels streams video at a cost the gallery does not need to prove.

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
  lacks cannot be built] → Composing (D15) and the cells tier lift most of
  it without a resident process; the resident tier (D19) is the escape
  hatch, and the gallery (D20) is the test of whether the ceiling is high
  enough.
- [`agent:prompt` gives an extension everything the agent can do, which is
  more than any other grant] → Consent says so in those words; a gesture
  always shows the exact text; unattended prompts need their own grant and
  are rate-limited, queued behind a running turn, marked in the pane and
  pausable with one key (D16). An extension with `agent:prompt` and `net`
  can exfiltrate through the agent; consent states the combination.
- [A composed tree or a cell grid can imitate host chrome] → Composed nodes
  are drawn inside the slot's frame, which the host draws and attributes;
  no node draws a frame title, a toast or a confirmation of its own.
- [A reporting hook in every harness is a new thing UZE writes into each
  harness's configuration] → It travels as a portable hook through each
  integration's existing delivery, so it is native where hooks are, and its
  absence on a harness is reported, not papered over.
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
rebuildable. The reporting hook (D16) is a new portable hook each
integration delivers; removing it reverts agent events to the paint
heuristic. The built-in extensions move behind the contract in the first
group of tasks, with no observable change. Rollback is reverting the change:
a plugin's `extensions/` directory is then inert bytes in the Store that no
harness reads.

## Diagrams

`docs/architecture/crate-layering.mmd` gains `uze-contract` and its
edges (from `uze-core` and `uze-extensions`). `containers.mmd` gains the
extension program as an external process the workspace client runs, the
resident component as a sandbox inside it, and the reporting hook as an edge
from each harness to the workspace client. Both are validated with
`cargo test -p uze-extensions` and `make artifacts`.

## Unresolved review findings

Two reviews ran on 2026-09-27, one on architecture fit and one adversarial on
security and product. The plan was frozen with their findings recorded here.
Each finding names the decision above that it invalidates. **None is resolved
yet, and implementation waits on all blockers and criticals.**

### Proposed split (pending decision, revised 2026-10-23)

This change is eight changes, each shipping one or more gallery entries
(D20) as its proof:

1. **generic-extension-host**: group 1 only, plus the ADR, invariant and
   AGENTS.md edits, with no new behaviour.
2. **extension-manifest-and-runner**: `uze-contract`, discovery, check, the
   bounded runner, output modes, the grants record and consent. Confinement
   is disclosed as not enforced.
3. **declared-extensions**: the engine (bind and compose), the keymap,
   `applies_when`, the `view`, `section` and `status` slots, and the catalog
   the first entries need. Proof: `gpu-meter`, `jump`, the openspec board.
4. **extension-confinement**: Landlock, seccomp and Seatbelt, with the
   dependency review.
5. **agent-bridge**: the reporting hook per integration, the normalized
   events, the `agents` source, the `band` slot and the agent actions.
   Proof: `gsd-band`, `agents-hud`, `flightdeck`, `chimes`, `plan-review`.
6. **extension-media**: `cells`, `image`, `sprite`. Proof: `pets`, `screen`.
7. **extension-resident**: the WebAssembly tier and its runtime. Proof:
   `snake`.
8. **Growth**: store, preferences, pipes, CLI commands and the Skill, each
   when a gallery entry needs it.

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
- **The WebAssembly runtime** (D19): `wasmi` or `wasmtime`, judged by the
  dependency policy, with the cost of each on build time and binary size.
- **Who orders the status bar** (D14): the status bar is the operator's;
  whether a new segment appears until placed, or stays hidden until placed.
- **Unattended prompt limits** (D16): the rate per agent, the queue bound,
  and whether `agent:prompt:auto` may be granted machine-wide at all or only
  per project.

### How the 2026-10-23 revision bears on the findings

It resolves none of them outright. It changes these:
- *Events* (minor): D17 replaces the badge-derived, focused-only
  `checkout.changed` and per-frame coalescing; D16 replaces the repaint
  heuristic for `agent.*`. The event requirement is rewritten to what each
  source can observe.
- *`output: lines`* (UX): adopted in D4.
- *Enabling where confinement is missing*: the resident tier is confined on
  every platform; programs are not, so the decision is still owed for them.
- *`kind: interactive`*: retired in favour of the resident tier (D13).
- *M4*: compose (D15) makes it more pressing, and the risk names the
  direction.

It opens these, to be reviewed with the rest: the agent bridge's reach
(D16), composed and cell answers imitating chrome (D15), pipes as a way in
for any local process (D17), and the resident tier's budgets (D19).

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
