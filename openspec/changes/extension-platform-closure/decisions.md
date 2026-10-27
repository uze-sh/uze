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

## D40 — The runner keeps each extension's account in memory; `uze doctor` reads what a workspace wrote

- status:   pending
- task:     4.1, 4.3
- commit:   fbe06723
- trigger:  D25 and D28 ask for runs, runs per minute, p95 and memory per extension "in the inspector and `uze doctor`", but runs exist only in a running workspace's runner and `uze doctor` is another process.
- decided:  `ExtensionRunner` notes every question it answers (declaration, asking, input as one cut line and an 8-character digest, duration, outcome, whether a script or program ran), the last 64 per extension, and its runs over five minutes; a run is one that executed, so a remembered answer or a fixture is listed but costs nothing. Runs per minute is over at least a minute. Memory is the engine's reckoning (`DeclaredExtension::approximate_bytes`: its compile, its data and events as JSON, its built nodes) plus what the runner remembers of its answers. The workspace writes every extension's cost to `runtime/extensions/costs.json` (generated) every 30 s while any is hosted, and `uze doctor` prints a runs/min · p95 · memory table with when it was measured, and a warning for each over D28's targets (p95 20 ms, 1 MiB).
- rejected: `uze doctor` running the sources itself (side effects, and no rate to measure); a record under `state/` (nothing in it is the operator's).
- blast:    `uze-application` `extensions/{runs,costs,runner}.rs`, `DoctorReport.extension_costs`, `UzeHome::extension_costs_path`, `src/cli/status.rs`.

## D41 — A linked extension's fixture switch uses `fixtures/<source>.json`; the recording is `fixtures/session.json`

- status:   pending
- task:     4.2
- commit:   4d5d78ae, f86cb218
- trigger:  D25 lets the inspector switch "a source with both" a fixture and a script to its fixture, but the manifest gives a source one answerer (`fixture`, `script` or `run`); and "record a session's events into a fixture" names no file.
- decided:  A script- or program-fed source is switched to the fixture named after it, `fixtures/<source>.json`, read off the drawing thread through the same bounded reader as every fixture; one that is missing says so in a toast. The engine then asks nothing for it until switched back. A session's events (the engine's log, oldest first, synthetic ones marked) are written to `fixtures/session.json` by `Extensions::record_fixture`, which writes only into a linked extension's own `fixtures/`, never through a symlink, and nothing else.
- rejected: A `fixture` beside `script` in the manifest: a contract change owned by another group, for an authoring aid; asking the author for a file name: a dialog for one write.
- blast:    `uze-extensions` `declared/authoring.rs`, `uze-application` `extensions/checked.rs`, `src/ui/orchestrator/declared/inspector.rs`.

## D42 — The inspector holds the moves once opened or clicked; its tools are rows, its edit field is a rename's

- status:   pending
- task:     4.1, 4.2
- commit:   f86cb218
- trigger:  The tools need a keyboard path, and the inspector shared the view's keys (only Esc and the page keys reached it).
- decided:  Opening the inspector by its key, or a click in it, lends it Up/Down/Enter: they walk its tools and run one, as a click on the row does; a click on the view gives them back. The tools are text rows under an `author` heading, muted at rest and bright under the pointer or the keys. "rerun <source> with input…" opens a field prefilled with the last input as JSON, typed through the rename scope (Enter reruns, Esc cancels, both also clickable hints under it); only a JSON object is accepted. The rerun is the engine's own refresh job, so the runner answers it under the same consent, and it stands until "put <source>'s input back". Size cycles 80 by 24, 60 by 16, 40 by 12 within the pane; theme cycles the bundled dark and light on the surface alone (`uze_theme::with_active`); both reset when the inspector closes. A synthetic event is delivered marked `"synthetic": true` and sends no agent prompt; fake agents are three fixed rows marked synthetic.
- rejected: A new keymap action to focus the inspector: one more key for something a click and the open key already do.
- blast:    `src/ui/orchestrator/{declared/inspector.rs,render/declared.rs,render.rs,session/*}`, `src/ui/theme.rs`.

## D43 — The runner keeps its listing of installed extensions for a second

- status:   pending
- task:     4.3
- commit:   56344813
- trigger:  Holding D28's storm showed listing every installed plugin was most of a question's cost (8.5 ms of a question with fifteen plugins, unoptimized): the storm took 330 ms for fifteen shared runs.
- decided:  `ExtensionRunner` answers where an extension is installed from a listing at most a second old, dropped whenever `forget` is called (a consent or enabling moved). Its files are still read, digested and judged against the record on every question, so a listing a second stale can only find files that are gone or no longer consented. The storm dropped to about 65 ms unoptimized.
- rejected: Caching the read files or the digest: that is what consent is judged on.
- blast:    `uze-application` `extensions/runner.rs`.

## D44 — D28's clocks are held where they can be: fastest of many, the target itself, unoptimized slack only for the digest

