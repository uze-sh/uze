## 0. Resolve before implementation (plan frozen)

- [ ] 0.1 Decide the split into eight changes (design.md, *Proposed split*), and move groups 1–20 into them
- [ ] 0.2 Settle the product decisions owed: `write:checkout`, enablement scope, unenforced-consent wording, the WebAssembly runtime and HTTP client, the unattended prompt rate
- [ ] 0.3 Resolve architecture blockers A1–A5 and rewrite D2, D8 and discovery accordingly
- [ ] 0.4 Resolve security criticals S1–S6 and rewrite D5, D6 and the extension-security spec accordingly
- [ ] 0.5 Fold the majors, highs and mediums into the specs, design and tasks, or record why each is deferred
- [ ] 0.6 Reorder: keymap before engine actions; ADR, invariant and AGENTS.md amendments with task 1.1
- [x] 0.7 Review the 2026-10-23 revision (D13–D20) for architecture fit, security and authoring, and record its findings beside the earlier ones (design.md, *Second review*)
- [ ] 0.8 Resolve the second review's open items (R-A1 amendments to ADR-056 and ADR-040, R-A5 reserving `spec`, R-A8 several images per frame, R-A10 consent refused in an agent's pane, R-A12 `notify`/`sound` in `uze-platform`, R-A13 stale citations, R-S12 a per-extension memory budget, R-U7 `open` and global keys)
- [ ] 0.9 Amend ADR-059 (writes to a pane stay refuse-not-queue, now also from extensions), ADR-041 (WebAssembly as the only resident form; extension state in `UzeHome`) and the extensions rule (raw keys for the resident tier) with task 1.1

## 1. Generic host contract (no observable change)

- [ ] 1.1 Add `Extension`, `Outcome`, `Job`/`JobAnswer`, `Place`, `SurfaceId` and the struct `ExtensionHit { extension, surface, hit }` to `uze-extensions`, with doc comments that state the contract
- [ ] 1.2 Move `architect` onto `Extension`: the artifact read becomes a job, and `ArchitectOutcome` maps onto `Outcome`
- [ ] 1.2b Move `spec` onto `Extension`: its document and summary surfaces, `spec_places`, and its `Spec`/`SpecSummary` hits
- [ ] 1.3 Move `code` onto `Extension`: the diff, changes, file and measure reads become jobs, the timeline becomes a `section` surface, and `CodeOutcome::Copy` maps onto `Outcome::Copy`
- [ ] 1.4 Turn `ExtensionRegistry` into a factory (`open(id, context) -> Box<dyn Extension>`), keeping `BuiltinExtension` as catalog metadata
- [ ] 1.5 Orchestrator: replace the per-extension model fields, channels, `spawn_*`/`absorb_*` pairs and `follow_*` with one slot map, `spawn_extension_job`/`absorb_extension_answer` (inside `thread::spawn`) and one `follow_outcome`
- [ ] 1.6 Orchestrator: route keys, pointer hits, hover, wheel and scrollbar drags through `ExtensionHit` and the slot, and render every surface through one `render_extension` path
- [ ] 1.7 Replace `Remembered.code_places`/`architect_places` with one `(extension, checkout) -> Place` map
- [ ] 1.8 Keep every existing orchestrator, TestBackend and perf test green, then run `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings` and the architecture suite

## 2. Catalog

- [ ] 2.1 Point `release_notes.rs` at `shared/markdown` (the move itself is done)
- [ ] 2.2 Add the catalog families of D15 to `view.rs`, each node only when a gallery entry (D20) needs it: layout, text, data, measure (segmented `progress`, `gauge`, `pips`, `chart`, `heatmap`), status (`badge`, `marks`, `chip`, `activity`, `elapsed`), input and feedback
- [ ] 2.3 Add the `uze_theme::Symbol`s the new types draw with (bar and sparkline steps, progress fill), resolved in every built-in theme
- [ ] 2.4 Render each new type in `src/ui/extension_view.rs` from the widget vocabulary, with a TestBackend test per type in the default and a light theme
- [ ] 2.5 Make non-numeric chart data and empty tables render as notices or empty states, never panics, each with a test

## 3. Declaration crate

