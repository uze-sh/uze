<!-- Decisions taken while implementing, one entry each, judged at archive. -->

## D1 — An action's scalars are properties, what it does is its one child

- status:   pending
- task:     1.2
- commit:   cf2bf3a2
- trigger:  D22 fixes the nodes and the kebab-case of properties but not where an action's `label`, `on`, `key` and `confirm` go, and the specs write `confirm "…"` and `key "d"` loosely.
- decided:  `action "<id>" label="…" on="<surface>" key="…" confirm="…" { <does> }`, the block holding exactly one of `script "x.rhai"`, `refresh "<source>" …`, `clipboard "<template>"`, `open-path "<template>"` or `open "<surface>"`. A source's parts are child nodes: one of `script`/`run`/`fixture`, `input name="<template>" …` (properties, text only), `every "<n><ms|s|m|h>"`, and one `refresh-on "<event>" <paths…>` per event. `wants "<grant>" …` is written once, as arguments.
- rejected: Child nodes for an action's scalars (`label "…"`): one line per action reads as the operator's examples do, and a surface is already written that way.
- blast:    The LSP's manifest completions and the scaffold follow this spelling.

## D2 — A suggested key is spelled `ctrl-r`, and any unmodified character is bare

- status:   pending
- task:     1.3
- commit:   cf2bf3a2
- trigger:  The design and the specs spell a manifest's key `ctrl-r`/`ctrl-q`, while `keys.json` and `uze_keys::Chord::parse` join with `+`; and the check must refuse a "bare-letter" key on a `confirm` action.
- decided:  `uze_contract::KeySuggestion` reads lowercase modifiers joined to the key by `-` (`ctrl-shift-r`, `f5`, `ctrl--`), the key one printable ASCII character or a named key. A confirmed action refuses any single character without a modifier (a digit or `/` as much as a letter). Step 3 converts a suggestion into a `Chord`; uze-contract may not depend on uze-keys.
- rejected: The `+` spelling of `keys.json`: the manifest examples already use `-`. Refusing only letters: a stray `1` is as easy to press as a stray `d`.
- blast:    A manifest written with `ctrl+r` is refused with the `-` spelling as the suggestion.

## D3 — A copy or an open-path action needs its grant in `wants`

- status:   pending
- task:     1.3
- commit:   cf2bf3a2
- trigger:  The task lists the grants accepted but not whether a declared `clipboard` or `open-path` action without the grant is refused, and a grant nothing uses would mean consent never covers what runs.
- decided:  The check refuses an action whose `does` is `clipboard` or `open-path` when `wants` lacks that grant, naming it (`Does::needs`). Whether a script calls a host function its grants do not register stays with step 4.
- rejected: Letting the runner refuse it at the gesture: the author learns it from the check instead of from a dead button.
- blast:    None today: no manifest declared actions before this change.

## D4 — Later nodes are named with their change, and `events`/`deliver` wait

- status:   pending
- task:     1.2
- commit:   cf2bf3a2
- trigger:  The *Proposed split* lists `deliver` and `events` in step 4.1, while this change's task 1.2 and its Non-Goals leave `deliver: each` and the `events` source to change 6. The grant is spelled `open_path` in D5 and `open-path` in this change.
- decided:  `preferences` (6), `deliver` (6), `output` (5) and `agent.*` events (6) are refused as "arrives in a later change (change N)"; `events` is an unknown node. `run` is decoded into `SourceKind::Run` and refused by the check (5). Later grants: `exec:*`/`store` 5, `agent:*`/`notify` 6, `resident` 7, `net`/`secret`/`edit:checkout` 8; `write:checkout` does not exist. The grant is `open-path`, kebab-case as D22 asks of every manifest word.
- rejected: Decoding `deliver` now with only `latest`: a property that can take one value is a promise this change does not keep.
- blast:    Change 6 replaces the refusal of `deliver` and adds `events`.

## D5 — The built-ins' switch stays in `config.toml`, beside the record

- status:   pending
- task:     2.1
- commit:   2d8a4a00
- trigger:  M2 says `state/extensions.json` absorbs the existing switch and reads the old `disabled` list as shape 0, but that switch is the `[extensions]` section of `config.toml`: authored TOML at the root, not a record under `state/`.
- decided:  They coexist. `[extensions]` stays the machine-wide on/off of built-ins and of every declared extension, keyed as today; `state/extensions.json` is a new record at shape 1 with no shape-0 rung, keyed `plugin@marketplace/id`. A declared extension that runs code needs both the switch on and the record's consent and enabling for the project.
- rejected: Migrating the section into the record: `persistence-tiers` gives authored files no shape and forbids writing over the operator's text, and a built-in's switch is a preference, not a consent.
- blast:    Step 7's Extensions screen shows the switch and the consent state side by side.

## D6 — The load is bounded at 256 files, 8 MiB and four levels, and lives in `uze-core`

- status:   pending
- task:     2.2
- commit:   44e4363d
- trigger:  M3 asks for a bounded read into memory without fixing the bounds, and puts the read in `uze-application` while `uze-core` is the only reader of an extension's KDL.
- decided:  `uze_core::capability::extension::Loaded::read` reads the whole directory once (`LOAD_FILES` 256, `LOAD_BYTES` 8 MiB, `LOAD_DEPTH` 4: the bounds the compile cache already walked with) and digests it with `digest::files_sha256` over `sha2`, already a dependency. `compile_loaded` compiles from those bytes alone; `Extensions::load` in `uze-application` calls both and reads fixtures from the same load. A link is neither followed nor loaded, and a directory past the bounds is refused whole instead of compiled uncached. The authoring check and the language server still read disk with an editor's overlay.
- rejected: Reading in `uze-application` and handing text to `uze-core`: two modules would decide what an extension's files are.
- blast:    An extension that relied on a link inside its directory, or on more than 8 MiB, no longer loads in the workspace; `uze agent plugin check` still compiles it.

