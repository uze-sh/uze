<!-- Decisions taken while implementing, one entry each, judged at archive. -->

## D1 — Host Markdown reads `uze_extensions::shared::markdown`, and `shared` is public for that module alone

- status:   pending
- task:     1.1
- commit:   49b0b551
- trigger:  The task names `shared::markdown`, but `shared` was a private module; release notes were not the only host caller of the `code::markdown` re-export (a plugin's preview, two popups, the extension tree's `Markdown` node and its tests).
- decided:  `pub mod shared` with only `markdown` public and every other module `pub(crate)`; all six callers render through `shared::markdown::render`, and the re-export in `code.rs` is gone.
- rejected: A crate-root re-export (`uze_extensions::markdown`): a second name for the same function, which is what the task removes. Moving only release notes: it would have left the re-export alive for the other five.
- blast:    `crates/uze-extensions/src/{lib,shared,code}.rs`, `src/ui/{release_notes,view/plugins,extension_view/*,orchestrator/render/*}.rs`.

## D2 — A chart with any value that is not a number draws a notice in place of the chart

- status:   pending
- task:     1.2
- commit:   8029f089
- trigger:  "A notice" did not say whether a series with some numbers draws them, skips the rest, or draws nothing.
- decided:  Any value of the first 512 (`CHART_MAX`) that is not a number (a numeric string still is) replaces the chart with a `Warning` notice, `Chart: <n> of <m> values are not numbers`; a `series` that is present and not a list says so too. An absent or null `series` is still an empty chart: the data has not arrived.
- rejected: Dropping the unreadable values: the bars shift left and the chart lies about time.
- blast:    `crates/uze-extensions/src/declared/bind.rs::chart`.

## D3 — `Clean` deserializes as text, and `Span::clean` is the declared engine's only way to make a span

- status:   pending
- task:     1.3
- commit:   75b8b539
- trigger:  A `Deserialize` that sanitizes must pick one of two bounds (label or text), and `view::Span` has a public `text: String` that every built-in extension and the host fill with their own text.
- decided:  `Deserialize` reads a string and cleans it as `Clean::text` (the looser bound; a label is cut where it is drawn). Lengths are counted in the cells `unicode-width` gives each character, a line break counting one; a wide character that would overflow is left out whole. `Span::new` stays, documented as the constructor for host-owned text, and `Span::clean(Clean, Role)` is added; `tests/architecture/layering.rs::the_declared_engine_builds_spans_only_through_clean` fails on `Span::new(`, a `Span {` literal or a write to `.text` in `declared.rs`/`declared/` outside tests. `Clean` gained `one_line` and `lines`, which keep it clean.
- rejected: Making `Span::text` private: about 130 call sites in the built-in extensions and `src/ui`, two of them in areas parallel work is changing, for a guarantee the built-ins do not need. Renaming `Span::new` to a host-named constructor: the same churn.
- blast:    `crates/uze-contract/src/clean.rs`, `crates/uze-extensions/src/view.rs`, `declared/{bind,action,notify}.rs`, `declared.rs`, `docs/architecture/invariants.md`.

## D4 — An agent action's key needs a modifier, with the confirm action's message shape

- status:   pending
- task:     1.4
- commit:   e05fb04b
- trigger:  The umbrella's 8.2 says "a confirmed or agent action refusing a bare-letter suggestion"; "agent action" was not listed.
- decided:  `agent-prompt` (gesture or unattended), `agent-restart` and `focus-agent` refuse a bare key with "the action `<id>` reaches an agent, so its key needs a modifier, not a bare `<k>`" and the same `ctrl-<k>` suggestion; the Skill's `key` paragraph says so.
- rejected: Exempting `focus-agent`, which only moves focus: one stray letter taking the operator's keyboard into another agent's pane is the harm the rule is for.
- blast:    `crates/uze-core/src/capability/extension/manifest/check.rs`, `plugins/uze/skills/extension/SKILL.md`.

## D5 — The schema and the reference are written by an example of `uze-contract`, compared by the root suite

- status:   pending
- task:     2.1, 2.2
- commit:   55bc305d
- trigger:  The repository's generated artifacts (the harness matrix) use a binary with `--check`; `uze-contract` may name no workspace crate, so it cannot dev-depend on itself to turn its own feature on in tests.
- decided:  Feature `schema` on `uze-contract` (`schemars` 1, already locked through the MCP fixture, so no new crate and no `cargo deny` change; MIT, pure Rust, GREsau's single-maintainer crate the dependencies rule names as tier 4) derives `JsonSchema` on the manifest's types and the envelope; `docs.rs` adds the catalog nodes as hand-built defs (`<Name>Node`, `CatalogNode`, `ComposedAnswer`), one worked manifest per slot each fed by a different source kind (six covering six), and two composed answers. `cargo run -p uze-contract --features schema --example extension-docs` (also `make extension-docs`) writes `docs/extensions/schema.json` and `reference.md`; the root crate's dev-dependency turns the feature on and `tests/architecture/generated_docs.rs` fails naming that command. The reference is generated too, so it is in the same drift test; the prose for slots, sources, grants, events, actions, members and preference kinds lives in `docs.rs` and generation stops when a name the contract accepts has no line. The composed common properties moved to `catalog::COMPOSED_PROPS` so the engine and the schema read one list.
- rejected: A `[[bin]]` in the root crate: it would put a docs generator beside the shipped binary. A `UZE_BLESS` environment variable on the test: an unusual workflow here, and a test that writes files.
- blast:    `crates/uze-contract/{Cargo.toml,src/docs.rs,src/catalog.rs,examples/}`, `Cargo.toml` (dev-dependency), `Makefile`, `docs/extensions/`, `tests/architecture/`, `declared/compose.rs`.
