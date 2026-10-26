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
## S1. `rhai` reviewed, and one licence the earlier review missed (task 4.1)

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
said every licence was on the list; it was not. Open, for the operator: an
MPL-2.0 exception for `smartstring` alone in both files (file-level copyleft;
UZE does not modify it, and its source is published), or another engine.
`cargo deny` and `cargo-about` are not installed on this machine, so neither
was run.

## S2. `uze-script`'s shape where the plan did not settle it (tasks 4.1–4.4)

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