- [ ] 3.1 Create the leaf crate `uze-contract`: `Manifest`, `SurfaceDecl`, `SourceDecl`, `ActionDecl`, `EventName`, `CommandDecl`, `Grant`, and the contract's `ProgramInput`/`ProgramAnswer` as serde types with `deny_unknown_fields`
- [ ] 3.2 Implement the field-reference grammar (`{field}` and `{a.b}` only) with a parser that refuses expressions, and table tests for it
- [ ] 3.3 Implement the sanitizer (C0/C1 controls, ESC sequences, bidi overrides, length bounds) with tests covering OSC 52, OSC 8, CSI cursor moves and RLO
- [ ] 3.4 Reserve the ids `list`, `enable`, `disable`, `help`, `pipe`, `simulate`, `code`, `architect`, `spec` and `uze`, and the protocol version `1`
- [ ] 3.5 Generate the manifest and wire JSON Schema with `schemars` behind a non-default feature, write it to `docs/extensions/schema.json`, and add a test that fails on drift (record the dependency justification in the PR)
- [ ] 3.6 Layering: admit `uze-contract` into `uze-extensions` and `uze-core`, forbid I/O, `uze_*` and rendering crates inside it, and update `crate-layering.mmd`

## 4. Domain: discovery, check and grants

- [ ] 4.1 Add `CapabilityKind::Extension` and discover `extensions/*/extension.yaml` in `package_resources_at`, capping the manifest size before parsing
- [ ] 4.2 Parse manifests with the existing YAML reader (duplicate keys refused), test alias-bomb and oversized inputs, and record the `noyalib` exposure in design risks if the parser cannot bound expansion
- [ ] 4.3 Implement `check_extension`: schema, reserved ids, argv-only programs, relative paths resolving inside the extension (links followed), `exec:` grants covering bare names, `role` values semantic-only, and `kind: interactive` refused pointing at `feed: resident`
- [ ] 4.4 Extend `check_plugin` to run `check_extension` on every extension it carries
- [ ] 4.5 Add the `state/extensions.json` record (accepted grants, accepted digest, enabled), named in `UzeHome` and implementing `uze_document::Shaped` at shape 1
- [ ] 4.6 Implement grant comparison by set, so that an update that widens holds the extension and one that narrows applies, and removing a plugin removes its extensions' records
- [ ] 4.7 Add the extension read model and the `ExtensionRunner` service to `uze-application` (list, enable, disable, grants, run)

## 5. Confined execution

- [ ] 5.1 Implement the program runner in `uze-core`: argv only, allowlisted environment, checkout cwd, one JSON stdin, bounded stdout/stderr, the time limit with process-group kill via `libc`, and exit codes 0/3/other mapped to answer/refusal/failure
- [ ] 5.2 Linux: build the Landlock ruleset before fork (read checkout, extension dir and system paths; write checkout only with `write:checkout`; deny the rest of `$UZE_HOME`; TCP denied without `net`), apply it in the child, and add the `libc` seccomp filter on `socket(AF_INET|AF_INET6)`
- [ ] 5.3 Dependency review for `landlock` (publisher and repository, no C, `cargo deny check`, `cargo tree`), falling back to raw `libc` syscalls if it fails the policy
- [ ] 5.4 macOS: generate the `sandbox-exec` profile for the same three questions
- [ ] 5.5 Detect confinement availability, report it in `uze doctor`, and expose it in the read model
- [ ] 5.6 Tests: a token in the parent environment is absent; writes to `~` and `$UZE_HOME` are denied; TCP connect is denied without `net`; a hang is killed with its children; an output flood is cut off; a link escaping the extension is refused
- [ ] 5.7 Architecture rule: the runner is the only sanctioned spawn of extension programs

## 6. Declared-extension engine

- [ ] 6.1 Add `Host::run_program(ProgramRequest) -> ProgramAnswer` and implement it in `src/ui/extension_host.rs` through `uze-application`'s runner
- [ ] 6.2 Implement `declared/` in `uze-extensions`: `Extension` over a `Manifest`, holding selection, folds, scroll, subject, mode, menu and confirmation, and building `View`/`Section` from the catalog with sanitized spans only
- [ ] 6.3 Sources: resolve declared inputs, emit jobs, absorb answers with staleness by question, and remember answers in `cache/extensions/<id>/` keyed by checkout, source and input digest
- [ ] 6.4 Actions: item menus, footer, confirmation, program or built-in (`clipboard`, `open_path` inside the checkout, `refresh`), and reply `toast`/`refresh` applied, with clipboard and open only on a gesture
- [ ] 6.5 `applies_when` gating of surfaces and sources
- [ ] 6.6 Architecture test: the declared engine builds `Span`s only through the sanitizing constructor
- [ ] 6.7 Engine tests from fixture manifests and canned answers: binding, grouping, tree, every catalog type, refusal notice, failure state and stale answer drop

## 7. Lifecycle events

