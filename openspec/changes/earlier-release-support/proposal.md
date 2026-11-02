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
  from and still be carried across, starting at `1.0.0-beta.22`, the last
  published release.
- Every piece of support for an earlier release becomes an **entry** in the
  registry of its kind, and each entry names the **last release that wrote
  what it handles**. Kinds and their one home:
  - a record's shape: a rung of its `Shaped` ladder;
  - a path in the UZE home: a retired entry of the owned map, with the step
    that carries it across, run once at bootstrap under the mutation lock;
  - what an integration left in a harness's own files: that integration's
    `earlier` module, reached through one lifecycle step;
  - what UZE left in a project (regions of `AGENTS.md`, checkouts an older
    build made): the project's and the workspace's `earlier` modules;
  - an older wire, endpoint or command spelling: the terminal runtime's and
    the CLI's `earlier` modules.
- Raising the floor makes a test fail on every entry below it, naming each;
  deleting those entries is the whole of retiring that support.
- Architecture tests refuse support for an earlier release outside an
  `earlier` module or a registry, refuse a list of retired names written
  twice, and refuse a serde alias or default that reads an old field.
- A project rule tells an agent where such support goes, before it writes
  any; the tests' failure messages carry the remedy.
- **BREAKING:** support whose last release is below `1.0.0-beta.22` is
  deleted; a machine last run on an older release meets the floor of each
  record's class instead of being carried across.

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
