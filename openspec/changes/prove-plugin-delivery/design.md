## Context

The motivation is in the proposal (Why). The relevant state of the Lab:

- **Providers.** `conformance/harnesses/<vendor>/provider.py` scripts tool
  calls by names its authors wrote. Claude's provider already records the
  declared tools (`provider.py:99`); nothing reads that record.
- **Hook vocabulary.** UZE's hook vocabulary is one `ToolBinding` table per
  integration (`crates/uze-integrations/src/<vendor>/hooks.rs`). The Rust
  tests check each table against itself (`tests/integrations/hooks.rs:931`).
  - OpenCode's table says no capture was ever taken.
  - Claude's was written from memory.
  - Codex's and Antigravity's cite `--discovery` captures.
- **Results.** `shared/common.py` records results with a free-form `kind`.
  `gate.py` adjudicates only `kind == "adapted"`, while five contract files
  declare with `kind="adapt"`.
  - `evidence/expected.json` holds five OpenCode entries, all
    `versions: ["*"]`.
  - `evidence/<harness>.json` is the baseline for `VERSION DRIFT`. Nothing
    refreshes it, and the drift is only printed.
- **No hooks contract.** `contract/__init__.py` lists no hooks contract, so
  each vertical's hook phase is bespoke.
- **Prompts answered for the user.** Today's Codex vertical answers folder
  trust with Enter (`scenarios.py:140`). It runs hooks with
  `--dangerously-bypass-hook-trust` (`:415`) and pre-seeds
  `[features] hooks = true` (`:54`). `inventory.md` (Sweep A) lists every
  such answer across all four verticals.
- **Product side.** `uze-core`'s delivery inspection has no state for
  "delivered, but the harness holds it back until the operator acts". Its
  context reporting has no notion of whether the harness will load the
  context in this project.

## Goals / Non-Goals

**Goals:**
- A run fails when a vendor release changes a fact a delivery depends on:
  - a tool name
  - an input field
  - a manifest precedence
  - a prompt in front of a capability

  It fails on the night of the release, not when a user reports it.
- Every check is either a measurement or fails. No verdict can be produced
  without the behaviour having been observed.
- The fixes in the proposal land as red-to-green transitions of checks
  that exist before the fix.

**Non-Goals:**
- Pinning harness versions in the Lab image. The Lab keeps testing
  channel-latest, because the user installs channel-latest.
- Replacing journeys. A journey still proves UZE's own flows; the Lab
  proves what the vendor does with what UZE delivered.
- New portable events or effects. Transform is only adopted where Phase 5
  measures it native, under the existing `portable-hooks` contract.

## Decisions

### D1. Two independent vocabularies, one measured truth

**Decision.** The provider extracts the declared tools and their input
schemas from every harness request, in each wire dialect:
- Anthropic `tools[]`
- OpenAI Responses `tools[]`
- OpenAI chat `tools[].function`
- Gemini `functionDeclarations`

The Lab writes `evidence/tools/<harness>.json`, keyed by harness version.
The Lab's `bindings.py` declares its own `alias → native tool + fields`
expectation. The run asserts:
- that UZE's table is a subset of the capture;
- that the Lab's expectation is a subset of the capture;
- that the two agree.

A tool the harness resolves without declaring it to the model, if any
exists, is measured by a `--discovery` capture recorded with its version.

**The snapshot.** The committed snapshot `evidence/tools/<harness>.json`
is read by a deterministic Rust test, so `cargo test` catches a table edit
that contradicts the last measured vendor without Docker. The live run
catches the vendor moving away from the snapshot. A snapshot refresh is a
reviewed diff, like any evidence.

**Alternatives considered.**
- Derive UZE's table from the capture. Rejected: mapping an alias to a tool
  is a semantic judgement, and a generated table would rename silently
  instead of failing.
- Compare only against UZE's table. Rejected: that is validating UZE with
  UZE again. Two lists written independently and both checked against the
  vendor turn a shared mistake into a disagreement.