## D7 — A narrowing records the narrower set, applied when it is judged

- status:   pending
- task:     2.3
- commit:   2d8a4a00
- trigger:  The update rule says a narrower or equal set applies and records the new digest, but not whether `accepted_grants` shrinks with it, nor who writes it.
- decided:  An applied update records the grants as they are now, so a grant an update dropped is a widening when it returns. It is applied when `Extensions::state` or `standings` judges it, under the record's lock and judged again there.
- rejected: Keeping the wider accepted set: an author could drop a grant, ship, and bring it back with no question asked.
- blast:    Reading a state can write the record; a read-only caller sees the same answer.

## D8 — Only a plugin leaving the machine forgets its consents

- status:   pending
- task:     2.3
- commit:   2d8a4a00
- trigger:  Updating a plugin goes through the same teardown as removing it (`detach_and_remove`).
- decided:  `Plugins::remove` and removing the plugin's marketplace forget its extensions' records; an update keeps them and is judged by the update rule. Taking the plugin out of `agents.yaml` keeps them.
- rejected: Forgetting in the shared teardown: every update would ask for consent again.
- blast:    None beyond the removal paths.

## D9 — The asker is passed in, by the check `uze workspace allow` makes

- status:   pending
- task:     2.5
- commit:   2d8a4a00
- trigger:  The task asks for `consent(identity)` and `enable(identity, project)` refusing inside an agent's pane, but `uze-application` may not spell `UZE_AGENT` (the layering suite), and `uze workspace allow` read the stamps inline.
- decided:  `consent` and `enable` take an `Asker` (`Operator` or `AgentPane`). `src/cli/workspace.rs::asker()` is the one reading of the stamps, used by `uze workspace allow` and by the callers step 7 adds. `disable` takes none: taking a reach away is never refused.
- rejected: Reading the environment in `uze-application`, which breaks the variable's single owner.
- blast:    Every caller of consent and enabling names who is asking.

## D10 — Fixtures only means no script, no program and no grant

- status:   pending
- task:     2.4
- commit:   2d8a4a00
- trigger:  The task defines `FixturesOnly` as declaring neither a script nor grants; an action can also run a script, and the workspace's drawing does not yet consult the new state.
- decided:  An extension needs consent when it wants any grant, has a source that is not a fixture, or an action whose `does` is a script. `ExtensionState::runs_scripts` is the question step 4 asks before a script answers; until then nothing runs and the switch decides drawing as before.
- rejected: Hiding a not-enabled extension's surfaces now: its fixtures still draw, and the spec's BREAKING note lands with the scripts that make it matter.
- blast:    Step 4 wires `runs_scripts` into the runner.

## D11 — What a checkout is refused for, and how each is seen

- status:   pending
- task:     2.6
- commit:   3f412c5f
- trigger:  D6 of the parent lists what is refused but not how a volume root, a temporary directory or a bare repository is recognised, and `uze-git`'s reads never discover a bare repository.
- decided:  A volume root is a path with no parent or a network (9p) mount point from `uze_platform::mounts`; a temporary directory is anything at or below `std::env::temp_dir()`; the home directory and its ancestors, and `$UZE_HOME` with anything above or below it, are compared canonically with `uze_platform::path::is_within`. A bare repository is named from its files (`HEAD`, `objects`, `refs` at its top) because Git refuses it; a worktree root without a `.git` inside it is refused too. Enabling does not check the checkout: the runner does, on every answer, from step 4.
- rejected: Refusing only when Git says bare: through `uze-git` a bare repository reads as no repository at all, and the reason would be wrong.
- blast:    A project kept under the system temporary directory has no extension source run in it.
## D12 — `rhai` reviewed, and one licence the earlier review missed (task 4.1)

**Who publishes it.** `cargo info rhai@1.27.0`: repository
`github.com/rhaiscript/rhai`, MIT OR Apache-2.0, MSRV 1.66.

**What it brings** with `default-features = false, features = ["std",
"sync"]` (`cargo tree -p uze-script -e normal`): `ahash` 0.8.12 (with
`const-random` → `const-random-macro`, `getrandom` 0.2, `tiny-keccak`,
`crunchy`; `zerocopy`), `bitflags`, `num-traits`, `once_cell`
(`portable-atomic`), `rhai_codegen` 3.3.0 (a proc macro over `syn` 2, in the
tree already), `smallvec`, `smartstring` 1.0.1 (`static_assertions`),
`thin-vec`. New in `Cargo.lock`: `ahash`, `const-random`,
`const-random-macro`, `crunchy`, `getrandom` 0.2, `no-std-compat`, `rhai`,
`rhai_codegen`, `smartstring`, `spin`, `thin-vec`, `tiny-keccak`. No crate
compiles C.

**`unsafe`.** The crate does not forbid it (only its `grain` module does).
Four sites in 1.27.0: `src/reify.rs:19` and `:56` (`transmute_copy` after a
`TypeId` check, the `Dynamic` downcast), `src/func/register.rs:60` (the same,
for a `&str` argument), `src/func/func_call.rs:198` (swapping the first
argument of a method call). `uze-script` itself is `#![forbid(unsafe_code)]`.

