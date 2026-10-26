## 0. Resolve before implementation (plan frozen)

- [x] 0.1 Decide the split into eight changes, visual engine first (design.md, *Proposed split*), and move groups 1–25 into them; change 1 is `openspec/changes/archive/2026-10-24-extension-view-engine` (decided 2026-10-23)
- [x] 0.2 Settle the product decisions owed: `write:checkout`, enablement scope, unenforced-consent wording, the `rhai` dependency review and the HTTP client, the unattended prompt rate (decided 2026-10-26: design.md, *Product decisions owed*; D5, D6, D16, D19)
- [x] 0.3 Resolve architecture blockers A1–A5 and rewrite D2, D8 and discovery accordingly (decided 2026-10-26: D2, D8, *Discovery*; A1–A3 built by `generic-extension-host`, A5 by `extension-view-engine`)
- [x] 0.4 Resolve security criticals S1–S6 and rewrite D5, D6 and the extension-security spec accordingly (decided 2026-10-26: D5, D6, *Triage*; `extension-security` spec)
- [x] 0.5 Fold the majors, highs and mediums into the specs, design and tasks, or record why each is deferred (decided 2026-10-26: design.md, *Triage*, one row per finding)
- [x] 0.6 Reorder: keymap before engine actions; ADR, invariant and AGENTS.md amendments with the commit that creates each fact (decided 2026-10-26: design.md, *Proposed split*, "Order inside change 4"; group headers below carry the step)
- [x] 0.7 Review the 2026-10-23 revision (D13–D20) for architecture fit, security and authoring, and record its findings beside the earlier ones (design.md, *Second review*)
- [x] 0.8 Resolve the second review's open items (R-A1 amendments to ADR-056 and ADR-040, R-A5 reserving `spec`, R-A8 several images per frame, R-A10 consent refused in an agent's pane, R-A12 `notify`/`sound` in `uze-platform`, R-A13 stale citations, R-S12 a per-extension memory budget, R-U7 `open` and global keys) (decided 2026-10-26: design.md, *Triage*; texts in tasks 3.4, 11.6, 14.9, 14.10, 17.2; D5, D8)
- [x] 0.10 Lay the minimal generic path of D2 in the first change (contract, slot map, one spawn/absorb pair in `orchestrator/reads.rs`, one outcome follow-up) for the view engine only, adding no per-extension field, channel or hit variant (done: `extension-view-engine` E3–E4, `generic-extension-host` D1–D7; `declared.rs`, `answers.rs::schedule_engine_jobs`/`absorb_engine_answer`, `ExtensionHit { slot, hit }`)
- [x] 0.9 Amend ADR-059 (writes to a pane stay refuse-not-queue, now also from extensions), ADR-041 (Rhai as the only embedded language, in `uze-script`; extension state in `UzeHome`) and the extensions rule (raw keys for the resident tier), each with the commit that creates its fact (decided 2026-10-26: ADR-041 and the rule's two lines with task 11.7 in change 4; ADR-059 with task 14.9 in change 6; raw keys with task 18.4 in change 7; the texts are in those tasks)

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

- [ ] 2.1 Point `release_notes.rs` at `shared/markdown` directly (`uze_extensions::code::markdown` is a re-export of `shared::markdown::render`, so this is one line; change 4, with the docs)
- [ ] 2.2 Add the catalog families of D15 to `view.rs`, each node only when a gallery entry (D20) needs it: layout, text, data, measure (segmented `progress`, `gauge`, `pips`, `chart`, `heatmap`), status (`badge`, `marks`, `chip`, `activity`, `elapsed`), input and feedback
- [ ] 2.3 Add the `uze_theme::Symbol`s the new types draw with (bar and sparkline steps, progress fill), resolved in every built-in theme
- [ ] 2.4 Render each new type in `src/ui/extension_view.rs` from the widget vocabulary, with a TestBackend test per type in the default and a light theme
- [ ] 2.5 Make non-numeric chart data and empty tables render as notices or empty states, never panics, each with a test

## 3. Declaration crate (change 4, step 1, for what remains)

- [ ] 3.1 Create the leaf crate `uze-contract`: `Manifest`, `SurfaceDecl`, `SourceDecl`, `ActionDecl`, `EventName`, `CommandDecl`, `Grant`, and the contract's `ProgramInput`/`ProgramAnswer` as serde types with `deny_unknown_fields`
- [ ] 3.2 Implement the field-reference grammar (`{field}` and `{a.b}` only) with a parser that refuses expressions, and table tests for it
- [ ] 3.3 Implement the sanitizer (C0/C1 controls, ESC sequences, bidi overrides, length bounds) with tests covering OSC 52, OSC 8, CSI cursor moves and RLO
- [ ] 3.3b `Clean`: a newtype in `uze-contract` sanitizing in `Deserialize`, also dropping Cf, zero-width and tag characters (U+E0000–E007F), U+2028/U+2029 and combining marks stacked beyond two, bounding length in display cells through `unicode-width` (dependency review in the PR); the declared engine builds `Span`s only through it, and the constructor-grep test of 6.6 becomes that assertion (S14)
- [ ] 3.4 Reserve the built-in ids and the protocol version `1`: `RESERVED_IDS` in `crates/uze-contract/src/lib.rs` already holds `code`, `architect`, `spec` and `uze` (R-A5 confirmed 2026-10-26); add `timeline`, with its test, and the charset `^[a-z0-9][a-z0-9-]{0,31}$` (`is_valid_id` already refuses uppercase, a leading digit or hyphen and `--`; add the 32-character bound) (S15)
- [ ] 3.5 Generate the JSON Schema of the compose wire with `schemars` behind a non-default feature, write it to `docs/extensions/schema.json`, and add a test that fails on drift (record the dependency justification in the PR) (change 6, with compose)
- [x] 3.6 In the commit that creates `uze-contract`: amend ADR-041 and AGENTS.md so `uze-extensions` may depend on `uze-contract` only, admit exactly that edge in `layering.rs`, forbid I/O, `uze_*` and rendering crates inside it, and update `crate-layering.mmd` (`extension-view-engine`, amended 2026-10-23)

## 4. Domain: discovery, check and grants (change 4, steps 1–2)

- [x] 4.1 Discover `extensions/*/extension.kdl` through `capability::extension::discover` (A5), capping the manifest size before parsing (`extension-view-engine`: `capability::extension::discover`, consumed by `Extensions::installed`)
- [ ] 4.2 Grow the bounded KDL decoder of group 21 to the full manifest: `source { script | run | fixture, input, every, refresh-on, deliver }`, `action { id, label, on, key, confirm, script | refresh | clipboard | open_path | open }`, `wants`, `events`, with the same refusals (unknown node, repeated property, wrong case, type annotation); `preferences` refused until change 6
- [ ] 4.3 Implement `check_extension` for change 4: schema, reserved ids and charset, `role` values semantic-only, `kind: interactive` refused pointing at `feed: resident`, relative paths resolving inside the extension (links followed), a script naming a host function its grants do not register refused, the grants of D5 only (`read:checkout`, `clipboard`, `open_path`) and every other grant, `run` and `exec:` refused with "arrives in change N"; a `confirm` action with a bare-letter `key` refused; `every` under 5 s raised with a report. Change 5 adds: `exec:` grants covering bare names, `exec:uze` refused, harness names refused through the integration registry, shebang parsing (`#!/usr/bin/env X` and `#!/path/X` need `exec:X`), a packaged native binary listed as "ships a program of its own", general-purpose runners flagged "can run anything", `--` required before a substituted value (S3, S8)
- [ ] 4.4 Extend `check_plugin` to run `check_extension` on every extension it carries (`Extensions::check` exists since `extension-view-engine`; it gains what 4.2 and 4.3 add)
- [ ] 4.5 Add the `state/extensions.json` record, shape 1, per identity `plugin@marketplace/id` (the marketplace by its registered name): accepted grants, accepted digest and time, resolved `exec:` paths (filled from change 5), `enforced_at_accept`, enabled projects (`enabled_in`, as ADR-055 identifies a project); named in `UzeHome` in the state tier and implementing `uze_document::Shaped` (S4, 0.2.2)
- [ ] 4.6 Implement the update rule: by grant set where every accepted grant is enforced on this machine (a widening holds, a narrowing applies, the same set applies and shows "updated <date>, same grants" with the new digest), by digest where any grant is unenforced or `agent:prompt:auto` is held; a change of plugin or marketplace is a new identity; removing a plugin with `uze remove` removes its extensions' records, undeclaring it from a project keeps them (S4, S5)
- [ ] 4.7 Add the extension read model and the `ExtensionRunner` service to `uze-application` (list, enable per project, disable, grants, `answer` with the in-memory session cache keyed by (identity, source, checkout, input digest))
- [ ] 4.8 Digest the whole `extensions/<id>/` tree (manifest, views, scripts, packaged programs, fixtures) over the bytes the loader read into memory once, bounded; compile and run from those bytes, never from a re-read of the disk; a mismatch with the record holds the extension (R-S14)
- [ ] 4.9 Refuse enabling from inside an agent's pane: the Extensions screen and the `uze install` prompt are the only doors; the CLI refuses when `UZE_AGENT` is set or `launched_by` points at a pane, as `uze workspace allow` does (ADR-055); no `--yes`/`--trust` flag for extensions (R-A10)
- [ ] 4.10 Define the checkout for extensions in `uze-application`: a worktree root the workspace knows, canonicalised through `uze-git`, refusing `/`, a volume root, `$HOME` or an ancestor, `$UZE_HOME` or anything above or below it, a temporary directory and a bare repository; checked on every `answer`, with "your home directory is not a checkout" on the Extensions screen (S6)

## 5. Confined execution (change 5)

- [ ] 5.1 Implement the program runner in `uze-core`: the argv rebuilt from the accepted manifest by declaration id (`Host::answer` resolves it; the engine never carries argv), the pinned `exec:` realpath executed with `argv[0] = name`, `no_new_privs`, allowlisted environment, the checkout checked against D6's definition on every run as cwd, `TMPDIR` on the per-run scratch directory, one JSON stdin, bounded stdout/stderr, the time limit with process-group kill via `libc`, and exit codes 0/3/other mapped to answer/refusal/failure; `GIT_CONFIG_PARAMETERS` and `GIT_CONFIG_NOSYSTEM=1` injected as `acquisition/git.rs` does (S3, S6, S9)
- [ ] 5.2 Linux, as `uze-platform`'s Linux implementation of the confinement concept: the `uze __confine -- <argv>` helper, a re-executed single-threaded process applying Landlock, then seccomp, then `execve` (never `pre_exec`); the Landlock read allowlist of D6 (checkout and its Git common dir, extension dir, scratch dir, system prefixes, `/proc/self`, the devices named, the prefix of each pinned path; nothing of the checkout writable; the rest of `$UZE_HOME` denied; execute on system prefixes and pinned directories); the `libc` seccomp family allowlist of D6 (`socket` refused without `net`, AF_INET/AF_INET6 only with it, `socketpair` AF_UNIX only, AF_VSOCK refused on WSL, `io_uring_*`/`ptrace`/`process_vm_*` refused, the audit-arch prologue); Landlock TCP on ABI ≥ 4, scoping on ABI ≥ 6, signals limited by seccomp below it; enforcement recorded per right with `CompatLevel::HardRequirement` (S1, S10, S17)
- [ ] 5.2b Resource limits in the helper: `PR_SET_CHILD_SUBREAPER`, `RLIMIT_CPU` at twice the time limit, `RLIMIT_FSIZE` 64 MiB on the scratch directory, `RLIMIT_NOFILE` 256, `RLIMIT_AS` 2 GiB, never `RLIMIT_NPROC`; a double kill of the group; two concurrent runs in all; a pids cgroup only where the subtree is already delegated (S13)
- [ ] 5.2c Every run verifies `RulesetStatus == FullyEnforced`, that `PR_SET_NO_NEW_PRIVS` and `SECCOMP_SET_MODE_FILTER` returned, and that `sandbox-exec` exited 0; a failure returns the helper's own exit code, the runner marks the run unenforced and refuses it when the record says `enforced_at_accept: true`, asking for consent again (S18)
- [ ] 5.3 Dependency review for `landlock` (publisher and repository, no C, `cargo deny check`, `cargo tree`), falling back to raw `libc` syscalls if it fails the policy
- [ ] 5.4 macOS, as `uze-platform`'s macOS implementation: the `sandbox-exec` profile of D6 (`deny default`, `deny network*` without `net`; TCP, DNS and the TLS mach services with it; Unix sockets denied; `signal (target same-sandbox)`; `process-fork` with the same rlimits), with "unavailable" as the implementation elsewhere; Windows runs under a Job Object (UI limits, kill-on-close, `ActiveProcessLimit` 32, a memory limit, breakaway denied) and is disclosed; the restricted-token model is a later change, `extension-confinement-windows`
- [ ] 5.5 Detect confinement availability per right, report it in `uze doctor`, `uze status` and the Extensions screen ("runs as you" where unenforced), and expose it in the read model
- [ ] 5.5b The "runs as you" consent step: `exec:` grants asked on their own after the rest, with the pinned path (`gh → /usr/bin/gh`), the fixed texts of D27 (confined, unconfined, general-purpose runner), `enforced_at_accept` recorded, re-asked by digest on update where unenforced; a name not found leaves the extension enablable with "`gh` not found" and a "Resolve again" on the Extensions screen; the resolution refuses the checkout, `$UZE_HOME`, tmp and `/mnt/*` (0.2.3, S7)
- [ ] 5.6 Tests: a token in the parent environment is absent; a write into the checkout (`.git/hooks/pre-commit`, `README.md`), into `~` and into `$UZE_HOME` is denied; TCP connect is denied without `net`; a connect to a Unix socket (`docker.sock`, the user bus) is denied with and without `net`; AF_VSOCK is denied on WSL; a signal to the client is refused; a hang is killed with its children and a fork bomb is reaped; an output flood and a scratch flood are cut off; a link escaping the extension is refused; a changed `PATH` still runs the pinned path; confinement that fails to apply refuses the run
- [ ] 5.7 Architecture rule: the runner is the only sanctioned spawn of extension programs, and only the runner asks `uze-platform` for a confinement
- [ ] 5.8 `output: lines` (one item per line, tab-separated fields the declaration names) in the runner and the check; the check warns when a program-backed source has `every` under 30 s (25.5)
- [ ] 5.9 The store (from 16.1): per (`plugin@marketplace/id`, project), 1 MiB, or global only when the manifest declares `store: global` and that grant was consented to on its own; patched from any answer's envelope in arrival order; named in `UzeHome` in the tier the `persistence-tiers` rule gives state; removed with the plugin; `store` in `input` when named, and the `store` host function for scripts (R-S13; proof: `gpu-meter`'s sparkline over it)
- [ ] 5.10 `run_program` as a host function for scripts, under the program's `exec:` grants, the sources' limits and the runner's confinement, reported on the Extensions screen as the platform gives it; `--run` in the check executes programs only where confinement is enforced, else against `--checkout <path>` or recorded answers (S12)