### D2. A hooks contract, and presence as a typed precondition

**Decision.** Add `contract/hooks.py`, stated in outcome terms, iterating
over `alias × event × effect` from a single matrix that the bindings fill
or declare. Fixture handlers always leave a presence marker. The marker
carries the handler's group, position and every `HOOK_*` value it
received, written by the delivered handler itself, so the harness's own
payload is what is relayed.

`check_absence` takes a required `proof` argument: the result of the
presence check in the same turn. An absence whose proof failed is recorded
as unproven, never as passed.

`settled` is computed by the provider from the tool result it receives.
An error result, an unknown-tool reply or a refused call means not
settled.

**Lint.** A pytest lint in `conformance/tests/` walks the AST of every
scenario and contract. It refuses three things:
- a literal verdict;
- `check_absence` without `proof`;
- an unknown `kind`.

**Alternative considered.** Reviewer discipline, as the conformance-debug
skill asks today. Rejected: the skill already said "gate every absence on a
presence", and the OpenCode phase shipped without it.

### D3. One declaration kind, measured each run

**Decision.** Collapse `adapt` and `adapted` into one `declared` kind,
emitted only through `declare(name, measurement, reason)`. Here
`measurement` is the observed result proving the limitation still holds,
for example "the permission event's resources carried no command".
Passing a boolean literal fails the lint.

`bindings.unsupported` entries become registry entries, with the
measurement that proves each one. The gate refuses `*`.

On an uncovered version, a reproducing declaration passes, and the run
writes `evidence/expected.next.json` with the version appended. CI uploads
it as an artifact, and a maintainer commits it.

**Why not fail on every uncovered version.** Claude Code ships almost daily.
A gate that fails on every release while nothing changed teaches people to
re-pin without looking, which is the habit that let `*` survive. When every
declaration is a fresh measurement, the version list is a record and not a
permission.

**Alternative considered.** Expiring entries by date. Rejected: a date
measures nothing.

### D4. A recorded decision for every prompt the Lab answers, and a first-session scene

**Decision.** `conformance/DECISIONS.md` gains a "prompts the Lab answers"
section, one entry per answer, each naming:
- the prompt;
- why it is outside UZE's scope;
- the code site.

The pytest lint scans `scenarios.py`, `bindings.py`, `entrypoint.sh` and
fixture homes for a deny-list of answer patterns, such as:
- `--dangerously-*`, `--yolo`;
- `bypass`;
- trust entries in seeded config;
- `[features]` in seeded config;
- permission modes.

Each match must carry a `# decision: <id>` comment that resolves to an
entry in the section.