**Licences.** `rhai`, `rhai_codegen`, `ahash`, `const-random*`, `getrandom`,
`thin-vec`: MIT OR Apache-2.0; `crunchy`, `spin`, `no-std-compat`: MIT;
`tiny-keccak`: CC0-1.0. **`smartstring` 1.0.1 is `MPL-2.0+`**, a mandatory
dependency of `rhai` (no feature removes it), and MPL-2.0 is in neither
`deny.toml`'s `allow` nor `about.toml`'s `accepted`. `cargo deny check` (the
workspace graph) and `make attributions` will refuse it. The earlier review
said every licence was on the list; it was not. **Decided by the operator
(2026-10-26):** one per-crate MPL-2.0 exception for `smartstring` in both
files (file-level copyleft; UZE never modifies it). `smartstring` is also
archived (RUSTSEC-2026-0249, ignored with its exit in `deny.toml`), so
`uze-script` enables rhai's `compact_str`, which replaces it inside the
engine; rhai 1.27 still compiles it, and the exception leaves when rhai
makes it optional. rhai 1.27.0's `pad` loops forever on an empty padding
(rhai#1195, unreleased): `uze-script` shadows it with a bounded `pad` until
the fixed release is in the lock.

## D13 — `uze-script`'s shape where the plan did not settle it (tasks 4.1–4.4)

- **`uze-contract` is not a dependency yet.** Nothing in it is used today;
  the minimal `Grant` (`read:checkout` only, the grant that registers
  script functions) lives in `uze-script::Grant` until task 1.1 lands the
  contract's own, which then replaces it. The layering test admits
  `uze-contract` and `uze-platform` and nothing else. `uze-platform` is used
  for `Reach::clock`'s default.
- **`answer` takes `Arc<dyn Reach>`, not `&dyn Reach`.** Rhai keeps every
  registered function as `'static + Send + Sync`, and a borrowed `Reach`
  cannot be handed to one without `unsafe`.
- **`answer` returns `Answered { outcome, trace }`** rather than
  `Result<Envelope, ScriptError>`: the trace of a failed answer is what the
  inspector needs most.
- **An engine per answer.** About 0.2 ms (D28); in exchange each answer has
  its own trace, deadline and exactly its grants' functions, and nothing is
  shared between extensions on the pool. The packages are built once.
- **The check reads tokens, not the syntax tree.** Walking Rhai's AST needs
  `internals`, which D19 never enables; `host_functions_of` scans
  identifiers outside comments and strings whose whole text is a host
  function (`Fn("read_checkout")`), erring towards naming too much. The
  engine registering only what was granted stays the real bound.
- **`import` and `export` are disabled symbols** as well as `eval`, beside
  the `DummyModuleResolver` and `set_max_modules(0)`, so a script naming one
  fails to compile rather than at its first run.
- **`LanguageCorePackage` is left out**: its `sleep` would hold a pool lane
  past any operation count. Time is the host's `clock` (milliseconds since
  the epoch).
- **Limits** (`engine.rs`, each with its reason): 1,000,000 operations, a
  100 ms deadline (five times D28's 20 ms p95 target, checked every 256
  operations), 32 call levels, expression depths 64/32, strings 1 MiB,
  arrays and maps 10,000, scripts 256 KiB, a trace of 200 lines of 1 KiB.
  Measured here: a 50-row answer at most 0.2 ms in release (1.8 ms in a
  debug build), a compile under 1 ms in release. Rhai rechecks a
  container's size on every push, so a script pushing 10,000 items one by
  one reaches the deadline in a debug build before the array bound.
- **Primitives**: `lines`, `glob`/`glob_match` (`*`, `**`, `**/`, `?`, over a
  list, linear in pattern × path), `regex`/`regex_match` (the `regex` crate,
  rust-lang, already in the lock; compiled size capped at 1 MiB, 32
  patterns cached per answer), `json_parse`/`json_stringify` (depth 128),
  `front_matter` (a `---` block's top-level `key: value` lines, the rest in
  `raw`; no YAML parser, since the only one in the tree is `uze-core`'s
  exception). **`kdl` is deferred**: the `kdl` parser recurses without a
  limit and is parsed only behind `uze-core`'s size cap, nesting pre-scan and
  timed worker (D22); exposing it here would duplicate those guards in a
  second crate. It returns when a gallery entry needs it, through a guarded
  parse the contract can share.
- **`Clean` is the host's.** The envelope's strings leave `uze-script` as the
  script wrote them; they pass `uze_contract::Clean` where the host takes
  them (task 5.2).

## D14 — The contract holds the question, the answer and the envelope, and the question names its checkout

- status:   pending
- task:     4.5
- commit:   a12efaae, 88bf703d
- trigger:  M4 and the parent's D3 write `Host::answer(Question { identity, declaration, input })`, but a script reads beneath a checkout the runner must resolve, `uze-extensions` may name no UZE crate but `uze-contract`, and the runner in `uze-application` may not name `uze-extensions`. D13 left a provisional `Grant` and `Envelope` in `uze-script`.
- decided:  `uze_contract::{Question, Answer, Envelope}`. `Question` carries `checkout: Option<PathBuf>`, the root of the surface asking. `Answer` is `Answered { envelope, trace }`, `Failed { reason, trace }` (it ran and did not answer, or was stopped) or `Withheld { reason }` (nothing ran: consent, enabling, the checkout). `uze-script` uses `uze_contract::Grant` and returns `uze_contract::Envelope`; `uze-extensions` re-exports the three. `Host::answer` defaults to `Withheld`, so every host that runs no declared extension says so.
- rejected: The checkout inside `input`: the runner would read a path out of the extension's own data. Two envelope types bridged in the client: one shape, written twice.
- blast:    `uze-script` depends on `uze-contract`, as the layering test already allowed.

## D15 — The runner reads the extension on every question and remembers only what a script answered