## 6. Declared-extension engine (change 4, step 5)

- [ ] 6.1 Add `Host::answer(Question { identity, declaration, input }) -> Answer` and implement it in `src/ui/extension_host.rs` through `uze-application`'s `ExtensionRunner`, which resolves the declaration from the accepted compile and only then knows whether it is a script, a program or a fixture; an architecture test asserts only `declared/` calls `answer`, and there is no `ProgramHost`
- [ ] 6.2 Implement `declared/` in `uze-extensions`: `Extension` over a `Manifest`, holding selection, folds, scroll, subject, mode, menu and confirmation, and building `View`/`Section` from the catalog with `Clean` spans only
- [ ] 6.3 Sources: resolve declared inputs, emit jobs, absorb answers with staleness by question, and share answers through the runner's session cache keyed by (identity, source, checkout, input digest); nothing written to disk (M2)
- [ ] 6.4 Actions: item menus, footer, confirmation showing the resolved arguments, script or built-in (`clipboard`, `open_path` inside the checkout, `refresh`, `open`), and the minimal envelope (`data`, `refresh`, `toast`, `notice`) applied; clipboard and open only on a gesture; the copy toast shows the copied text, cut for the screen and whole on the clipboard, and a copy of more than one line or over 256 bytes asks first (S16)
- [ ] 6.5 `applies_when` gating of surfaces and sources
- [ ] 6.6 Architecture test: the declared engine builds `Span`s only through `Clean` (3.3b)
- [ ] 6.7 Engine tests from fixture manifests and canned answers: binding, grouping, tree, every catalog type, refusal notice, failure state and stale answer drop
- [ ] 6.8 The attribution strip `id · plugin@marketplace` on every third-party `view` and `section`, uncoverable by an answer, from `Identity`'s `Display`, never the display name (S15, R-S7; from 13.1b)