- status:   pending
- task:     4.3
- commit:   56344813
- trigger:  D28's targets were measured in a release build; the gates run unoptimized on a loaded 4-vCPU VM.
- decided:  Binding is measured per gallery view in `uze-extensions`' own `tests/budget.rs`, the fastest of 41 fresh builds against 1 ms (the heaviest, `snake`, binds in about 0.4 ms unoptimized); sections are skipped, since they bind against no space a build can be made fresh by. Memory is `approximate_bytes` under 1 MiB for every gallery extension. The storm (`uze-application/tests/extension_storm.rs`) asserts exactly fifteen runs for 75 questions and the fastest of three storms under 50 ms optimized, 200 ms unoptimized, where the per-question SHA-256 of the extension's files is several times slower.
- rejected: Medians: they move with load; a release-only test: the gates never run it.
- blast:    `crates/uze-extensions/tests/budget.rs` (with `uze-core` as a dev-dependency), `crates/uze-application/tests/extension_storm.rs`.

## D60 — From an agent's pane, `uze install` says an agent cannot enable an extension; the journey drives it with the agent's own stamp

- status:   pending
- task:     5.1
- commit:   b8026a4a, 7a0837f9
- trigger:  12.3 asks for a journey where `uze`, run from an agent's pane, refuses to consent to or enable an extension and says why. The only CLI path that asks either question is the end of `uze install`, and from an agent's pane it printed what any pipe gets ("installed disabled; enable it from the workspace"): nothing said why. No CLI verb consents or enables; a nested `uze workspace` in a pane opens a space in the running uze rather than a client.
- decided:  When `asker()` reads an agent's stamp (`UZE_AGENT` or `UZE_AGENT_KEY`), `ask_about_extensions` never asks, terminal or not, and each waiting extension's line says "<ext> installed disabled; an agent cannot enable an extension, so the operator does, from the workspace" (a held one: "<ext> is held: its update wants …; …"). Journey 21 places an agent from the workspace, then runs the install from a shell carrying the two variables read off that agent's process (`inherit`), and checks the Store and `agents.yaml` written, the line, and `state/extensions.json` absent. Held by `an_install_from_an_agents_pane_says_an_agent_cannot_enable_it` in `tests/cli/extensions.rs`.
- rejected: A `uze extension enable` verb whose only use would be to be refused. Driving the workspace client started with the stamp: no pane can start one (the nested `uze workspace` opens a space instead), and `a_client_started_in_an_agents_pane_cannot_enable_an_extension` already holds that path.
- not proven: the client-side refusal (left to that test); an agent running the install through a harness's tool rather than a shell, which reaches the same `asker()`.
- blast:    `src/cli/extensions.rs`, `tests/cli/extensions.rs`, `journeys/suites/04-workspace/21-an-agent-cannot-enable-an-extension.yml`.

## D61 — The held update is proven by a record only the new script writes; the question reaches the operator on the next opening

- status:   pending
- task:     5.1
- commit:   7a0837f9
- trigger:  12.3 asks that, after `uze update`, a widened extension does not run its new code until the operator allows the new grant, checked on the record and on what is drawn and asked. A script that does not run leaves no trace of its own, and the workspace offers a project's extensions once a session.
- decided:  Journey 22's second release keeps `{ ran: "release two" }` in the extension's store, which wants the `store` grant nobody accepted: the store directory is absent while it is held, and present with that record once it is allowed, so "the new code ran" is read off the disk. The consent's `accepted_grants` and `accepted_digest` are compared with a copy taken before the update (same digest, one grant), `uze status` is the subject that names it held. The question is reached by leaving the workspace (ctrl+q) and opening it again: the offer says the extension runs nothing there, and its dialog shows only the new grant ("Its update wants more than you allowed"). Allowed, the row draws the second release's answer and the project stays enabled without being asked again.
- observed: in the workspace left open across the update, the status row keeps showing the first release's last answer; nothing of the second ran (the store says so), but the row does not say it is held until the workspace asks again. Left as found; a held extension's surfaces saying so in a running session would be a change of its own.
- not proven: the Extensions screen's "Held" row and its consent (held by `src/ui/tests/extensions.rs`); a held program extension's pins.
- blast:    `journeys/suites/04-workspace/22-an-update-that-wants-more-is-held.yml`.

## D62 — A section's answers are its own: they follow the chrome's path, never the view open in the pane

- status:   pending
- task:     5.2
- commit:   58ab8167, efab58f0
- trigger:  Journey 26 (plan-review) pressed "Ask for a review" in the sidebar section and nothing happened: `hit_surface` handed a section's outcome to `follow_surface`, which asks of the agent, toasts and attributes through `slot_in_pane()`, the view open in the pane, so with no view open an `agent-prompt` from a section was dropped without a word (and a copy or a failure was attributed to whatever view was open).
- decided:  A slot that is not the open view (a section, once drawers, overlays and chrome are handled before it) follows `follow_chrome(id, …)`: an agent request is asked of the agent as the section's owner, a toast and a copy are attributed to the section, a close is nothing. Held by `a_sections_button_asks_of_the_agent_rather_than_of_the_open_view`.
- rejected: Teaching `follow_surface` about sections: it is the open view's path, and a second meaning of "the slot" in it is how this was missed.
- blast:    `src/ui/orchestrator/declared.rs`, `src/ui/orchestrator/tests/declared_actions.rs`.

