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

## D20 — `applies-when` names one path inside the checkout, and its presence is the whole condition

- status:   pending
- task:     3.1
- commit:   562b0c43
- trigger:  The umbrella names `applies-when` (D22's kebab-case example, change 4 step 5) and never says what it is written as or what it tests.
- decided:  A surface's `applies-when=".planning"` property and a source's `applies-when ".planning/STATE.md"` child name one relative path, checked like a file the manifest names (no `..`, no absolute path, no `\` or `:`). The engine asks the host, off the drawing thread, whether the checkout holds it (`Host::checkout_has`, which lists the parent and reads nothing of the file; a climbing path is `false`), keeps the bit per (checkout, path) and asks again after `APPLIES_RECHECK` (10 s). Unknown counts as not applying. A surface that does not apply is not offered (tab strip, index doors, drawers, overlays, bands) and its section, status, band, drawer or overlay draws nothing; a view already open stays drawn. A source that does not apply is never asked and is withheld with "Not for this checkout: it has no <path>". Surfaces are judged on the checkout in front, an open view on the one it was opened on.
- rejected: A binding expression over `{context.*}` or `{pref.*}`: context carries nothing that tells checkouts apart, and a preference is the operator's switch, which the extension switch already is. Several paths or and/or: no gallery entry needs more than one marker.
- blast:    `uze-contract` `SurfaceDecl`/`SourceDecl::applies_when`; `uze-core` manifest decoder; `uze-extensions` `declared/applies.rs`, `Extension::applies`, `Host::checkout_has`; `src/ui/orchestrator/declared{,/doors,band,drawer,overlay}.rs` (`offers_slot`).

## D21 — `Select`/`Choice`, `Field` and `Form` are catalog nodes; focus moves with ←→ and Enter acts on the focused input

- status:   pending
- task:     3.2
- commit:   562b0c43, b651722b
- trigger:  D24 names `select`, `field` and `form` but not their spelling, their keys, or how a declared view (which never sees the keyboard) takes text.
- decided:  `Select "Label" id=… { Choice "Label" value=…; }` like `Tabs`/`Tab` (first choice until one is chosen, `show` checked against the declared values); `Field "Label" id=… placeholder=…`; `Form action=… submit=… item=… { … }`; `List filter="ui.<id>"` keeps the rows with any text or number (three levels deep) containing what was typed, case-insensitively, checked statically against an `id` the view declares. Values are host state under the input's key; focus and the field typed into are kept under `#focus`/`#editing`, keys no view can read. In the Declared scope ←/→ (both arrive as `FocusNext`) cycle focus through the inputs in drawn order and Enter acts: a select steps on (wrapping), a field starts typing, a form is sent. While a field is typed into `Extension::typing` is true, so the host seals the view as it does for the code editor: letters and paste go into it (one line, control characters dropped, at most `FIELD_MAX` = 1024 characters), Backspace erases, Tab moves to the next input, Enter sends its form (or stops typing outside one), Esc stops typing and only the next Esc closes the view; a press anywhere else stops typing. Mouse: `‹` steps back, the choice and `›` step on, the whole field (both rows) starts typing, the button sends. Drawn as `label ‹ choice ›` (the harness picker's shape), a field over the input rule with the caret only while typed into, the rule accent with focus.
- rejected: Distinguishing ← and → for a select: both reach a declared view as `FocusNext` (`COMMAND_ACTIONS`), and splitting them changes every built-in surface's left arrow. Text input in sections, drawers and overlays: only the view in the pane is routed text by the host (`open_typing`); a field there takes a click but no keys — left open.
- blast:    `uze-contract/src/catalog.rs`; `uze-core` view decoder (`declare_state`, `refuse_unknown_filters`); `uze-extensions` `view.rs` (`NodeKind::{Select,Field,Form}`, `NodeAct::{Step,Edit}`), `declared/{bind,input}.rs`; `src/ui/extension_view/tree.rs`.

## D22 — An action reads only what its `with` names; a form delivers only the fields its action names

- status:   pending
- task:     3.2
- commit:   562b0c43
- trigger:  D24 says actions declare what they read (`with = ["ui.tab", "form.*"]`) and that a form's submit delivers `form.<id>`, without saying which wins when a form is bound to an action that names none of its fields, or how a program receives them.
- decided:  `with "ui.<id>" "form.<id>" "form.*"` is a child of `action`. A script's `input` and a program's stdin `input` gain `ui` and `form` objects holding exactly the named values, each cleaned as `Clean::text`; templates read `{form.<id>}` (beside the item; it no longer counts as reading the item, so no row need be selected). A form sent to an action whose `with` names none of its fields runs nothing and says so in the view's notice. Argv and `UZE_IN_*` keep reading the item only. A field's id is a literal: the per-item id of D24 (`id="comment.{id}"`) is not taken.
- rejected: A form's submit implying `form.*`: two places would declare what reaches the program. Substituting form values into argv: `substitute` refuses every scope but the item, and widening it is the runner's change, not the engine's.
- blast:    `uze-contract` `ActionDecl::{with,reads_ui,reads_form}`; `uze-core` action decoder; `declared/action.rs` (`reading_scope`, `action_input`).

## D23 — A two-step action is `run`/`script` then `then { agent-prompt … }`, filled from `answer.*`

- status:   pending
- task:     3.3
- commit:   562b0c43, b651722b
- trigger:  D24 gives the idea (`text = "{answer.prompt}"`) and no manifest spelling; D16 forbids unattended text that is not fixed.
- decided:  `action … { run "./bin/next.sh"; then { agent-prompt "Review {answer.file}" target="self"; } }`, allowed only after `run` or `script`; `when=` inside `then` is refused; it needs `agent:prompt` (`ActionDecl::needs`). When the first step answers, its envelope is applied and the prompt is resolved against the item, `form.*` and `answer.*` (the envelope's `data`), cleaned with `Clean::text`, and returned as `Outcome::Agent` with `unattended: false`; the host's answer path now forwards such a request to `ask_of_agent`, which shows it and applies every guard. An empty prompt or a failed first step sends nothing and says why. The envelope's toast gives way to the prompt.
- rejected: A second does-node after the first: "an action does one thing" stays a simple rule. A prompt sent without showing it when the bridge's day-long allowance holds: that allowance is for fixed unattended text only.
- blast:    `uze-contract` `Then`; `uze-core` action decoder and check; `declared/action.rs` (`second_step`); `src/ui/orchestrator/answers.rs`.
