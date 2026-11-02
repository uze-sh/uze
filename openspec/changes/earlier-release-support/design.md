## Context

A census of the workspace found about 91 places that exist only because an
earlier release left something on a machine, in five kinds. Half go through
a shared mechanism; half are inline:

| kind | sites | through a mechanism | inline |
|---|---|---|---|
| record shape | 8 | 6 (`Shaped` ladders) | 2 (`marketplace_catalogue.rs:161` default, `terminal/runtime/server.rs:1014` clamp) |
| path in the UZE home | ~22 | ~9 (`owned.rs` retired entries) | ~13 (`carry_into_sources` inside `MutationLock::acquire`, `carry_across_the_move` inside `record::ensure`, `setup.rs` `RETIRED` and `catalogue.rs` `Subject::Retired` repeating the map, `runtime/attachments` cleanup, `RETIRED_ENGINE_PATH`) |
| outside the home (harness files, project files, `.git`) | ~45 | ~20 (`retired_region`, receipt retirement in `lifecycle/attach.rs`) | ~25 (`uze-local` in five functions, `uze-official` in `setup.rs`, OpenCode bridges across `bridge.rs`/`opencode.rs`/`rules.rs`, four skill-wrapper cleanups, `StaleProjection` beside `RetiredRegionsPlan`, earlier checkouts across `pool.rs`/`reconcile.rs`/`task.rs`/`occupancy.rs`) |
| wire, endpoint, process | ~11 | 9 (`terminal/runtime/endpoint.rs`) | 2 (`--resumed`, `uze terminal` in `cli/grammar.rs`) |
| command spelling, config | 3 | 1 | 2 |

