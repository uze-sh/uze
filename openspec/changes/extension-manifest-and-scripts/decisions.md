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