- status:   pending
- task:     4.5
- commit:   6cbeddc8
- trigger:  The task asks the runner to find the declaration in the consented compile by the accepted digest, and to keep a session cache by (identity, source, checkout, input digest), without saying when consent is checked against the cache, what the project of a checkout is, or what a script is handed as `context`.
- decided:  Each question loads the extension once (`Loaded::read`, the compile cache by digest) and takes the script's bytes from that load, so what runs is what the digest covers. The state is judged for `project_root_of(checkout)` before the cache is looked at, so disabling stops a remembered answer at once. The cache key adds the files' digest: a save is a new question. Only an `Answered` script is remembered; a failure or a withheld answer is asked again. A fixture is answered from the same load, with no consent and no cache. `context` is `{ checkout, extension, source }`; `input` is the declared one as written, its `{field}` references resolved by the engine in 5.1. The reach resolves every path through its links and refuses one that lands outside the root, reads at most 1 MiB of UTF-8 and lists at most 10,000 entries, a link listed as `link`. Scripts compile by the SHA-256 of their bytes in one `ScriptCache` per runner.
- rejected: Keeping the load in memory by stamp: a second reader of the directory's state for a cost D28 does not measure as a problem. Remembering failures: a script fixed in the checkout would stay broken for the session.
- blast:    A path swapped for a link between the resolution and the read is followed; the window is one `open` wide and the checkout's own agent is the only writer that could aim it.

## D16 — The pool takes the serial lanes; a built-in's parallel reads keep their threads

- status:   pending
- task:     4.6
- commit:   6fd88f9c
- trigger:  D28 asks for one pool of two threads with a serial lane per extension, while the contract's `Lanes::Parallel` says `code` and `spec` run their Git reads together, and `schedule_engine_jobs` gave each a thread.
- decided:  `orchestrator/lanes.rs` is a `LanePool` of `POOL_THREADS` (2) threads for the whole client, one lane per `Owner`; a lane runs one task at a time in the order asked, and a busy lane is skipped, never waited on. Every extension whose `lanes()` is `Serial` (every declared one, and `architect`) runs there; a `Parallel` built-in keeps a thread per job. The task built in `reads.rs` carries an `EngineHost { home, runner }` onto the pool's thread; `WorkspaceHost` stays named only inside `thread::spawn` there.
- rejected: Everything on the pool: a diff waiting behind a file read inside `code`'s own lane would undo what `Parallel` promises. A pool per extension: fifty enabled extensions would be a hundred threads.
- blast:    `architect`'s reads now share two threads with every declared extension.

## D17 — What the declared engine asks, and when

- status:   pending
- task:     4.5, 5.1 (its start)
- commit:   88bf703d
- trigger:  The engine had to ask `Host::answer` for each source a surface shows, but nothing said which checkout a source shown by a view and a section at once is about, or when a question is asked again.
- decided:  Shown are the open views' sources on the checkout each was opened on, then the sections' and drawers' on the checkout in front; a source shown on two checkouts is asked about the first, so the two never take turns. A source is asked again when its checkout or input changes, when a view showing it opens (consent may have changed), and after a newer compile. With no checkout known nothing is asked. Every question is numbered and an answer whose number is not the source's latest is dropped. `Failed` replaces the data with its reason, in the danger role; `Withheld` is drawn in the warning role, said in the inspector as `not asked: …`. Fixtures are still handed in with the load, so they draw without waiting, and are asked through the runner like any source.
- rejected: The engine branching on a source's kind: what answers is the runner's to know (parent D3). Asking only visible drawers: the engine is not told a drawer opened; a closed drawer's question costs one remembered answer.
- blast:    A linked extension's fixture saved broken is drawn as its error instead of the last good draw, once the runner answers it.

## D18 — Who asks a source is checked by name

- status:   pending
- task:     4.5
- commit:   6fd88f9c
- trigger:  The task asks for an architecture test that only `declared/` calls `answer`, but `answer` is a common method name (`ScriptCache`, a language server) and the client must hand the question on.
- decided:  `only_the_declared_engine_asks_a_source`: in `uze-extensions`, a `.answer(` outside `declared.rs` and `declared/` fails; in `src/`, `Host::answer` and `host.answer(` fail everywhere, and naming `uze_extensions::Question` or `uze_contract::Question` fails outside `src/ui/extension_host.rs`, which passes it to the runner. The orchestrator test that draws a section from `state.rhai` keeps its checkout under the build's `target/`, since the runner refuses one in the temporary directory (D11).
- rejected: Matching `Question {`: the client has local types of that name.
- blast:    A second engine that answers sources is a change to the test, said in its reason.

## D19 — The tables are registered beside the keymap in force, per extension

- status:   pending
- task:     3.2
- commit:   eb7d8135
- trigger:  D8 of the parent says a table is registered when an extension is enabled and dropped when it is disabled, but the keymap in force is one process-wide value that the Keys screen and `keys.json` replace whole.
- decided:  `uze_keys::active` keeps the operator's keymap and the registered `ExtensionTable`s apart, by owner (`plugin@marketplace/id`), and answers both resolved together (`Keymap::with_extensions`); `register_extension`/`unregister_extension` change one owner. The workspace registers each hosted declared extension whose switch is on and that declares an action, after a discovery or compile lands and after a switch, and takes back what it registered when the extension is switched off or leaves. "Enabled" is the extension switch for now: the consent record's `enabled_in` reaches drawing with step 7, as D10 says.
- rejected: Tables inside the keymap that `set_active` replaces: a rebind on the Keys screen, or a test resetting the default, would forget every extension. One "replace all" registration: two clients or tests registering different sets would take each other's keys.
- blast:    Step 7's Extensions screen reads `Keymap::extension_problems` for the same collisions `uze doctor` prints.

## D20 — An action is live on its own surface, after the host's keys and before the shared vocabulary