Prompts that affect delivery are answered through the harness's own
interface, in the first-session scene and in every phase that needs them
(for example Codex's hook review key), never by flag.

**Alternative considered.** Keeping the bypass in the hook phases and
asserting the prompt only in the first-session scene. Rejected: the phases
would then prove hooks on a machine no user has. The extra seconds of
answering through the interface buy a green that means something.

### D5. "Held back by the harness" is a delivery report beside the receipt, not a receipt state

**Decision.**
- An integration reports, per installed package, what the harness holds
  back until the operator acts: a new `IntegrationPort` method returning
  `HeldBack { capability, action }` notes, modelled on the existing
  `unreadable` channel (`UnreadableDelivery`) and on `PublicationStatus`.
- The receipt stays `Matched`. `AttachmentState` answers a different
  question: whether UZE owns the artifact and may detach it. A hook awaiting
  Codex's review is UZE's artifact, intact. A new state there would change
  removal (`plan_remove`, `detach_receipt` detach only on `Matched`) and the
  inspection cache (which caches only `Matched`), for no gain in safety.
- Context reachability is answered per harness by the integration's
  context delivery: trust, a setting turning the file off, and size caps.
- Status, inspect, doctor and the install report render both through the
  existing read models, with human and JSON parity (`emit`).

Where the vendor's record of the operator's decision lives (Codex's
per-hash trust store and its project trust table) is measured before code
is written (`experiments/codex/trust-store`). UZE only reads those records,
never writes them. Answering the vendor's prompt for the operator would
repeat the Lab's mistake in the product.

**Alternatives considered.**
- A `Pending` variant of `AttachmentState`. Rejected for the reasons above;
  the 2026-10-06 map of every consumer of the enum (removal, the
  inspection cache, doctor repair, status, the TUI health view) showed none
  that should treat a held-back artifact differently from a matched one.
- Writing the trust entries during install so nothing is pending. Rejected:
  - it bypasses a security control the vendor put in front of the operator;
  - it is outside a receipt-owned artifact UZE may manage;
  - it is precisely the kind of quiet workaround this change exists to
    remove.

### D6. Manifest precedence lives in the integration, measured by a fixture

**Decision.** The Codex integration owns one function that answers "which
manifest Codex reads for this package tree". It follows the precedence
measured in the Lab: the root `plugin.json` with the Agent Plugins
`$schema` first, then `.codex-plugin/plugin.json`. Coverage and the mirror
both read from that function.

If the measurement confirms that Codex loads a UZE package's root manifest
natively, Phase 5 decides whether the explicit route keeps mirroring it or
the generated envelope becomes the only Codex route. That decision is
recorded as an ADR candidate below.

### D7. Order: red first, then fix, then climb

The phases in `tasks.md` are ordered so that every fix in Phase 4 has a
check that was red before it.

The branch carries the hardened Lab and the fixes together. `main` never
receives a Lab that is red for a known reason, and never receives a fix
that no check asserts.

Phase 5 (rungs toward native) starts only after a clean nightly of
Phases 1–4.

## Candidate ADRs

- **The Lab measures the vendor; UZE's beliefs are only inputs to compare.**
  It amends ADR-035, adding the tool capture as truth, the measured
  declarations and the prompts the Lab may answer. It sets a rule every
  future vertical inherits.
- **Held back by the harness as a delivery report.** It changes what
  "delivered" means to a person across status, inspect, doctor and the
  install report, without changing what a receipt means to removal.
- **Codex: a UZE package is a native Codex plugin** (if Phase 5 measures
  it). It would change the Codex route precedence of ADR-040 and its
  successors.

## Risks / Trade-offs

- **Answering prompts through the interface makes phases slower.** Measure
  each harness's added time in the first run. If it exceeds the leg budget,
  split legs (`--part`) rather than reintroducing a flag.
- **Wire dialects for the capture differ per vendor, and a parser bug could
  report "tool missing".** A capture that parses zero tools fails the run as
  a Lab defect, distinctly from a missing binding.
- **Codex's trust storage format is undocumented and may move.** Reading it
  is inspection only. An unreadable record reports "trust unknown", never
  "delivered".
- **Absorbing tasks from `native-first-hooks` and
  `extend-conformance-coverage` can leave those changes inconsistent.**
  Task 0.1 marks the moved tasks in place, with a pointer here.
- **The nightly is red from the first Phase 1 merge until Phase 4.** Phases
  1–4 merge as one series from this branch, not piecemeal to `main`.

## Migration Plan

- The tool snapshots are new evidence files.
- `expected.json` entries are rewritten with measured versions; there are
  no `*` entries to preserve.
- Existing installs pick up the corrected bindings and dialects on
  `uze update` / `uze install` re-projection. Nothing is migrated in place.
- Rollback is reverting the series. The Lab and the fixes revert together,
  because they land together.

## Open Questions

- Whether Codex's project-trust gate applies to `codex exec` as well as the
  interactive session. Task 5.1 measures it, and the answer only selects
  which scene asserts it.
- Whether `Task` still matches as an undocumented alias in Claude Code.
  Task 4.3 keeps it in `also_matches` while the capture shows it declared,
  and drops it when the capture no longer does.