## D63 — An image whose bottom is the screen's last row stays in half blocks

- status:   pending
- task:     5.2
- commit:   c6878578, efab58f0
- trigger:  Journey 27 (pets) found the frame smeared upward a row at a time: the container's tmux speaks Sixel, the pets band sits at the bottom of the pane on the screen's last rows, and a Sixel image leaves the cursor on the row under it, so every frame of the sprite scrolled the whole screen. Any terminal that scrolls after a Sixel image at the bottom would do the same.
- decided:  `render_fitted` places no pixels for a picture whose cells end on the screen's last row; that picture keeps its half blocks. Held by `a_picture_on_the_screens_last_row_stays_in_half_blocks`.
- rejected: Turning Sixel scrolling off (DECSDM): its meaning is inverted between terminals. Moving bands off the last row: a layout change for a terminal quirk.
- blast:    `src/ui/extension_view/picture.rs`.

## D64 — The gallery's journeys: one file per entry, each staging its own extension inline

- status:   pending
- task:     5.2
- commit:   efab58f0
- trigger:  20.2 asks for a journey per gallery entry still without one: gpu-meter, gsd-next, chimes, jump, plan-review, pets, screen.
- decided:  Seven files, 23–29 in `04-workspace`, each the shape of journeys 16–20: the entry's fixture files copied inline as a marketplace's plugin carrying that extension alone (the container mounts only `journeys/`, and one extension is one offer), installed disabled, allowed and enabled from the workspace's offer, then its own claim. Each claim is independent of the others, and a run stops at its first failure, so none shares a file. `pets`' sheet is binary: a first-scene shell step writes it from base64 and publishes it as a second commit of the marketplace. `gsd-next`'s band is fed by its fixture and still waits for consent, since its action wants `agent:prompt`; its button types `/gsd:execute-phase 12`, the stand-in Claude's spelling of the skill, read back from the conversation the stand-in records.
- blast:    `journeys/suites/04-workspace/2{3..9}-*.yml`.

## D65 — gpu-meter is proven where there is no nvidia-smi; its reading is left to its own tests