## 7. Lifecycle events

- [ ] 7.1 (folded into group 15, D17 supersedes D10) Derive `checkout.changed` from the change badge's read, and emit `agent.started`/`agent.finished`, `work.named`, `install.converged`, `rebase.paused` and `theme.changed` from their existing signals
- [ ] 7.2 (folded into group 15) Deliver only declared events, coalesced per extension and per frame, with tests for "undeclared costs nothing" and "a burst runs once"

## 8. Keymap (change 4, step 3)

- [ ] 8.1 `Action::Extension(ExtensionActionId)` interned in `uze_keys::extension` (`intern(name) -> id`, `name(id)`), named `ext.<plugin>@<marketplace>/<id>.<action>` in `keys.json`, with label and description from the manifest, sanitized; `ALL_ACTIONS` and `Scope::Declared` unchanged
- [ ] 8.2 The per-extension second table, registered on enable and dropped on disable: surface actions resolved after `Global` and before `Scope::Declared`, `open` actions after `Global`, `Workspace` and `Surfaces`; a suggestion bound only when `Keymap::resolve` answers `Fallthrough` in the action's stack; the operator's file over the suggestion; a conflict inside one extension dropping that one binding with a `Problem`, never the load; a name of an absent extension a warning; an `open` chord two extensions suggest going to the first by identity; collisions in `uze doctor` and on the Extensions screen beside the action; a confirmed or agent action refusing a bare-letter suggestion; an `open` suggestion needing a modifier or a function key (A4, R-U7)
- [ ] 8.3 Footer, action index and item menus advertise extension actions with `chord_for`'s answer, or by pointer when there is none; disabling an extension drops its table; `press` in `.test.kdl` reaches an extension action through the table