- status:   pending
- task:     3.2
- commit:   feda4c94
- trigger:  The spec says "while that extension's surface is in front", and an extension may declare several surfaces; D8 says "after `Global` and before `Scope::Declared`" without placing `Surfaces`, and does not say under which scope `keys.json` writes an `ext.*` name.
- decided:  `Keymap::resolve_for(chord, scopes, Front { owner, surface })` asks the table only when `Scope::Declared` is reachable, and only for actions whose `on` is the surface in front; any host binding in a reachable scope other than `Declared` (the global keys, the doors in `Surfaces`) answers first, then the table, then the declared surfaces' generic vocabulary. A suggestion is bound when `resolve` falls through on `[Global, Workspace, Declared, Surfaces]`; a conflict is judged per surface, the first declared keeping the chord. `keys.json` binds `ext.*` names under `"declared"` (another scope is a warning) and the first chord written is the one bound; an operator chord a global key or a door holds is dropped with a warning, since it could never fire. Every table problem is a `Severity::Warning`.
- rejected: One table per extension regardless of surface: two surfaces of one extension never share a screen, so a chord on each is no conflict. Letting the operator's chord shadow `Global`: leaving and help must stay reachable everywhere.
- blast:    Ext names saved by the Keys screen go back under `declared` (`difference_from_default`).

## D21 — A key reaches the engine through the host's `Hosted::press`, not the contract

- status:   pending
- task:     3.3
- commit:   b31dd5ab
- trigger:  The task routes a key to a named action and leaves what it does to step 5, while `contract.rs` is being changed in parallel by tasks 4.5–4.6.
- decided:  `Hosted::press(surface, action)` (default: nothing) calls `DeclaredExtension::press`, which records the action on no item (`item: null`) exactly as a button does, and only when the manifest offers it on that surface. The `.test.kdl` runner registers the extension's table and resolves `press` through it, so `expect-action` sees a key-fired action; its footer carries the same host keys as the workspace's. Item menus are not drawn: the declared engine has none yet (task 5.2 brings them, with `chord_for` beside each entry), and a drawer's keys go through the drawer's own path with no table.
- rejected: A `Command::Action(String)` in `view.rs` or a new contract method now: either changes the contract in two branches at once, for a behaviour step 5 rewrites.
- blast:    Step 5 replaces the record with the run, and may move `press` into the contract with the runner.

## D22 — The footer names only the actions that hold a key; `open` with a global key waits

- status:   pending
- task:     3.3
- commit:   b31dd5ab
- trigger:  The task says the footer, the index and the menus show an action "with `chord_for`'s answer or by pointer", and the design system keeps a footer to one line of keys.
- decided:  The footer lists the surface's declared actions that hold a key, before the inspector, and a click on one is pressing it; the index lists every one, with its key or without, and choosing one runs it. `open` actions bound over the pane and every surface (R-U7, D8's second part) are left out of this step: the table holds surface actions only, and an `open` action is offered on its `on` surface like any other.
- rejected: A footer hint without a key: the hint line is keys, and the index is the pointer's way to the rest.
- blast:    `open` with a global key lands with step 7's tab strip and `jump` in change 6.

## D23 — `uze doctor` reports key collisions from the binary, as `keys` concerns

- status:   pending
- task:     3.2
- commit:   e0fc2857
- trigger:  The collisions live in the keymap in force, which is the binary's (`src/keymap.rs`), while `DoctorReport` is `uze-application`'s and is being changed in parallel.
- decided:  `uze::keymap::extension_collisions` compiles every installed extension (switched on or not: a collision holds whenever it is enabled), resolves the tables over the keymap in force, and `uze doctor` adds each problem to the report's `machine_concerns` with the subject `keys`, so the text and the JSON both carry them as warnings that never fail the command.
- rejected: A field on `DoctorReport`: it would touch `uze-application` for a fact only the binary can compute.
- blast:    None beyond the doctor's warnings.

## D24 — A question says what it asks: a source, a refresh or an action

- status:   pending
- task:     5.1, 5.2
- commit:   404d6350
- trigger:  An action's `script` runs through the same runner as a source's, and a `refresh` must run a script the session cache would otherwise answer from memory; `Question.declaration` named a source only.
- decided:  `uze_contract::Asking { Source, Refresh, Action }` on `Question` (serde default `Source`). `Source` may be served from the session cache; `Refresh` runs and replaces what was remembered; `Action` finds the `ActionDecl`, runs its `Does::Script` under the same checkout and consent gates and is never remembered (an action that does not run a script is a failure). The script's `context` names `action` instead of `source`.
- rejected: A second `Host` method for actions: one door, one test that only `declared/` asks it (D18). A `fresh: bool` beside the id: it cannot say an action is never remembered.
- blast:    Every `Question` names what it asks.

## D25 — What an action receives, and where it runs

- status:   pending
- task:     5.1, 5.2
- commit:   d7945687
- trigger:  The task says the item is the action's input and that `{field}` references resolve "with the item selected when there is one", without the input's shape, how `{item.name}` (the spec's spelling) and `{name}` relate, or whether a `Button` runs an action offered on another surface.
- decided:  An action's script is asked with `{ "item": <item> }`. Every template (a source's `input`, `confirm`, `clipboard`, `open-path`) reads an item scope holding the item's own fields plus `item` naming it, so `{name}` and `{item.name}` agree, beside `ui.*` and `context.{checkout,slot,locale}`; a lone reference keeps its JSON type, one to nothing is `null`. A button's item is its `item` property; a key acts on the row selected in the surface's first `List` (`Act::List` now carries each row's item), `null` when none. A `Button` runs the declared action of its id wherever it is drawn; a key only where `on` offers it (D20). Every gesture is still recorded for the inspector and `expect-action`, now with `declared`; an undeclared one runs nothing. Item menus (D21) are not drawn yet.
- rejected: The item as the whole input: a script would branch on whether a row was selected to read anything.
- blast:    D21's `item: null` for keys becomes the selection; item menus move to step 7.3 with the tab strip.

## D26 — What an envelope does beside its data

