## Why

The same migration report (see `deliver-the-whole-plugin`) rated UZE 6/10 as
a substitute for native Claude Code delivery, and projected 8.5 to 9 once the
delivery bugs are fixed. What it named as the gap to 10 is not a bug but
opacity: UZE sits between the author and the harness, and when something
does not appear, the only way to learn why is to read `settings.json`,
`~/.claude/agents`, the plugin cache and the Store by hand. Concretely, on
1.0.0-beta.3:

- **The install report hides the route.** It prints `<harness>: native
  (<location>)` for both an explicit and a generated envelope, keeps only the
  last location per harness (a `BTreeMap` keyed by harness), and omits every
  loose attachment of a harness that also has a plan — so the duplicate
  delivery was invisible. `--verbose` added nothing the tester could use.
- **A failed install leaves the package installed.** The Store ingests the
  package before any harness attaches; when an attach fails the command
  prints "Failed to install", yet `uze status -m` lists the package and
  `uze doctor` reports "1 matched". Update has a rollback (`reinstate`),
  install has none.
- **`update` says "already current" after updating.** For a linked
  marketplace the compared revision is the checkout path, which never
  changes; the Store did receive the new bytes.
- **`doctor` counts receipts, not intent.** "1 matched, 0 missing" for a
  package that should have delivered five capabilities.
- **Degradation is silent.** ADR-031 promises "route evidence rather than
  hidden loss"; an agent losing its prefix, a frontmatter field the target
  cannot read (OpenCode drops the agent entirely), a file that does not reach
  the plugin root — none is said.
- **Nothing shows what the harness will see** without an authenticated
  session, which is what a plugin author needs to test a release.

## What Changes

- **Install and update report every delivery**, per harness: the route
  (explicit envelope, generated envelope, loose), why that route, and each
  capability with the name the harness exposes it under and its status
  (native, adapted with what was lost, shadowed, unsupported with the
  reason). Text and JSON carry the same facts.
- **`uze inspect <plugin> [--harness <h>]` shows the effective view** from
  the same model: the final names a session would see, per capability and
  route, with no session and no network. It is also the regression surface a
  plugin author runs before publishing.
- **A failed install is not an installed package.** When no harness
  attaches, the Store entry and any partial receipts are rolled back. When
  some harnesses attach and others fail, the package stays, is recorded as
  partially delivered, and `status`, `inspect` and `doctor` say so with the
  failing harness and error.
- **`update` compares content**, not provenance display: a package whose
  bytes changed reports the change, whatever its source kind.
- **`doctor` checks delivery against intent**: for each package, every
  capability the plan expects per detected harness is present, under the
  expected name, readable by the harness (a plugin cache that is empty, a
  label the harness would refuse, an agent whose frontmatter the harness
  drops). Receipt counts stay as detail.

## Capabilities

### New Capabilities

- `delivery-reporting`: what install, update, inspect and doctor say about
  where and how each capability reached each harness, and what a failed or
  partial delivery leaves behind.

### Modified Capabilities

<!-- none; `plugin` and `doctor` spec requirements are extended by the new
capability rather than rewritten -->

## Impact

- `crates/uze-application`: `lifecycle/install.rs` (rollback, partial
  record), `project_environment.rs::update_machine` (content comparison),
  `doctor.rs` (expected-vs-delivered), `read_models.rs` (effective view read
  model built from the exposure plans).
- `crates/uze-integrations`: each integration contributes a readability
  check for its own artifacts (plugin cache present, label valid, agent
  frontmatter accepted); vendor knowledge stays there.
- `src/main.rs`: `render_add_report`, `render_update`, `inspect`, doctor
  rendering; `--harness` flag on `inspect` (classified `Budgeted` in
  `command_performance.rs`, read-only).
- A partial-delivery marker is a record (state tier) and declares its shape
  through `uze_document::Shaped`.