## 9. Surfaces for the operator (change 4, step 7)

- [ ] 9.1 Make the Extensions screen list built-in and installed extensions with plugin, grants, enforced/declared confinement and enabled state per project, plus Enable (showing grants, the toast → dialog on first open of a project that declares the plugin) and Disable; "updated <date>, same grants" with the digest after an update
- [ ] 9.2 Show an installed-but-disabled extension (needs consent, needs enabling in this project, held by a widened update or by a digest change under the digest rule) with the reason and the new grants
- [ ] 9.3 Open declared `view` surfaces from the tab strip and the action index, and draw `section` surfaces in the sidebar under the extension's attribution
- [ ] 9.4 Attribute host chrome: confirmation dialogs and toasts from an extension name it as `id · plugin@marketplace`

## 10. CLI (change 4, step 7)

- [ ] 10.1 Report extensions in `uze status` and `uze inspect <plugin>`: enabled (and in which projects), disabled, needing consent, held by a widened update, "runs as you" where unenforced, with their grants; no `uze ext` noun
- [ ] 10.2 After `uze install`, prompt on a TTY outside an agent's pane for extensions needing consent, and report them otherwise; refuse when `UZE_AGENT` or `launched_by` says a pane (4.9)
- [ ] 10.3 Extend `uze agent plugin check` to extensions: manifest and views, field references against fixtures, the tests with recorded answers, `--run` executing scripts in the engine (programs in change 5, 5.10), `--update-snapshots`
- [ ] 10.4 Add `--extension <id>` to `uze agent plugin create` (see 23.1), and `uze agent plugin lsp`, classified in `command_performance.rs`