- status:   pending
- task:     5.1, 5.2
- commit:   d7945687
- trigger:  The envelope's `refresh`, `toast` and `notice` must be applied, but `Outcome` had no toast, `absorb` answers one outcome, an action's envelope has `data` with no source to bind, and two sources refreshing each other would ask forever.
- decided:  `notice` is held per surface by the engine and drawn as the view's `notice` and the section's caption, in the info role, cleared by the next answer for that surface. `toast` becomes `Outcome::Toast(text)`; the host raises it with the extension's identity (`Display`) as the detail, from a gesture, a drawer or an answer arriving later. `refresh` marks each named source stale, asked with `Asking::Refresh` once a surface shows it; an answer never refreshes its own source, and the answer to a refresh another answer asked refreshes nothing (one hop). An action's `data` is not drawn: what it changed is refreshed. Every string passes `Clean` where the engine takes it.
- rejected: An action's `data` replacing the source its surface shows: the runner's cache would serve the old answer on the next question.
- blast:    `Outcome::Toast` is a new variant every host match follows.

## D27 — A confirmation is asked in a view, and a long copy is one

- status:   pending
- task:     5.2
- commit:   d7945687
- trigger:  The contract draws a question only on a `View` (`View.confirm`); a `Section` and a drawer have no room for one, and the spec asks that a copy over one line or 256 bytes asks first.
- decided:  The engine resolves the action first, then holds it until answered: title the resolved `confirm` (or `Copy N lines?`/`Copy N bytes?`), subject the identity's `Display`, body the resolved arguments (the script and its item, the sources, the copied text, the path, the surface), the confirm word the action's label, the keyboard starting on the way out. `Activate` answers with the focused button, `Close` answers no, the focus keys swap, `ViewHit::Answer` answers from a click; nothing else runs while asked. Pressed in a section or a drawer, the question is set on the view the action is `on`, or the extension's first view, which is shown (`Outcome::Show`); an extension with no view refuses it in a toast. Closing the view drops the question. The copied text passes `Clean::text`.
- rejected: A host-level dialog for sections: a second question path beside `View.confirm` for a case no fixture has.
- blast:    A confirmed action pressed in the sidebar opens its view.

## D28 — `open-path` is judged by its words, against the surface's checkout