- status:   pending
- task:     5.2
- commit:   efab58f0
- trigger:  A journey's machine has no GPU, and a stand-in `nvidia-smi` cannot be pinned there: a program under the temporary directory, where every world lives, is refused a pin (`unpinnable`).
- decided:  Journey 29 first checks the world finds no `nvidia-smi`, then proves what such a machine meets: the programs step names it as "`nvidia-smi` not found", allowing it anyway records `exec:nvidia-smi` and `store` with no `exec_paths` pin, the section says "`nvidia-smi` not found; resolve it again on the Extensions screen", and no store (no history) is written.
- not proven: a reading, its meters and history, the `warn-at` preference, `nvidia-smi` confined with its `reads` (held by the extension's own tests against `fixtures/gpu.json`). On a machine whose search path has `nvidia-smi` the journey fails its first scene rather than proving something else.
- blast:    `journeys/suites/04-workspace/29-a-meter-whose-program-this-machine-lacks.yml`.

## D66 — chimes' bell is read off the bytes the app wrote to its terminal

- status:   pending
- task:     5.2
- commit:   efab58f0
- trigger:  20.2 expected the bell itself not to be observable, only the status row.
- decided:  Every cue is the terminal's bell (`send_notification`), and `open`'s `tap` keeps every byte the app writes, so journey 24 counts the BELs standing outside OSC sequences (which may end with the same byte) with a small script staged in the world: none before the turn, exactly one after the one turn that finished. The workspace's own chime is silent on a machine that never chose one (`config.toml` absent, checked), so the bell is the extension's.
- not proven: that anybody hears it (the terminal's business); the `attention` cue and notification on `agent.approval.asked`, which the stand-in never raises.
- blast:    `journeys/suites/04-workspace/24-a-finished-turn-rings-the-chime.yml`.

## D67 — jump, pets and screen: what their journeys read, and what the screen alone tells

- status:   pending
- task:     5.2
- commit:   efab58f0
- trigger:  Which agent is in front, a sprite's frames and a picture's pixels are on no disk.
- decided:  jump (25): each of two isolated agents is sent a line only it shows, and the gate after ctrl+g is the chosen agent's line with the picker's header gone; a list's first row is selected as the picker opens, so ctrl+g takes the first agent and Down then ctrl+g the second. pets (27): the gates read the agent's label and state written beside the sprite (ready, working, then done or ready), the checks read the store (`name` Pixel, then `fed` 1 after Feed). screen (28): opened 60 rows tall so the section has room for its picture; the gates wait for its half blocks (the container's tmux speaks Sixel, so pixels are sent over them, and are not read); the checks read the pinned `sh`, the picture kept under `runtime/extensions/pictures/media@studio/screen/shot.png`, no scratch left, nothing in the checkout.
- not proven: Enter on a picker row (Enter activates a focused input, D21, and does not press a row's button); a row's Go by pointer (exercised while probing, not kept as a scene); which sprite frame plays; the picture's colours or size in pixels.
- blast:    `journeys/suites/04-workspace/{25,27,28}-*.yml`.

## D70 — Closed on one full check and the umbrella's ledger

- status:   pending
- task:     5.3
- commit:
- trigger:  The change exists to leave `extension-platform` with no open task.
- decided:  The umbrella's 17 open tasks are ticked, each pointing at the group of this change that delivered it; `extension-platform` has 0 open tasks. Every step of `make check` passes in one run (56 test suites) with PATH free of the machine's broken `mise` `bun` shim and the two `foreground_status_*` tests of `uze-terminal` skipped by name: both fail identically on `main` (`f5a7972e`) and this work does not touch their code. `openspec validate --all --strict` passes (81).
- rejected: Fixing the two `uze-terminal` tests here: they belong to the terminal runtime, not to the extension platform.
- blast:    None.

## D80 — A manifest's `reads` and a Seatbelt profile's paths are POSIX paths, judged the same on every host

- status:   pending
- task:     PR #309 (Windows tests)
- commit:
- trigger:  On Windows, `exec … { reads "/dev/nvidiactl" }` was refused as "not an absolute path", because `Path::is_absolute` asks the host, and `/dev/…` is not absolute there. `uze extension check` failed the gallery's `gpu-meter` (5 problems), the two `uze-extensions` budget tests failed to compile the gallery, and `an_exec_names_what_a_wanted_program_reads` failed. The Seatbelt profile, pure text built and tested on every platform, refused `/usr/lib` the same way.
- decided:  A `reads` path is absolute when it starts with `/` and has no `..`; a Seatbelt path when it starts with `/`. Both are read only by Landlock and Seatbelt, on Linux and macOS, so a manifest checks the same wherever it is checked and a plugin's author on Windows is told what a Linux user's confinement will be given. `gpu-meter`'s fixture is unchanged: on Windows its program runs unconfined (`confine::availability()` is unavailable, its consent records `enforced: false` and the operator is shown so), and its `reads` mean nothing there, which is not a fault of the manifest. A Windows spelling (`C:\Windows`) is refused on every host.
- rejected: Skipping the check of `reads` on Windows: the same manifest would pass on one machine and fail on another. Gating the Seatbelt tests to macOS: the module is compiled everywhere so that its string building is tested on every CI runner.
- blast:    `crates/uze-core/src/capability/extension/manifest.rs::exec`, `crates/uze-platform/src/confine/seatbelt.rs::rule`.

## D81 — D28's storm clock holds where it was measured; elsewhere a one-second backstop, the run count the claim

- status:   pending
- task:     PR #309 (Windows tests)
- commit:
- trigger:  On the x86-64 Windows runner the fastest of three unoptimized storms took 216.9 ms (of 216.9, 345.9, 277.1 ms) against D44's 200 ms; the Arm runner passed. The cost is the per-question digest's reads, which the platform's filesystem charges for.
- decided:  `MEASURED_ON = ["linux", "macos"]`, matched as data against `uze_platform::target::OS`: there the storm keeps D44's 50 ms optimized and 200 ms unoptimized; anywhere else it is held to 1 s, still catching a storm an order of magnitude slower, while the exact claim (15 runs for 75 questions) holds everywhere. The binding and memory budgets (`uze-extensions/tests/budget.rs`) are unchanged: on Windows they failed compiling `gpu-meter` (D80), before measuring anything, so no Windows number argues for moving them; should one fail on its clock, it takes the same table. `uze-script`'s `each_call_has_its_own_budget` failed on its first call on the x86-64 runner, the whole run ending at 0.11 s: the 100 ms deadline, not the operation bound, which a sixth of the bound per call reaches in an unoptimized build there. It now spends about a thirtieth of the bound per call over 48 calls, still past the bound in total (250 000 iterations in one call already stop at it), so it tests what it names on any machine.
- rejected: A generous ceiling everywhere: it would stop the Linux gate catching a 4x regression. A clock calibrated against an unshared storm on the same machine: the digest is paid by both, so the ratio says little about sharing.
- blast:    `crates/uze-application/tests/extension_storm.rs`, `crates/uze-script/tests/resident.rs`.

## D82 — A harness's reported path is placed in the checkout by the platform's rules, and carried with `/`

- status:   pending
- task:     PR #309 (Windows tests)
- commit:
- trigger:  The bridge kept a report's path when `Path::is_absolute` and `strip_prefix` placed it inside the checkout: on Windows a `C:/…` or differently cased spelling of the checkout, or a rooted `\etc`, was misjudged, and the kept path carried `\`.
- decided:  `uze_platform::path::relative_within` (spelling only, on `is_within`'s rules: case, separators and the verbatim prefix on Windows) gives the part under the checkout of an anchored path (`is_anchored`); a relative one is kept as written; either is kept only when every part is a plain name, and an event carries its parts joined with `/` on every platform. The test builds its checkout from the temporary directory, so it is an absolute path on whatever runs it.
- blast:    `crates/uze-platform/src/path.rs`, `src/ui/orchestrator/declared/bridge.rs::inside_checkout`.

## D90 — macOS bounds no program's memory, and says so

- status:   pending
- task:     CI on macOS
- commit:
- trigger:  Every confined run on the macOS runner failed with `uze __confine: not confined: Invalid argument (os error 22)`: `setrlimit(RLIMIT_AS, 2 GiB)` is refused by macOS, which will not set an address-space limit below what a process already maps.
- decided:  `rlimit::apply` sets the address-space limit only on Linux; elsewhere it is skipped. `Availability` gains `memory` (true for Landlock and seccomp, false for Seatbelt and where nothing confines), and its summary names "a memory bound" among what a machine does not enforce, so `uze doctor` and the Extensions screen say it. CPU time, file size and open files stay bounded on macOS.
- rejected: Skipping the limits on macOS altogether: three of the four still hold there.
- blast:    `uze-platform::confine::{rlimit, macos}`, `Availability`.

## D91 — On macOS a confined program can leave its process group

- status:   pending
- task:     CI on macOS
- commit:
- trigger:  The test that a confined program cannot leave the group the runner ends relies on seccomp refusing `setsid`; Seatbelt filters no system call.
- decided:  The test runs where the confinement filters system calls (`Availability::syscall_filter`) and says why it is skipped elsewhere. Open for the operator: on macOS a program's children may outlive the run's time limit, since the runner ends a group they can leave; the wall-clock and CPU limits still end the program itself.
- rejected: Claiming the guarantee on macOS without a mechanism behind it.
- blast:    `tests/lifecycle/extension_programs.rs`; a product question.

## D92 — Seatbelt is handed the paths the kernel resolves, and its refusal is EPERM

- status:   pending
- task:     CI on macOS
- commit:
- trigger:  On macOS `/var`, `/tmp` and `/etc` are links into `/private`, and Seatbelt matches resolved paths; Seatbelt refuses with `EPERM` where Landlock answers `EACCES`.
- decided:  `confine::macos::enter` resolves every path of the confinement before building the profile (a missing one fails the run, as the contract already demands); the tests accept either refusal, and read another process's environment only where `/proc` publishes it.
- rejected: Writing both spellings into the profile: a rule per alias, for a resolution the kernel already does.
- blast:    `uze-platform::confine::macos`, `tests/lifecycle/extension_programs.rs`.

## D93 — The macOS and Windows secret stores are not exercised by unit tests

- status:   pending
- task:     CI on macOS
- commit:
- trigger:  `fetch`'s secret test stored into the runner's login Keychain, which CI keeps locked (`security` exit 45), and would write a developer's own.
- decided:  The test runs where a secret is kept in a file (Linux and other Unixes), as `uze-workspace`'s own secret test already does; the Keychain and Credential Manager paths stay unproven by tests (extension-growth D5).
- rejected: A throwaway keychain created in the test: `security create-keychain` changes the user's search list.
- blast:    `extensions/fetch.rs` tests.

## D100 — Journeys compare a recorded Windows path with forward slashes, and the rest of this round's journey fixes

- status:   pending
- task:     CI journeys
- commit:
- trigger:  On Windows UZE records `C:\…` while a journey's placeholders spell `C:/…`, so every `enabled_in` check failed; journey 15 counted `costs.json` as a compiled view; journey 22's `sed -i` is refused by macOS `sed`; journeys 17 and 28 run packaged `#!/bin/sh` programs.
- decided:  The runner's `json` check passes each string through `machine.spell_value` (forward slashes for a drive path on Windows, identity elsewhere). Journey 15 counts only digest-named files and leaves `costs.json`, which the workspace rewrites every 30 s, out of its tree comparison. Journey 22 writes through a temporary file. Journeys 17 and 28 are unsupported on Windows, where `sh` is not on the search path and the programs step rightly says so. Journeys 15 and 22 passed in Docker; what Windows and macOS prove is CI's to show.
- rejected: Recording paths with forward slashes on Windows: the record is the host's spelling, read by the host.
- blast:    `journeys/{journey,windows,unix}.py`, journeys 15, 17, 22, 28.

## D101 — A subagent's checkout is released whenever the target fast-forwards

- status:   pending
- task:     process finding
- commit:
- trigger:  Four subagent checkouts were moved to a detached `main` mid-work, losing uncommitted edits, at the moments the operator's workspace ran `sync_target` and found `main` fast-forwarded: `checkout::slots` then reconciles every checkout, and a subagent of an agent running in another process leaves no pane in its directory.
- decided:  Recorded, not changed: whether a subagent with no pane is "in use" is the workspace's design question (`checkout::Presence`), not the extension platform's. Work continued in the agent's own checkout, committed fix by fix.
- rejected: Fixing the reconciliation from this change.
- blast:    None here; a UZE workspace issue for the operator.

## D120 — Below Landlock ABI 6, a confined program signals only itself and its own group

- status:   pending
- task:     review finding
- commit:
- trigger:  The signal filter let every non-negative target through, so on a kernel without Landlock signal scoping a confined program could `kill` any process of the user by its id.
- decided:  `kill`, `tkill`, `tgkill`, `rt_sigqueueinfo` and `rt_tgsigqueueinfo` accept as first argument only 0, the program's own pid (the helper's, known before `execve` and kept by it) and the negated own group. A child or a thread of the program, having an id of its own, is reached through the group; a child's `raise` (which targets its own pid) is refused there, an accepted cost on kernels without scoping.
- rejected: Tracking the program's children by id: a filter is fixed when installed and cannot learn ids born later.
- blast:    `uze-platform::confine::linux::{seccomp, linux}`.

## D121 — No confinement mode listens or accepts

- status:   pending
- task:     review finding
- commit:
- trigger:  With network granted the filter allowed `listen`, `accept` and `accept4`, though `Confinement::network` promises no listening port, and Landlock below ABI 4 does not deny binding.
- decided:  The three calls join the calls refused in every mode; `bind` stays allowed with network (a client may bind a local address, and Landlock denies a TCP bind where it can).
- rejected: Refusing `bind` too: it would refuse an outgoing UDP socket bound before sending.
- blast:    `uze-platform::confine::linux::seccomp`.

## D122 — A run's streams are waited for one second after it ends

- status:   pending
- task:     review finding
- commit:
- trigger:  After the program ended, the stdin writer and both readers were joined with no deadline; a descendant that left the group (macOS, an unconfined run) holding the pipes kept `run` waiting as long as it lived.
- decided:  After `end_survivors`, the writer and the readers are joined until a shared deadline one second away; a reader still blocked is left behind, and what it had read (kept in a buffer the run shares with it) is the run's output, judged by the exit status as before. A reader left behind ends when the descendant closes the pipe.
- rejected: Closing the pipes from the host to wake the readers: a blocked `read` on Unix is not woken by closing another handle, and the readers own their ends.
- blast:    `uze-core::machine::extension_program`.

## D123 — One call of a script asks the host at most 16 times and waits at most 60 s

- status:   pending
- task:     review finding
- commit:
- trigger:  Time spent in `run_program` and `fetch` is taken off the script's deadline, and nothing bounded how many it made, so one answer could hold a pool lane for hours.
- decided:  `CallClock` counts the host calls of the call running now; the 17th, or any made once 60 s were spent waiting on the host, fails naming the bound. Both reset with the call (`restart`), so a resident session gets them per call. The ceiling is checked before a call: one program or request still runs to its own limit.
- rejected: Cutting a call short at the ceiling: a program is ended only by its runner's own time limit.
- blast:    `uze-script::engine`.

## D124 — A general runner is known by a wider list, without case or `.exe`

- status:   pending
- task:     review finding
- commit:
- trigger:  `is_general_runner` missed `ksh`, `awk`, `busybox`, `pwsh`, `cmd`, `java`, `npm`, `cargo`, `go`, `uv`, `find` and others, and `Bash.exe` or `PWSH` passed as ordinary programs, so consent did not warn that the grant runs anything.
- decided:  The list names shells and argument-forwarders, interpreters, package managers and builders, and what runs a program elsewhere or as someone else; the name is lower-cased and a trailing `.exe` removed before comparing, and the versioned families (`python3.12`, `tclsh8.6`) widen to `pypy`, `tclsh`, `wish`, `java`. The list stays a warning, never a refusal, so naming too much costs only a louder consent.
- rejected: Recognising a runner by what it does at run time: the consent screen speaks before anything runs.
- blast:    `uze-core::capability::extension::program`; consent's `runs_anything`.

## D125 — At most four zero-width characters follow a base, and a cell costs at most 16 bytes

- status:   pending
- task:     review finding
- commit:
- trigger:  Only combining marks of five blocks were capped; any other character that takes no cell (Hebrew points, Arabic harakat, variation selectors) stacked without bound and added no width, so a `Clean` string was unbounded in bytes.
- decided:  Every character that takes no cell counts towards a run that a base character resets; past four in a run the rest are dropped, and the five blocks keep their own cap of two. The output also stops before passing `max × 16` bytes (a four-byte base and three four-byte marks per cell).
- rejected: Dropping every zero-width character outside the five blocks: scripts such as Hebrew and Arabic need them.
- blast:    `uze-contract::clean`.

## D126 — A process's first program run sweeps the scratch directories killed runs left

- status:   pending
- task:     review finding
- commit:
- trigger:  `runtime/extensions/scratch/<run>` is removed by the run that made it, so one is left only when UZE itself was killed mid-run; `runtime/extensions` is a kept tenant, so nothing ever reclaimed it.
- decided:  `UzeHome::extension_scratch_root` names the directory; the first run of each process (a `Once`, in `uze-application`'s program runner) removes every run directory beneath it not changed for an hour, far past the 10 s a program may run. A living process removes its own runs', so once per process is enough.
- rejected: Naming `scratch` a swept tenant of `runtime/`: it lives beneath `extensions`, a kept tenant, and a sweep by `doctor` could take the directory of a run in flight in another process.
- blast:    `uze-core::machine::home`, `uze-application::extensions::program`.

## D140 — A source with a question out is not asked again by its `every` or an event window

- status:   pending
- task:     review finding
- commit:
- trigger:  A refresh from `every` or a closed event window forgot the question still out, and `absorb` drops every answer that is not the latest: a source answering more slowly than it is re-asked never showed data.
- decided:  `pass` leaves a due source due while its question is out (`question_out`: asked, and that number not yet answered), and asks it on the first pass after the answer lands, as `deliver each` already waited on `awaiting`.
- rejected: Taking any answer newer than the shown one: two questions about different inputs would race to the frame.
- blast:    `declared.rs` (`pass`, `question_out`); `declared/event_tests.rs`.

## D141 — An action on rows answered for another checkout is refused until the new answer lands

- status:   pending
- task:     review finding
- commit:
- trigger:  After the checkout in front changed, a section kept drawing the old checkout's rows until its new answer; an action pressed meanwhile resolved its item from the old rows and ran on the new checkout.
- decided:  The engine remembers the checkout each source's data was answered about (`answered_for`), and an action pressed on an item, on a surface whose data was answered about another checkout than the one the action would run on, is refused with "the rows are still the previous checkout's". The old rows stay drawn: the operator sees the frame they had, and the action works again once the new answer lands. Data no checkout answered (a fixture, events, agents) is never refused.
- rejected: Dropping the data when the checkout changes: every switch would flash "Waiting for data" on every section, and a view open on its own checkout would have to be told apart.
- blast:    `declared.rs` (`answered_for`, `absorb`, `set_data`); `declared/action.rs` (`resolve`, `rows_of_another_checkout`); `declared/action_tests.rs`.

## D142 — A source's input reads the item the operator chose, taken when the selection moves

- status:   pending
- task:     review finding
- commit:
- trigger:  A source whose `input` reads `{item…}` read the selected row of its own list at each frame; the row index survives answers, so an answer renaming that row fed the next question, which asked again without end.
- decided:  The engine keeps the item of each surface's selected row as it was drawn when the selection moved to that row (`chosen`), taken before a source is asked, and the input reads that. An answer redrawing the row under the same index changes nothing; a new row chosen reads the row as drawn then. Rows still answered about another checkout (D141) give no item. An action pressed still reads the row drawn now.
- rejected: Refusing, in the check, an input that reads the item of its own source's list: a surface shows only its own source, so that refuses every input reading an item, the master-detail shape the input exists for.
- blast:    `declared.rs` (`chosen`, `capture_selection`, `input_of`, `jobs`, `open`, `close`, `replace`); `declared/source_tests.rs`.

## D143 — Two extensions' doors live everywhere on one chord: the first table keeps it

- status:   pending
- task:     review finding
- commit:
- trigger:  `bind_table` checked a chord against the actions of one table only, so two extensions offering an EVERYWHERE door on the same chord both kept it, and `everywhere_at` answered whichever owner sorts first, silently.
- decided:  `with_extensions` checks each EVERYWHERE door against the doors earlier tables bound; the later one is offered without a key, and `extension_problems` says which chord opens both and which is dropped. "First" is the order the tables are registered in.
- rejected: Dropping both: an extension that worked would lose its door when another is installed.
- blast:    `uze-keys` `keymap.rs` (`with_extensions`); `extension.rs` tests.

## D144 — The inspector keeps the newest 64 gestures

- status:   pending
- task:     review finding
- commit:
- trigger:  `recorded` grew by one entry per press, each with its whole item, for the life of the workspace (and was carried across rebuilds by `memory`).
- decided:  `RECORDED_MAX` = 64, as the runner keeps the last 64 questions it answered per extension; the oldest go first on a press and on restoring a memory.
- rejected: A byte budget: the items are already bounded by the data a source may answer, and a count is what the inspector shows.
- blast:    `declared/action.rs` (`RECORDED_MAX`, `act_with`); `declared.rs` (`restore_memory`); `declared/action_tests.rs`.

## D145 — Only the surfaces in view at the last pass hold pictures

- status:   pending
- task:     review finding
- commit:
- trigger:  The pictures wanted were gathered from every memo, and a memo was dropped only when a view closed, so a folded section, a closed drawer, a band or an overlay out of view kept its pictures held against the 32 MiB budget.
- decided:  The engine keeps the surfaces of the last `Pass` (`in_view`). Before asking for pictures, `jobs` lets go of every build out of view that names a picture, and gathers the wanted pictures from the builds left, so the shelf releases what no surface in view draws. A build out of view that names no picture is kept, so a section drawn folded every frame is not rebuilt each pass. Before any pass (a host that never passes, tests) every build counts as in view.
- rejected: Dropping every memo out of view: a folded section's header is drawn each frame and would be rebuilt after every pass for nothing.
- blast:    `declared.rs` (`in_view`, `pass`, `jobs`, `pictures_in_view`); `declared/media_tests.rs`.

## D146 — A picture read carries the shelf's epoch, and one from before a clear is dropped

- status:   pending
- task:     review finding
- commit:
- trigger:  `Shelf::clear` (on `replace`) forgot the read in flight, but its answer could land after a new read of the same key started, and was taken as the new one's: the old compile's picture drawn for the new.
- decided:  The shelf has an `epoch` that only `clear` bumps; each read's job carries the epoch it was asked in, and `absorb` drops an answer from another epoch before matching the key. The new read stays out and its own answer is taken.
- rejected: Reusing `generation`: it moves on every absorb and refusal, so a read would be dropped for a change that did not forget it.
- blast:    `declared/media.rs` (`Shelf::{epoch, clear, next, absorb}`, `MediaAnswer`, `read`); `declared/media_tests.rs`.
## D180 — The pause key is looked up before anything in front, never bound in `Global`

- status:   pending
- task:     review finding 3 (F8 over an overlay)
- commit:
- trigger:  F8 was bound in `Scope::Surfaces`, which a declared overlay's sealing `Scope::Declared` and every host question cut off, and `perform` routed the overlay before the pause; over an overlay, resident or plain, F8 did nothing.
- decided:  The workspace client resolves every chord against `[Global, Workspace, Surfaces]` first, as if nothing stood in front, and a chord meaning `PauseAgentWrites` is performed before the resident surface, the docked drawer or any scope routing; `perform` handles the pause in its first block beside Quit. The binding stays in `Surfaces`, so a keys.json rebinding still moves it. A question open when F8 is pressed stays open; only the writes are held.
- rejected: Binding F8 in `Scope::Global`: Global is live in management too, whose index would offer a pause that does nothing there. A scope of its own pushed over every seal: one action does not need a new keyboard.
- blast:    `src/ui/orchestrator/session/keys.rs`; `tests/declared_resident.rs`.

## D181 — An unattended allowance is given to one action, not to the extension

- status:   pending
- task:     review finding 4 (SECURITY)
- commit:
- trigger:  The 24-hour allowance to prompt an agent unattended was keyed by (extension, agent): once one `when=` prompt was allowed, every other auto-prompt of that extension, whatever its text, was typed unseen.
- decided:  `AgentRequest` carries the id of the action that asks, and the allowance is keyed by (extension, agent, action id). The question names the action ("Only <label>, to <agent>, on its events…"). The allowance lives only in memory (extension-agent-bridge D16), so no persisted format changes.
- rejected: Keying by the prompt's text: an action whose text is templated would ask on every event. Keying by the label: two actions may share one.
- blast:    `uze-extensions` `contract::AgentRequest` (new `action` field; `Outcome::Agent` now boxes it, which the larger request needs under clippy's `result_large_err`) and `declared/action.rs`; `src/ui/orchestrator/declared/agent.rs` and the five places that hand a request to `ask_of_agent`.

## D182 — A prompt is at most 2048 bytes, and is allowed only once the frame has shown it whole

- status:   pending
- task:     review finding 5 (SECURITY)
- commit:
- trigger:  An attended prompt had no length bound and the question's dialog clips at the frame's height, so the operator could allow text they never saw.
- decided:  Every prompt (attended or not) over 2048 bytes is refused with an attributed notice before anything is shown. The question is not made scrollable: `dialog::fits` says whether the frame shows the dialog whole, the question records it on every frame, and while it does not, the dialog says how long the prompt is and to make the terminal taller, offers no way to allow, and an allow (key or call) leaves it open. Esc still declines; once the frame has room the text appears whole and may be allowed.
- rejected: A scrollable body: an allow reachable before the end was scrolled to is the same hole, and proving the end was read needs state the widget does not keep. Refusing outright at ask time: the frame's height is not known there, and a resize makes a refused prompt readable.
- blast:    `src/ui/orchestrator/declared/agent.rs`; `src/ui/widget/dialog.rs` (`fits`, the layout split out of `render_with`).

## D183 — A skill an extension types is a plugin's, named `<plugin>:<skill>`; a harness's built-in commands are out of reach

- status:   pending
- task:     review finding 6 (SECURITY)
- commit:
- trigger:  `agent-prompt skill=` skipped the command-character check (rightly, since the harness's skill prefix leads the typed text) and nothing checked the name, so `skill="clear"` typed `/clear`, a harness's own command.
- decided:  Built-in harness commands are not skills, and an extension cannot reach them. A skill is named as plugins name theirs, `<plugin>:<skill>`, each part ASCII letters, digits, `-` and `_` (`uze_contract::manifest::is_skill_name`). The manifest decoder refuses any other name, and the host refuses it again before showing the prompt, since a request reaches it from an engine and not only from a decoded manifest.
- rejected: A list of each harness's built-in commands to refuse: it goes stale with every release and names harnesses outside `uze-integrations`. Allowing an un-namespaced user skill: it cannot be told apart from a built-in command.
- blast:    `uze-contract` (`manifest::is_skill_name`, the generated reference's spelling), `uze-core`'s manifest decoder, `src/ui/orchestrator/declared/agent.rs`, the extension skill's docs.