## 11. Authoring skill and docs (change 4, step 8)

- [ ] 11.1 Write `plugins/uze/skills/extension/SKILL.md` (`uze:extension`), minimal in change 4 (the manifest, a script, grants, the check loop), growing with each change: catalog, bindings, the program contract, confinement, the openspec board worked example
- [ ] 11.2 Add the step to `uze:author` that hands off to `uze:extension`
- [ ] 11.3 Add the protocol and catalog reference page, linked to the generated schema
- [ ] 11.4 Add the `invariants.md` entries, each naming its test: the declared engine builds `Span`s only through `Clean`; only the runner asks by declaration (`Host::answer`); consent before first run; identity keys the record; what runs is what was digested; a widened update is held; enabling refuses inside an agent's pane; and beside "Remote executable capabilities cross an explicit consent boundary": "Trust (may these bytes be installed) is still never persisted. Consent to run an extension's code is a different question, answered per grant set and recorded in `state/extensions.json` (ADR-041, `add-extension-platform` D5): that record is what lets a widened update be held."
- [ ] 11.5 Add `uze-script` to `crate-layering.mmd` and `containers.mmd` (the extension program joins in change 5), then run `cargo test -p uze-extensions` and `make artifacts`
- [ ] 11.6 Refresh the stale citations (R-A13): the doc of `crates/uze-extensions/src/lib.rs` ("one `ExtensionHit` variant per surface it draws" → "its surfaces tagged with the slot") and of `registry.rs` ("never touches the machine"); design.md already cites symbols
- [ ] 11.7 Amend ADR-041 with the commit that creates `uze-script`: "Amended (extension-manifest-and-scripts): the loading mechanism is chosen. An extension's logic is a Rhai script run by `uze-script`, a crate depending on `uze-contract` and `uze-platform` only, whose engine is built raw and reaches only the functions the extension's grants register; a program (argv) is run by the OS-confined runner where the platform confines it, and consent says where it does not. Dynamic libraries and native resident interpreters stay ruled out. The 'second concrete use case' is the gallery of D20. Extension state (the grants record, the store, the preferences) lives under `UzeHome` and is written by `uze-application` on the extension's behalf: an extension still touches none of UZE's state directly, and asks the host by declaration id, never by argv." And add to `.agents/rules/extensions.md`: "A declared extension's logic is a Rhai script in `uze-script`, the only crate that names `rhai`; `uze-extensions` and `src/ui` never do. The declared engine asks the host for an answer by declaration id (`Host::answer`), never with argv or script text: what runs is what the operator consented to." and "An extension's actions reach the keyboard through `Action::Extension` and a table of that extension's own, resolved after the host's; nothing in `uze-extensions` names a chord."

## 12. Proof with a real extension