Code that copes with a *newer* build (the refusal of a newer record, keys
and theme forward tolerance) and with a *vendor's* older release
(`opencode2`, Antigravity's `serverUrl`) is not this change's subject and
stays where it is.

`tests/architecture/layering.rs` already has the shape this change needs
for enforcement: a `Rule` with a forbidden token, a scope, `sanctioned`
files with their reason, an exact `budget` for debt, and a `remedy`
printed on failure.

## Goals / Non-Goals

**Goals:**
- Every piece of support for an earlier release is findable by kind and
  bounded by a release.
- Retiring a release's support is: raise the floor, delete what the build
  names.
- An agent cannot add such support anywhere else without the build saying
  where it goes.

**Non-Goals:**
- Changing what any supported upgrade does: moving a site into its registry
  preserves its behaviour.
- Newer-build tolerance and vendor-version tolerance.
- A runtime "compatibility mode": the floor is a build constant.

## Decisions

### D1. The vocabulary lives in `uze-document`

`uze-document` is already "the one rule for reading a record another build
wrote" and a leaf every crate, the terminal runtime included, may depend
on. It gains:

```rust
pub struct Release { /* major, minor, patch, beta: Option<u32> */ }
impl Release { pub const fn beta(n: u32) -> Self; pub const fn stable(..) -> Self }

/// The oldest release a machine can come from and still be carried across.
pub const FLOOR: Release = Release::beta(22);

/// One piece of support for an earlier release.
pub struct Earlier {
    pub name: &'static str,           // stable id, printed on failure
    pub last_written_by: Release,     // the last release that wrote it
    pub site: &'static str,           // file!() of the entry
}
```

`Step` gains `last_written_by: Release`. `Release` is ordered; a beta is
below the stable release of the same number. No new dependency: the type
is the few pre-release forms UZE publishes, not general semver.

Alternative: a new `uze-earlier` crate. Refused — it would be a second
leaf holding the same rule about another build's state.

### D2. One home per kind

| kind | home | runner |
|---|---|---|
| record shape | the record's `Shaped::ladder()` rungs | `uze_document::read` (unchanged) |
| home path | `owned.rs` `retired(..)`, gaining `.carried_by(fn)` and `.last_written_by(r)` | one `earlier::carry_home` at bootstrap, under the mutation lock, replacing the hooks in `MutationLock::acquire` and `record::ensure` |
| harness files | `uze-integrations/src/<vendor>/earlier.rs`, plus `shared/earlier.rs` for what all verticals share (retired marketplace names, wrapper roots, runtime-tree leftovers) | `IntegrationPort::retire_earlier`, called once from the attach and detach lifecycle |
| project files | `uze-core/src/project/earlier.rs` (today's `retired_region` table) | `uze-application`'s `retired_regions`, which every reader consumes; `StaleProjection` folds into its plan |
| earlier checkouts | `uze-workspace/src/checkout/earlier.rs` | `reconcile`, which calls it |
| wire and endpoint | `uze-terminal/src/runtime/earlier.rs` | `endpoint::make_way` |
| command spelling | `src/cli/earlier.rs` | the grammar, which registers each spelling from it |

Every `earlier` module and registry exposes `fn earlier() ->
&'static [Earlier]`; the architecture suite gathers them all, so one test
sees every entry in the workspace.

Every list that today repeats retired names (`setup.rs` `RETIRED`,
`catalogue.rs` `Subject::Retired`, `uze-official`) is derived from its
registry instead.

### D3. The floor test

`tests/architecture/earlier.rs` fails on every entry, ladder rung and
retired path whose `last_written_by` is below `FLOOR`, printing its name,
its site and the instruction "delete this entry". The same suite checks
that each ladder's rungs are contiguous and each declares a release.

### D4. Support outside a registry fails the build

Rules in the existing `layering.rs` shape:

- **Vocabulary scan.** The tokens that mark support for an earlier
  release (`legacy`, `retired`, `earlier release`, `older build`,
  `previous release`, `used to`) are refused in production source outside
  `earlier` modules, `owned.rs`, `uze-document`, and the ladders. Today's
  sites go into an exact `budget` per file; each migration commit lowers
  it, and it ends at zero. Comments count: a comment that explains a
  fallback marks the fallback.
- **Names written once.** A literal retired path segment or retired
  marketplace name outside its registry fails, naming both places.
- **No serde back door.** `#[serde(alias` anywhere, and
  `#[serde(default` on a field of a `Shaped` record, fail with the remedy
  "add a rung to the ladder".
- **`Standing::Retired` only in `owned.rs`.**

A token scan cannot prove intent; it catches the way such code has been
written here so far, and the exact budget makes every exception visible in
a diff.

### D5. The agent's guide

`.agents/rules/earlier-releases.md`, `model_decision` ("Read before
writing code that copes with anything an earlier UZE release left") plus a
glob on `**/earlier.rs`, `**/earlier/**`, `**/machine/home/owned.rs` and
`crates/uze-document/**`. It says the kinds, their homes, the floor, how to
pick `last_written_by` (`git tag --contains` of the commit that stopped
writing it, minus one), and that the tests' remedy is the instruction to
follow. `document-ladder.md` and `persistence-tiers.md` point at it rather
than restating it. Every architecture failure message names the rule.

### D6. Migration order

1. Vocabulary, floor and the suite, with today's debt frozen as exact
   budgets — from this commit no new inline support can land.
2. One commit per kind moving its sites home, each lowering its budgets.
3. Find each entry's `last_written_by` from history; delete the entries
   below `1.0.0-beta.22`.

## Risks / Trade-offs

- [`last_written_by` set wrong keeps dead code or deletes live support] →
  each value is derived from `git tag --contains`, and the journeys'
  `07-upgrade` chapter runs the floor release against this build.
- [Token scan false positives (`retire_briefing` is current behaviour)] →
  renamed where the word misleads, sanctioned with a reason otherwise.
- [`IntegrationPort` gains a method] → it stays one trait (the
  `delivery-model` rule); the default does nothing.
- [Two open changes modify the same `persisted-state` requirement] →
  `refactor-home-file-shape` drops its beta wording and carries its renames
  as retired entries instead; this change lands first.

## Candidate ADRs

- A release floor bounds every piece of support for an earlier release:
  every later shape change and every retirement depends on it.