- [ ] 7.1 (folded into group 15, D17 supersedes D10) Derive `checkout.changed` from the change badge's read, and emit `agent.started`/`agent.finished`, `work.named`, `install.converged`, `rebase.paused` and `theme.changed` from their existing signals
- [ ] 7.2 (folded into group 15) Deliver only declared events, coalesced per extension and per frame, with tests for "undeclared costs nothing" and "a burst runs once"

## 8. Keymap

- [ ] 8.1 Add `Action::Extension(ExtensionActionId)` with `ext.<extension>.<action>` names in `keys.json` and a `Scope::Extension` in `uze-keys`
- [ ] 8.2 Register enabled extensions' actions at resolve time: a suggested chord that collides in the live stack is not bound, the operator's file wins, and collisions are reported in `uze doctor`
- [ ] 8.3 Footer, action index and item menus advertise extension actions with their resolved chords, or by pointer when there is none

## 9. Surfaces for the operator

- [ ] 9.1 Make the Extensions screen list built-in and installed extensions with plugin, grants, enforced/declared confinement and enabled state, plus Enable (showing grants) and Disable
- [ ] 9.2 Show an installed-but-disabled extension (needs consent, or held by a widened update) with the reason and the new grants
- [ ] 9.3 Open declared `view` surfaces from the tab strip and the action index, and draw `section` surfaces in the sidebar under the extension's name
- [ ] 9.4 Attribute host chrome: confirmation dialogs and toasts from an extension name it

## 10. CLI

- [ ] 10.1 Add `Command::Ext` with `list`, `enable` (prompts with grants on a TTY, refuses without one), `disable` and `<id> <command> [args]`, and classify each in `command_performance.rs`
- [ ] 10.2 After `uze install`, prompt on a TTY for extensions needing consent, and report them otherwise
- [ ] 10.3 Add `uze agent extension check <path>`: schema and rules, then a dry run of every source in a scratch checkout under confinement, verifying every field reference against the real answer
- [ ] 10.4 Add `uze agent plugin create --extension`, scaffolding a working manifest and a `sh` source, and extend the scaffold tests so it passes check

## 11. Authoring skill and docs

- [ ] 11.1 Write `plugins/uze/skills/extension/SKILL.md` (`uze:extension`) covering the manifest, catalog, bindings, the program contract, grants and confinement, the check loop, and the openspec board worked example
- [ ] 11.2 Add the step to `uze:author` that hands off to `uze:extension`
- [ ] 11.3 Add the protocol and catalog reference page, linked to the generated schema
- [ ] 11.4 Add the `invariants.md` entries, each naming its test: sanitization, the sole runner, consent before first run, and a widened update being held
- [ ] 11.5 Add the extension program to `containers.mmd`, then run `cargo test -p uze-extensions` and `make artifacts`

## 12. Proof with a real extension