- status:   pending
- task:     5.2
- commit:   d7945687
- trigger:  `open-path` runs "only inside the checkout", and `uze-extensions` reads no filesystem.
- decided:  The resolved text must be a relative path with no `..`, root or prefix component and not empty; it is joined to the checkout the surface is about (the view's, else the one in front) and returned as `Outcome::OpenPath { project, target }`. Refusals and a missing checkout are `Outcome::Failed` toasts naming why. A link inside the checkout is the checkout's own and opens as the code surface opens any of its files.
- rejected: Canonicalising through the host: the code surface opens the path through its own reads, which a lexical bound already keeps beneath the root.
- blast:    `a/../b` is refused though it stays inside.

## D29 — A plugin's section carries the strip over its header

- status:   pending
- task:     5.3
- commit:   d7945687
- trigger:  The view and the drawer already drew `render_attribution` from the identity's three fields; a section drew only its own title, which the extension chooses.
- decided:  `tree::attribution(&Identity)` is the identity's `Display`. A declared section takes one more row (`ATTRIBUTION_ROWS`), the strip drawn over its header and outside the area the section is drawn in, folded or open; a column with no room for both draws neither. The `.test.kdl` runner draws a section the same way.
- rejected: The strip in the header's caption: the caption is the section's, and an answer's notice now lands there.
- blast:    Every plugin section is a row taller.

## D30 — The ungranted-call check lives in `uze-application`

- status:   pending
- task:     4.3
- commit:   66b6d42c
- trigger:  Task 4.3 left `uze_script::ungranted_calls` unwired: the check reads the `.rhai` files, and `uze-core` compiles the manifest without `uze-script`.
- decided:  `Extensions::check` (the CLI's `uze agent plugin check` and the language server's path) scans every script a source or an action runs, from the editor's overlay or disk, and adds an error at the first call: "`read_checkout` needs the `read:checkout` grant, which the manifest does not want", suggesting `wants "read:checkout"`. `uze-core` stays without `uze-script`: the layering test admits the engine only where it is named today, and `uze-application` already depends on it.
- rejected: A `uze-core` dependency on `uze-script`: the package manager would link a script engine to compile a manifest.
- blast:    None beyond the new diagnostic.

## D31 — An extension not enabled says so on the surface and in a toast

- status:   pending
- task:     5.1, 5.2
- commit:   d7945687
- trigger:  The task asks to confirm what a surface says when its extension runs scripts and is not enabled in the project.
- decided:  Confirmed: the runner withholds with "needs consent", "needs enabling in this project" or "held: …", drawn in place of the data in the warning role and said in the inspector as `not asked: …`. An action withheld for the same reason is a toast titled "<label> did not run" with the reason as detail. The way to the Extensions screen is step 7's.
- rejected: None.
- blast:    None.

## D32 — The clock is the host's pass, counted by the engine

- status:   pending
- task:     6.1, 6.2
- commit:   da62ab2b, 0054cfc6
- trigger:  The task asks where `every`'s clock lives, without a thread per source, and the engine reads no clock of its own but is the only one that knows which sources a surface shows.
- decided:  The contract gains `Extension::pass(&Pass { now, visible, happened })`, told by `schedule_engine_jobs_at` on every tick of the orchestrator before `jobs()`, and `Extension::watches()`. The engine stamps each question with the last `now` and, for each source shown on a surface in `visible` whose `every` has passed since, asks again with `Asking::Refresh` (past the runner's cache, which would answer the same input from memory). In view is the view standing in the pane, every unfolded declared section while a checkout is in front (the sidebar is always drawn, with no gesture that hides it), and the open drawer. Tests drive `now` by hand.
- rejected: A timer thread per source, or the host reading each manifest's `every`: the engine already holds the declarations and what it asked when.
- blast:    D17's first question still goes to a folded section's or a closed drawer's source, once: only asking again waits for view.

## D33 — What is watched, bounded, and how events reach a source

- status:   pending
- task:     6.2
- commit:   da62ab2b, 019cdeab, 0054cfc6
- trigger:  `paths.changed` needs a digest of declared paths that the parent bounds without numbers, `checkout.changed` must come from the existing watch, and an event nobody declared must cost nothing.
- decided:  Each enabled engine's `watches()` names, for each source shown that declares `refresh-on`, the checkout it is shown on and its events; `spawn_checkout_watch` reads, in its one thread, `checkout_stamp(root, None)` for `checkout.changed` and `uze_application::paths_digest` for each path: at most 2,000 entries (`WATCH_ENTRIES`) and 8 levels (`WATCH_DEPTH`) beneath it, breadth first in name order, links read and never followed, Git not asked; a path not relative and inside digests as itself. A first read only sets the value compared with. What moved is told to each engine watching it on the next pass; the engine gathers the sources it is a reason for in a 250 ms window (`COALESCE`) from the first event, and asks each once when it closes, with `Asking::Refresh`; one closing while nothing showing the source is in view is remembered and asked when one is.
- rejected: A filesystem watcher (a dependency and a thread per checkout). Gathering in the host: the window would have to know which source each event reaches.
- blast:    A change past the 2,000th entry or the eighth level is not seen; the window adds up to 250 ms to the one-second watch.

## D40 — One report shape for every surface, and the record dates a same-grant update

- status:   pending
- task:     7.1, 7.4
- commit:   96a68eb2
- trigger:  The Extensions screen, `uze status`, `uze inspect` and the workspace's offer each need an extension's state, grants, the projects it is enabled in, a held update's new grants and why a checkout runs nothing, and the spec's "updated <date>, same grants" needs a date the record did not keep apart from consent.
- decided:  `uze_application::ExtensionReport` (`Extensions::reports(project: Option<&Path>)`), with an `ExtensionCondition` word (`fixtures-only`, `needs-consent`, `needs-enabling`, `enabled`, `consented` with no project asked about, `held`, `broken`). The consent record gains `updated_at: Option<u64>`, set by `apply_update` and cleared by `accept`, read and written in shape 1 (`serde(default)`, absent when empty). The day is printed in UTC (`YYYY-MM-DD`). `checkout_refused` is filled only for an extension that would run in the project asked about. `uze inspect` judges in the project it runs in (`inspect_in`) and says which in `extensions_project`.
- rejected: A second read model per surface: four renderers deriving the same states would drift. Showing `accepted_at` as the update's date: a consent and an update would read the same.
- blast:    `ProjectStatus`, `MachineStatusReport` and `PluginInspection` carry `extensions` in their JSON.

## D41 — Enable and Disable of a script extension are this project's, and Enable asks for the grants first

- status:   pending
- task:     7.1
- commit:   f36316ea
- trigger:  D5 keeps the machine-wide switch beside the record, and the screen's one pair of offers (`EnableExtension`, `DisableExtension`) switched it; consent and enabling per project need the same keys.
- decided:  For a declared extension that runs code, seen in a project, Enable opens a dialog with each grant and what it lets the extension do when consent is missing or a held update widened (`Allow and enable`), then consents and enables it in that project, turning the switch on if it was off; once consented it enables directly. Disable disables it in the project and keeps consent and the switch. A built-in, a fixtures-only or broken extension, or any extension seen with no project open keeps the switch alone. Enable is unavailable, naming the reason, when the checkout is refused. Enter does Enable when offered, else Disable, and the footer names that.
- rejected: A third offer for consent: two keys for one decision, and a consent never enabled anywhere is a question asked for nothing. Disable switching the machine off too: it would take the extension from every other project.
- blast:    The worker reads `crate::asker::asker()` for the screen's answers.

## D42 — The offer at opening is remembered in the consent record when put away

- status:   pending
- task:     7.2
- commit:   f94a3918
- trigger:  The offer must not return on every opening once declined; the commands' pattern (ADR-055) remembers the question only for the session, and an unconsented extension has no entry in the record to hold a decline.
- decided:  Like the commands, the toast is raised once a session per project (`Remembered::extension_offers`) for each extension of a plugin the project's `agents.yaml` declares that waits for the operator there and whose checkout may be read (`Extensions::offers`). Putting the dialog away (Cancel, Esc, a click outside) is written to a top-level `offers_declined` map of `state/extensions.json` by identity and canonical project, kept apart from the consents so it judges nothing; `forget_plugin` drops it with the rest. The Extensions screen still offers the extension. After one answer, the next extension the project offers is asked about straight away.
- rejected: Keeping the decline in `ClientLayout`: it is the operator's answer about an extension, and a layout reset would bring the question back. A `Consent` entry with no grants: `judge` would read it as a consent held by every grant.
- blast:    A record written by this build carries `offers_declined` when someone put an offer away; shape 1 reads it as optional.

## D43 — Who answers is read once per client, from the environment it started in

- status:   pending
- task:     7.1, 7.2
- commit:   f36316ea, f94a3918
- trigger:  D9 makes `src/cli/workspace.rs::asker()` the one reading of the stamps, but the binary's library half (the TUI) cannot reach the CLI's module, and a test suite run inside an agent's pane carries the stamps.
- decided:  `asker()` moves to `src/asker.rs` in the library half and `cli` re-exports it, so the CLI and both TUI surfaces read it one way. The workspace's `Attach` reads it once when the client starts and hands it to the offer's answer; the test harness sets `Operator`. A client an agent's pane started is refused like the CLI.
- rejected: Trusting the TUI because only a keyboard reaches its dialogs: a client started by an agent in its own pane is driven by that agent.
- blast:    None beyond the move.

## D44 — `--run` rehearses scripts without consent and its answers stand in for fixtures

- status:   pending
- task:     7.5
- commit:   928ab8fd
- trigger:  The task asks `--run` to execute scripts in the engine against a checkout with the tests' inputs, but `.test.kdl` gives data to sources (`given`), not inputs to scripts, and the runner answers only consented, enabled extensions.
- decided:  `ExtensionRunner::rehearse` loads the extension's directory as the check reads it, compiles it, and runs one `script` source with the input it declares, over the checkout `--checkout <path>` names (the current directory by default), refused as the runner refuses one (D11); no consent is asked, since the check runs what its caller is authoring, and the engine still registers only the manifest's grants. Each answer replaces that source's fixture before the views are bound and the tests draw; a test's `given` still wins, since it states its own world. A script that fails, or a refused checkout, is an error of the extension placed on its script, so the check fails. Fixtures, programs and sources not answered by a script are not run. The text report says `<script> answered \`<source>\` in <checkout>`; the JSON carries `runs` by extension.
- rejected: `given` overridden by the run: a test that pins its data would stop testing what it says. Asking for consent in the check: an author would consent to their own draft on every save.
- blast:    An agent can run its checkout's scripts through the check; they read only beneath that checkout.

## D45 — A declared view is an index entry, `open:<surface>`, interned beside its actions

- status:   pending
- task:     7.3
- commit:   e2876522
- trigger:  Declared views were on the tab strip (tested since change 3) but not in the action index, whose rows are `Action`s, and D22 left `open` with a global key for later.
- decided:  Each view of a declared extension that is switched on is offered without a key in the index as `Action::Extension` interned as `ext.<plugin>@<marketplace>/<id>.open:<surface>`, labelled `Open <title>` and described with its attribution; no manifest id holds a colon, so it never meets a declared action, and no table binds it. Choosing it opens the view over whatever stands in the pane. Confirmation dialogs of a third party's surface carry the attribution beside their subject, and its `Failed` and `Copy` toasts lead with it (`id · plugin@marketplace: …`); the clipboard gets the text alone. The strip on the surface itself stays task 5.3's.
- rejected: A row kind of its own in the index: two vocabularies for one list. Attributing in the toast's title: titles are the host's short words.
- blast:    `jump` and global keys for views (change 6) can bind these names.

## D46 — A journey's world keeps a temporary directory of its own on Unix

- status:   pending
- task:     8.5
- commit:   24494769
- trigger:  The journey's project lives under `/tmp/uze-journeys`, and an extension refuses to read a checkout inside the temporary directory (D11), so every script extension in a journey said its checkout was refused.
- decided:  `journeys/unix.py` gives the world `TMPDIR=<world>/tmp`, created with it, as `windows.py` already gives `TEMP`/`TMP`. The hand proof exports `TMPDIR` the same way, since its world is under `/tmp` too.
- rejected: Moving the worlds out of `/tmp`: the default is shared with CI and the Docker image. Relaxing the refusal for a journey: the product would be proved with a rule it does not have.
- blast:    A shell gesture's `mktemp` writes beneath the world, and goes with it.

## D47 — A space opens in a directory once a session, apart from its evaluation

- status:   pending
- task:     8.5
- commit:   9dc40662
- trigger:  The journey's offer never came: `uze workspace` opens with a shell in the project, the shell going quiet evaluates it, and the reads made where a space opens (the extension offers, commands awaiting, gates, retired regions) took only directories no evaluation had answered, so a project whose shell stood there first was never asked about.
- decided:  `WorkspaceModel::newly_opened_directories` hands those four reads each directory the sidebar names once a session, remembered in `Remembered::opened` by evaluation key, whether or not an evaluation reached it first; what is evaluated keeps its own `unread_named_directories`.
- rejected: Asking the offers on every evaluation: they are said once a session (D42) and would be read on every quiet pane.
- blast:    Commands awaiting approval, unspelled gates and the retired-region check are now also asked for a project a shell opened first, which they were meant to be.

## D48 — On a declared surface the arrows move between a question's answers

- status:   pending
- task:     8.5
- commit:   65769bb1
- trigger:  A declared action's confirmation could not be answered with → (nothing bound it in `Scope::Declared`, and its footer named no way to the confirm button), and its subject repeated the attribution beside itself (`notes · notes@studio  notes · notes@studio`).
- decided:  `Scope::Declared` binds `left`/`right` to `FocusPrevious`/`FocusNext`, which a standing question reads as moving between its answers and nothing else on a declared surface reads; the footer then says `right <confirm>`. `render_confirm` draws the attribution alone when the subject already is it.
- rejected: Pushing `Scope::Confirm` while a question stands: the scopes are built without the surface's view, and the surface's own keys already reach the question.
- blast:    `left`/`right` are the workspace's on a declared surface: an extension suggesting either keeps the workspace's.

## D49 — An install of what the project declares says what waits, as one of a single plugin does

- status:   pending
- task:     8.6
- commit:   31a60625
- trigger:  Preparing the hand proof, `uze install` with no argument installed both declared plugins and said nothing of their extensions: the report names a declared plugin bare (`proof`), and the question matched only `plugin@marketplace`.
- decided:  `ask_about_extensions` takes a plugin named either way; `an_install_of_what_the_project_declares_says_the_same` holds it.
- rejected: Qualifying the names in `InstallReport`: its JSON is a published shape and prints each name as the lock keys it.
- blast:    Two marketplaces holding a plugin of one name are both asked about after a declared install that installed either.