- [ ] 12.1 (dropped: the built-in `spec` already reads OpenSpec; the gallery is the proof, group 19) Build the openspec board as a fixture plugin (sources over `openspec list --json` and the change's files; archive action with confirmation; progress section; `applies_when: [openspec]`)
- [ ] 12.2 Run change 4's proof by hand in this repository for the operator's validation: a plugin from a non-linked marketplace with a `view` and a `section` fed by a Rhai script under `read:checkout`, one action with `refresh` and one with `confirm`, installed by `uze install`, consented to on the Extensions screen, opened from the tab strip, driven by key and pointer; and `gsd-band`'s board from `board.rhai` with `paths.changed` and `every`
- [ ] 12.3 After validation, write the journeys for install → consent → enable in a project → open → act, for a widened update being held, and for enabling refused from an agent's pane
- [ ] 12.4 Run `make check` and `openspec validate add-extension-platform --strict`

## 13. Slots and feeds (change 6)

- [ ] 13.1 Add the `status` (a row in the sidebar's header), `band` (fixed rows reserved in `compute_layout`, pane mouse and selection offset), `overlay` and `notify` slots beside `view`, `section` and `drawer`, with the slot size in the program's context and folding instead of cropping; the attribution strip of 6.8 on `overlay`
- [x] 13.1b (moved to 6.8, change 4) Draw the host's attribution strip on every extension `view`, uncoverable by an answer
- [ ] 13.2 Let the operator order status rows in the sidebar's header
- [ ] 13.3 Implement `feed: bind | compose`: validate a composed tree against the catalog and a node cap at absorb time, sanitize it through `Clean`, bind interactive nodes to declared actions with their `item`, show resolved arguments in confirmations, ignore clicks within 300 ms of a re-layout, and memoize the built `View` per (answer digest, size) (R-S11)
- [ ] 13.4 Add the output mode `view` and the envelope's `notify` and `sound` to the runner and the check (`lines` and `store` arrive in change 5, 5.8 and 5.9)
- [ ] 13.5 Host-animated nodes: `activity`, `elapsed` and segmented `progress` driven by the host's clock, with a TestBackend test that no program runs while they move

## 14. Agent bridge (change 6)

- [ ] 14.1 Add `uze agent report`, verified by the agent key, appending to a bounded per-agent journal named in `UzeHome`, read by the client's one-second watch; report the bridge unavailable where the journal cannot be written
- [ ] 14.2 Deliver the reporter per launch through each integration's `runtime_contribution`, failing open; grow the portable event vocabulary with turn start and approval asked/resolved where a harness offers them
- [ ] 14.3 Normalize reports into the `agent.*` events with ids, times, outcomes and scrubbed tool names and paths; fall back to the paint heuristic for turns; report per harness what is not observable
- [ ] 14.4 Add the `agents` source answered by the workspace client (state from its activity model, usage typed), under `agent:observe`, across open spaces
- [ ] 14.5 Add the skill-spelling method beside `invocation_prefix` and each integration's list of native skills; `agent.prompt` and `agent.restart` (always confirmed) with `target`
- [ ] 14.6 Write through `submit_notice` behind `may_submit`: the strict filter, byte equality with what was shown, refusal unless ready, foreground and no approval pending; never queue
- [ ] 14.7 `agent:prompt:auto` per project and agent, for 24 h from acceptance, revoked by any digest change; fixed text or closed set only, at most 512 bytes; the host's rates in `submit_notice`: per (extension, agent) one per turn and one per 60 s, per agent 3 per minute and 20 per hour, 10 s between unattended writes into one pane; the excess dropped with an attributed notice; attribution and the pause key resetting every counter (0.2.4)
- [ ] 14.8 Architecture rule: agent actions from an extension go only through the bridge
- [ ] 14.9 Amend with the commit that creates `uze agent report` (R-A1, 0.9): ADR-056, after "The one request a pane is served is `OpenSpace`…": "Amended (agent-bridge): a pane may also *report about itself*. `uze agent report <event>`, verified by the agent's launch key, appends to a bounded per-agent journal under `UzeHome` that the workspace client polls; the runtime still serves the pane no snapshot, no input and no stop, and a report is a claim the client draws, never a permission (D16)."; ADR-040, after "There is one implementation of the contract, and it is the wrapper": "Amended (agent-bridge): what UZE needs only inside its own workspace, the reporter that tells the client about turns, tools and approvals, is not a portable hook. A delivered hook runs in every session of the harness, UZE or not; the reporter rides each launch through `IntegrationPort::runtime_contribution` and is gone when the launch is."; ADR-059, with `agent.prompt`: "Amended (agent-bridge): refuse, never queue, holds for every writer into a pane, extensions included. An extension's `agent.prompt` goes through `submit_notice` behind `may_submit`, is refused with the reason while the pane is busy, typed into or awaiting an approval, and is never queued; the extension may try again on a later event (`add-extension-platform` D16)."
- [ ] 14.10 `uze_platform::notify`, one implementation per platform and no new crate (R-A12): the terminal's BEL and OSC 9/777 where `graphics.rs`'s capability query says the terminal can, the desktop notification through the tools `tools.rs` models by path (`notify-send`, `osascript`, PowerShell), `None` where there is no desktop as `desktop.rs` does; `sound` is BEL with a fixed cue name; peon-ping's voices stay a declared loss (D20); proof: `chimes`
- [ ] 14.11 Preferences (from 16.2): typed `text`, `number`, `toggle` and `choice` in the manifest and on the Extensions screen, `{pref.*}` in bind templates, `preferences` in `input` when named; `secret` waits for `net` (16.2); proof: `agents-hud`

## 15. Events (change 4, step 6; `deliver: each` and `events` in change 6)

- [ ] 15.1 `every` with a 5-second floor, run only while a surface showing its sources is visible
- [ ] 15.2 `paths.changed` and per-checkout `checkout.changed` from the one-second watch in `orchestrator/reads.rs`, with a bounded digest of declared paths' modification times, Git-ignored ones included; `deliver: latest` coalescing in a 250 ms window
- [ ] 15.3 (change 6) Event payloads in `input.event`, `deliver: latest | each` with a per-window bound and a `dropped` count, and the built-in `events` source
- [ ] 15.4 (out of this change, D11: pipes return only if a gallery entry needs them) `uze ext pipe <extension> <pipe> [payload]`, the `pipe` built-in, and hook delivery of pipes, each with its sender; refresh only; cross-extension pipes accepted from named senders; classified in `command_performance.rs`

## 16. Store and preferences (moved: the store to 5.9, preferences to 14.11)

- [x] 16.1 (moved to 5.9, change 5, with `gpu-meter`) Add the store per (`plugin@marketplace/id`, project), or global when declared and consented, bounded, patched from any answer's envelope in arrival order, named in `UzeHome` in the tier the `persistence-tiers` rule gives state, removed with the plugin
- [ ] 16.2 (preferences moved to 14.11, change 6, with `agents-hud`; this task is the `secret` type, change 8, with `net`) Secrets in the OS keychain through `uze-platform` with no new crate: Windows Credential Manager through `windows-sys`, macOS Keychain through the system's `security` by path or the framework, Linux a `0600` file under `state/extensions/secrets/` outside every path an agent is told about; never redisplayed, logged, piped, cached under or sent to an agent, `{pref.*}` excluded, and tainting the answers of runs that received them (R-S8)

## 17. Cells and media (change 7)

- [ ] 17.1 Expose `cells` (`{ch, role}` grid with `regions` bound to declared actions and items) on the existing canvas, refusing `Span.color` from declared extensions
- [ ] 17.2 Move the decoders to `shared/`, let `Picture` reach sections, bands and overlays and the painter hold several images per frame keyed by slot, each capped by its slot's geometry and decoded off the drawing thread (R-A8); the per-extension budget of R-S12: 8 MiB per file before decoding, 32 MiB of decoded pixels per extension, one decode at a time per extension
- [ ] 17.3 Add `sprite`: a packaged sheet with frame size, columns and per-state frames, timing and loop, a fallback state, scaled and alpha-composited by the host, played from an agent's state or a source's field

## 18. Resident tier (change 7)

- [ ] 18.1 The resident entry points in `uze-script`: `start`, `event`, `input`, `render`, `stop`, with the scope kept between calls while the surface is open
- [ ] 18.2 Raw keys and pointer to a focused resident surface, the host's escape chord and pause key excepted; renders at most once per frame and 30 per second
- [ ] 18.3 Stop a resident script on a breached budget with the bound named, and keep the last frame when a render is late
- [ ] 18.4 Amend `.agents/rules/extensions.md` with the raw keys (0.9): "The resident tier is the one exception to 'an extension knows nothing of the keyboard': a focused resident surface receives raw keys and pointer, except the host's escape chord and the pause key, which never reach it and cannot be rebound."; `timer` as a host function

## 19. Authoring

- [ ] 19.1 Write the manifest and wire grammar into the generated schema with one complete worked example per slot and per feed (D21)
- [ ] 19.2 (dropped: running one declaration on canned inputs is a test, 23.5) Add `uze ext simulate <path> <declaration>` with canned event, selection, store and size, printing what the host would draw; make it the core of `uze agent extension check`; `--run` required where confinement is not enforced

## 20. Gallery

- [ ] 20.1 Build the ten reference extensions of D20 as fixture plugins, each shipped with the change of the split that proves it (4: the change 4 proof and `gsd-band`'s board; 5: `gpu-meter`; 6: `gsd-band`'s button, `agents-hud`, `flightdeck`, `chimes`, `jump`, `plan-review`; 7: `pets`, `screen`, `snake`)
- [ ] 20.2 One journey per reference: install → consent → open → the scenario of the platform spec it proves
- [ ] 20.3 Run `make check` and `openspec validate add-extension-platform --strict`

## 21. Views in KDL

- [x] 21.1 Dependency review of `kdl` (`default-features = false`, `span`), placed in `uze-core` or behind a `uze-contract` feature only `uze-core` enables (`extension-view-engine`)
- [x] 21.2 The bounded parse: 64 KiB cap, a KDL-aware depth pre-scan refusing above 32, a worker thread with an explicit stack and a time limit; tests with a 400-deep nest and the kdl-rs#177 input (`extension-view-engine`)
- [x] 21.3 The hand-written decoder over `KdlDocument` with spans into the manifest and catalog types, PascalCase catalog nodes and lowercase directives and manifest nodes, the wrong case refused with a suggestion; unknown nodes, properties and arguments, repeated properties, type annotations and v1 spellings refused with file, line, column and a suggestion (`extension-view-engine`)
- [x] 21.4 Compile a validated view into the generated tier keyed by digest; the client reads only compiled views (architecture rule: no KDL in `src/ui` or `uze-extensions`) (`extension-view-engine`)
- [x] 21.5 Sanitize decoded strings after escapes, including raw and multi-line strings (`extension-view-engine`)

## 22. Binding and UI state

- [x] 22.1 The reference grammar of D23 (`{field}`, `{a.b}`, `{.}`, `{^.field}`, scopes, `{{`), typed lone references, interpolation in display text only, identity properties literal (`extension-view-engine`)
- [x] 22.2 `each` (one list field, nesting ≤ 2, expansion counted against the node cap), `when`/`unless` with the fixed falsy set, `show`/`show-any` checked against declared literals (`extension-view-engine`)
- [x] 22.3 `template`/`use` with no parameters and an acyclic graph; `lookup` tables with defaults and locales (`extension-view-engine`)
- [ ] 22.4 Host-held `ui.<id>` for `tabs`, `toggle`, `select`, `field` and selection; local `show` and `filter`; `with` on actions; forms in the view delivering `form.<id>` — `extension-view-engine` holds `tabs`, `toggle` and list selection; `select`, `field`, `filter`, `with` and forms remain
- [ ] 22.5 (change 5) Argv substitution of declared flat fields and `UZE_IN_<NAME>` variables, `--` required before substituted values, refused at run, warned in the check and taught by the scaffold (S8)
- [ ] 22.6 (change 6) Two-step actions: `run`, then `agent.prompt` with a field of its answer, gesture only

## 23. Authoring loop, inspector and tests

- [x] 23.1 `uze agent plugin create --extension <id>`: scaffold manifest, view, fixture and a passing test inside a linked marketplace (creating one if none), install it into the current project, with enabling asked in UZE's chrome (`extension-view-engine`, its test from `extension-tests-and-lsp`; enabling is asked in the Extensions screen)
- [ ] 23.2 Reload linked extensions on save: poll their directories every 250 ms, debounce 150 ms, per-file behaviour of D25; widened grants wait for consent; the last good draw kept with the error over it, for every extension — `extension-view-engine` ships all of it but the consent for widened grants, which waits for grants (change 4, 4.6)
- [ ] 23.3 The inspector drawer for every extension: runs, events, state, grants, the compiled tree with why a node is absent, cost; rerun with edited input on linked extensions — `extension-view-engine` ships the compiled tree with why a node is absent, data, UI state, recorded actions and errors; runs, events, grants, cost and rerun remain
- [ ] 23.0 `fixture` on a source: fed by it when it has nothing else, switchable from a linked extension's inspector, never read at runtime from a non-linked marketplace; actions that would run nothing recorded with their arguments — `extension-view-engine` feeds sources from fixtures and records actions; switching from the inspector remains
- [ ] 23.4 On a linked extension's inspector: emit a synthetic event, change size and theme, inject fake agents, record a session's events into a fixture
- [x] 23.5 Tests in `uze agent plugin check` over `tests/*.test.kdl` on the test backend: recorded answers by default, `--run` under confinement, cell snapshots per slot, size and theme with `--update-snapshots` (`extension-tests-and-lsp`; `--run` runs scripts from change 4 and programs from change 5)
- [x] 23.6 `uze agent plugin lsp` generated from the catalog: completion, hover and the check's diagnostics for manifests, views and tests, shipped with the views (split 3) (`extension-tests-and-lsp`)
- [ ] 23.7 Tests for every gallery entry (D20)
- [ ] 23.8 (change 8, cosmetic) Vendor a KDL v2 grammar into `crates/uze-extensions/grammars/` with its licence, map `.kdl`, and test that it compiles under `fancy-regex` and colours `#true`, raw and multi-line strings, slashdash and type annotations in the editor, the preview and Markdown fences; write a small one if no upstream v2 grammar compiles — `extension-view-engine` ships the grammar in the editor, the preview and preview fences; a `kdl` fence inside a `.md` open in the editor is not coloured yet

## 24. Scripts first (D27, D19; change 4, step 4)

- [ ] 24.1 Dependency review of `rhai` 1.27 (`rhaiscript`, `cargo tree` with `std`+`sync` only, `cargo deny check`, its five `unsafe` sites named in the PR), then create `uze-script` depending on `uze-contract` and `uze-platform` only, behind an `Extension` adapter; a layering rule keeps `rhai::` out of every other crate
- [ ] 24.2 The engine per extension: `Engine::new_raw` with chosen packages, `DummyModuleResolver` and `set_max_modules(0)`, `disable_symbol("eval")`, `set_strict_variables(true)`, `print`/`debug` to the inspector, `set_max_operations`/`call_levels`/`expr_depths`/`string_size`/`array_size`/`map_size` and `on_progress` as the deadline; `unchecked`, `internals` and `debugging` never enabled
- [ ] 24.3 Host functions as grants, in change 4 only `read_checkout` and `list_checkout` beneath the root of 4.10 with no link followed and `..` refused, and `clock`; `run_program` (5.10), `store` (5.9), `agent_prompt` with D16's confirmation (change 6), `notify` (change 6), `timer` (change 7) and `fetch` (change 8) do not exist yet and the check refuses a script naming one its grants do not register; the native primitives of D28
- [ ] 24.4 `script "…"` declarations answered by `answer(question)` on the job pool, compiled at load and on a linked extension's save from the digested bytes (4.8), digest-checked against consent
- [ ] 24.5 The argv runner and `exec:` grants only in change 5, with OS confinement or its explicit "runs as you" disclosure (5.5b)
- [ ] 24.6 Lead the check, the scaffold and the Skill with scripts, documenting programs as fully supported
- [ ] 24.7 (change 8) `fetch` as a host function over `uze_platform::tools::curl()`, the system's `curl` by path with `-q` and a scrubbed environment, run through the program runner under the run's bounds and the `net` grant; no TLS crate

## 25. Performance budget (D28)

- [ ] 25.1 Share answers: one run per (identity, source, checkout, input digest) feeding every surface and band that asked it, through the runner's session cache (change 4)
- [ ] 25.2 A global pool of two threads for extension jobs with a serial lane per extension (change 4)
- [ ] 25.3 Compile scripts and views on first use, cached by digest (change 4)
- [ ] 25.4 Register native primitives for scripts: lines, glob, regex, JSON, front matter, KDL (change 4)
- [ ] 25.5 Warn in the check when a program-backed source has `every` under 30 s (change 5, with 5.8)
- [ ] 25.6 A performance test holding D28's targets (embedded answer p95 < 20 ms, binding < 1 ms per surface, < 1 MiB per extension, the 15 × 5 storm < 50 ms with sharing), and per-extension runs per minute, p95 and memory in the inspector and `uze doctor`