- [ ] 12.1 (dropped: the built-in `spec` already reads OpenSpec; the gallery is the proof, group 19) Build the openspec board as a fixture plugin (sources over `openspec list --json` and the change's files; archive action with confirmation; progress section; `applies_when: [openspec]`)
- [ ] 12.2 Run it by hand in this repository for the operator's validation
- [ ] 12.3 After validation, write the journeys for install → consent → open → act, and for a widened update being held
- [ ] 12.4 Run `make check` and `openspec validate add-extension-platform --strict`

## 13. Slots and feeds

- [ ] 13.1 Add the `status` (a row in the sidebar's header), `band` (fixed rows reserved in `compute_layout`, pane mouse and selection offset), `overlay` and `notify` slots beside `view` and `section`, with the slot size in the program's context and folding instead of cropping
- [ ] 13.1b Draw the host's attribution strip on every extension `view`, uncoverable by an answer
- [ ] 13.2 Let the operator order status rows in the sidebar's header
- [ ] 13.3 Implement `feed: bind | compose`: validate a composed tree against the catalog and a node cap at absorb time, sanitize it, bind interactive nodes to declared actions with their `item`, show resolved arguments in confirmations, ignore clicks right after a re-layout, and memoize the built `View` per answer and size
- [ ] 13.4 Add the output modes `lines` and `view`, and the answer envelope (`data`, `store`, `refresh`, `toast`, `notice`, `notify`, `sound`), to the runner and the check
- [ ] 13.5 Host-animated nodes: `activity`, `elapsed` and segmented `progress` driven by the host's clock, with a TestBackend test that no program runs while they move

## 14. Agent bridge

- [ ] 14.1 Add `uze agent report`, verified by the agent key, appending to a bounded per-agent journal named in `UzeHome`, read by the client's one-second watch; report the bridge unavailable where the journal cannot be written
- [ ] 14.2 Deliver the reporter per launch through each integration's `runtime_contribution`, failing open; grow the portable event vocabulary with turn start and approval asked/resolved where a harness offers them
- [ ] 14.3 Normalize reports into the `agent.*` events with ids, times, outcomes and scrubbed tool names and paths; fall back to the paint heuristic for turns; report per harness what is not observable
- [ ] 14.4 Add the `agents` source answered by the workspace client (state from its activity model, usage typed), under `agent:observe`, across open spaces
- [ ] 14.5 Add the skill-spelling method beside `invocation_prefix` and each integration's list of native skills; `agent.prompt` and `agent.restart` (always confirmed) with `target`
- [ ] 14.6 Write through `submit_notice` behind `may_submit`: the strict filter, byte equality with what was shown, refusal unless ready, foreground and no approval pending; never queue
- [ ] 14.7 `agent:prompt:auto` per project and agent, time-boxed, revoked by any digest change; fixed text or closed set only; rate limit, attribution and the pause key
- [ ] 14.8 Architecture rule: agent actions from an extension go only through the bridge

## 15. Events and pipes

- [ ] 15.1 `every` with a 5-second floor, run only while a surface showing its sources is visible
- [ ] 15.2 `paths.changed` and per-checkout `checkout.changed` from the one-second watch, with a bounded digest of declared paths, Git-ignored ones included
- [ ] 15.3 Event payloads in `input.event`, `deliver: latest | each` with a per-window bound and a `dropped` count, and the built-in `events` source
- [ ] 15.4 `uze ext pipe <extension> <pipe> [payload]`, the `pipe` built-in, and hook delivery of pipes, each with its sender; refresh only; cross-extension pipes accepted from named senders; classified in `command_performance.rs`

## 16. Store and preferences

- [ ] 16.1 Add the store per (`plugin@marketplace/id`, project), or global when declared and consented, bounded, patched from any answer's envelope in arrival order, named in `UzeHome` in the tier the `persistence-tiers` rule gives state, removed with the plugin
- [ ] 16.2 Add typed preferences to the manifest and the Extensions screen, `{pref.*}` in bind templates except secrets; secrets in the OS keychain through `uze-platform` (else a `0600` file outside every path an agent is told about), never redisplayed, logged, piped, cached under or sent to an agent, and tainting the answers of runs that received them

## 17. Cells and media

- [ ] 17.1 Expose `cells` (`{ch, role}` grid with `regions` bound to declared actions and items) on the existing canvas, refusing `Span.color` from declared extensions
- [ ] 17.2 Move the decoders to `shared/`, let `Picture` reach sections, bands and overlays and the painter hold several images per frame, and cap pixels from slot geometry, decoding off the drawing thread
- [ ] 17.3 Add `sprite`: a packaged sheet with frame size, columns and per-state frames, timing and loop, a fallback state, scaled and alpha-composited by the host, played from an agent's state or a source's field

## 18. Resident tier

- [ ] 18.1 Create `uze-extension-runtime` (depending on `uze-contract` and `uze-platform` only) and publish the WIT world with every type (exports `start`, `event`, `input`, `render`, `stop`; imports `run-program`, `fetch`, `store`, `clock`, `timer`, `read-checkout`, `agent-prompt`, `notify`)
- [ ] 18.2 Dependency review of the chosen runtime and HTTP client (0.2), then embed the runtime off the drawing thread behind an `Extension` adapter
- [ ] 18.3 Refuse at check and enable a component whose imports its grants do not cover, or whose digest (checked on the buffer that is compiled) differs from the consented one; `read-checkout` resolves beneath the root with no link followed; `agent-prompt` raises the host confirmation without `agent:prompt:auto`; `run-program` shares the declared sources' limits; enforcement reported per import
- [ ] 18.4 Fuel and memory bounds per call, a 30-per-second frame cap, stop on breach with the bound named
- [ ] 18.5 Raw keys and pointer to a focused resident surface, the host's escape chord excepted

## 19. Authoring

- [ ] 19.1 Write the manifest and wire grammar into the generated schema with one complete worked example per slot and per feed (D21)
- [ ] 19.2 Add `uze ext simulate <path> <declaration>` with canned event, selection, store and size, printing what the host would draw; make it the core of `uze agent extension check`; `--run` required where confinement is not enforced

## 20. Gallery

- [ ] 20.1 Build the ten reference extensions of D20 as fixture plugins, each shipped with the change of the split that proves it
- [ ] 20.2 One journey per reference: install → consent → open → the scenario of the platform spec it proves
- [ ] 20.3 Run `make check` and `openspec validate add-extension-platform --strict`
