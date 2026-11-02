## Why

UZE copes with what earlier releases left in about 91 places, and half of
them are inline branches and fallbacks: retired marketplace names in five
functions across three files, the retired-path list written three times,
two layout moves hidden inside a lock and a record's `ensure`, near-copies
of one cleanup in every integration. Nothing says when any of it may go, so
none of it ever does, and nothing stops the next one from being written
anywhere. Before the first stable release the support for earlier releases
needs one shape that a person can retire by changing one value.

## What Changes

- A **floor**: one constant naming the oldest release a machine can come
  from and still be carried across, raised to `1.0.0-beta.22`, the last
  published release, once every entry declares its release.
- Every piece of support for an earlier release becomes an **entry** in the
  registry of its kind, and each entry names the **last release whose run
  could still leave what it handles**, or is unbounded when what carries it
  out does not run on every run. Kinds and their one home:
  - a record's shape: a rung of its `Shaped` ladder;
  - a path in the UZE home: a retired entry of the owned map, with the step
    that carries it across;
  - what an integration left in a harness's own files: that integration's
    `earlier` module;
  - what UZE left in a project (regions of `AGENTS.md`, checkouts an older
    build made): the project's and the workspace's `earlier` modules;
  - an older wire, endpoint or command spelling: the terminal runtime's and
    the CLI's `earlier` modules.
  Each runner stays where it runs today and reads its entries from there.
- Raising the floor makes a test fail on every entry below it, naming each;
  deleting those entries is the whole of retiring that support.
- Architecture tests refuse a retired name spelled outside its entry,
  legacy identifiers outside the homes, a serde alias, and a serde default
  on a record field not marked additive.
- A project rule tells an agent where such support goes, before it writes
  any; the tests' failure messages carry the remedy.
- **BREAKING:** bounded support whose last release is below
  `1.0.0-beta.22` is deleted. Unbounded support — a record rewritten only
  when next changed — stays until a release settles it on every run.

## Capabilities

### New Capabilities

- `earlier-releases`: how UZE declares, bounds and retires its support for
  state and behaviour an earlier release left on a machine.

### Modified Capabilities

- `persisted-state`: a ladder step becomes removable when the floor passes
  the last release that wrote its shape, rather than when "no machine can be
  below it".

## Impact

- `uze-document` gains the release type, the floor and a step's last
  release; every `Shaped` ladder declares it.
- `uze-core` (`machine/home/owned.rs`, `package/sources.rs`,
  `project/record.rs`, `project/retired_region.rs`, `delivery/state.rs`),
  `uze-application` (`setup.rs`, `catalogue.rs`, `schema.rs`,
  `retired_regions.rs`, `project_environment.rs`, `lifecycle/attach.rs`),
  `uze-integrations` (each vertical, `shared/marketplace.rs`,
  `shared/path.rs`, `shared/skill.rs`, `rules.rs`, `hooks/bridge.rs`),
  `uze-workspace` (`checkout/*`, `task.rs`), `uze-terminal`
  (`runtime/endpoint.rs`), `src/cli/grammar.rs` and the TUI and CLI readers
  of retired regions.
- `tests/architecture/` gains the earlier-release suite;
  `.agents/rules/` gains `earlier-releases.md`; `document-ladder.md` and
  `persistence-tiers.md` point at it.
- `refactor-home-file-shape` is revised to carry its renames as retired
  entries instead of suspending the upgrade contract.
