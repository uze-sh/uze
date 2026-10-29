<!-- Decisions taken while implementing, one entry each, judged at archive. -->

## D1 — Change 6 is split in two: slots and feeds first, the agent bridge second

- status:   pending
- task:     (planning)
- commit:
- trigger:  `extension-platform`'s *Proposed split* puts every slot, compose, notify, preferences, delivered events and the whole agent bridge in one change 6 named `extension-agent-bridge`.
- decided:  `extension-slots-and-feeds` takes what needs no agent (the `status`, `band`, `overlay` and `notify` slots, compose, `notify`/`sound` and `uze_platform::notify`, preferences, `deliver: each` and the `events` source, host-animated nodes). `extension-agent-bridge` keeps `uze agent report` and its journal, the `agent.*` events, the `agents` source, `agent.prompt`/`agent.restart` with their rates, `focus_agent`, global keys, the ADR-056/040/059 amendments, and the gallery entries that need them (`agents-hud`, `flightdeck`, `chimes`, `jump`, `plan-review`, `gsd-band`'s next step).
- rejected: One change: it would amend three ADRs and every integration in the same review as the sidebar's header.
- blast:    The parent's *Proposed split* reads as 6a and 6b; tasks 13.x, 14.10, 14.11 and 15.3 land here, 14.1–14.9 there.

## D2 — Preferences reach the engine through `Extension::configure`

- status:   pending
- task:     1.3
- commit:
- trigger:  `{pref.*}` is resolved where the engine binds a view, which has no way to ask for the operator's settings: data reaches it only as answers to its questions.
- decided:  `contract::Extension` gains `configure(&mut self, preferences: &Value)` (a default that does nothing), which the host calls when it reads the preferences and when the operator changes one. The declared engine admits each value against its declaration (`PreferenceDecl::admit`, default on anything it may not hold) and bumps its generation, so every memoized build is drawn again. Bindings gain the scope `pref` (`{pref.warn-at}`).
- rejected: Folding preferences into each source's data: every source would carry them, and a change would mean asking every source again.
- blast:    `uze-extensions::contract`, `declared.rs`, `declared/value.rs`.

## D3 — A reference to an undeclared preference reads nothing, as an absent field does

- status:   pending
- task:     1.2
- commit:
- trigger:  Task 1.2 asks the check to refuse `{pref.<name>}` naming no declared preference.
- decided:  Not refused yet: a view's references are checked against fixtures by the check, and `pref` joins `ui` and `context`, which are not either. An undeclared `{pref.x}` binds nothing, and a lone reference hides its node, by D23's rule.
- rejected: A separate walk of every compiled view's bindings for `pref` only.
- blast:    A typo in a preference's name shows as a missing value; the LSP lists the declared names (task 2.3 adds them).

## D4 — A `status` row and a `band` draw no attribution strip of their own

- status:   pending
- task:     1.1
- commit:
- trigger:  D14 keeps the host's attribution strip on `view`, `drawer` and `overlay`; a one-row status and a band of one to six rows would lose half their room to it.
- decided:  A status row is drawn in the sidebar's header under the host's own order and named on the Extensions screen; a band carries its extension's id in the host's chrome along its edge (task 4.2), never a full strip. `overlay` keeps the strip like a view. The test harness draws status and band into their rows as the workspace does.
- rejected: A strip on every slot: a 1-row band would be all strip.
- blast:    `src/ui/extension_test.rs`; the band's edge mark is task 4.2's.

## D5 — Preferences are set in an editor the drawer's Preferences button opens

- status:   pending
- task:     2.3
- commit:
- trigger:  S7 has the drawer's rows edited by Enter and click, but a drawer's rows are not focusable: its buttons are offers, reached by their actions.
- decided:  The drawer lists each preference (`Show °F · off`). `Action::EditPreferences` ("Preferences", no default key, a drawer button and an index row) opens `Overlay::Preferences`: a menu of `label  value` rows, the arrows move, Enter flips a toggle or moves a choice on at once, and opens the host's text prompt for a text or a number (Esc puts the prompt away, then the editor). A click on a row is choosing it and pressing Enter. Each answer is `Intent::SetPreference`, written off the frame through `Extensions::set_preference` (refused for a value the declaration does not hold), and the workspace reads it at its next discovery.
- rejected: Focusable rows inside the drawer: a second keyboard model for one screen.
- blast:    `src/ui/{model,overlay,hit,input,worker}.rs`, `uze-keys` action.

## D6 — Preferences are read with each discovery and handed to every engine

- status:   pending
- task:     2.2
- commit:
- trigger:  The engine needs the preferences before it draws, and reading the record is filesystem work the drawing thread may not do.
- decided:  The declared discovery job reads `Extensions::preferences` for each found extension beside its consent record (`ExtensionAnswer::preferences`); the host keeps them on `HostedExtension` and calls `configure` on a change and on every engine it builds. A source declaring `preferences` is asked with `input.preferences` as admitted now, which is also part of its question's digest. Setting a preference raises `plugins_changed`, so the next pass reads them again.
- rejected: A watch on the preferences file: one more poll for what changes only when the operator sets it, from this client.
- blast:    `src/ui/orchestrator/{reads,declared,answers}.rs`, `runner.rs`.

## D7 — A composed surface keeps its view file, drawn until its source answers

- status:   pending
- task:     3.1
- commit:
- trigger:  S5 builds a surface from the tree its source answers, while every surface still names `view=`, which the decoder requires.
- decided:  A surface whose source declares `output "view"` keeps its view file: it is what draws while the source has not answered (or is withheld), and the composed tree replaces it once it has. A tree that does not validate draws a notice naming the first wrong node by path. The tree becomes a compiled view of literals only, built by the same binder, so `{…}` in a composed string is text, never a reference.
- rejected: An optional `view=` for composed surfaces: a second surface shape for the check, the LSP and the docs to tell apart.
- blast:    `uze-extensions::declared::{compose, build}`.

## D8 — Composed nodes take only `grow` and `min-width` of the common properties

- status:   pending
- task:     3.1
- commit:
- trigger:  `COMMON_PROPS` includes `each`, `when`, `unless`, `show` and `show-any`, which bind against data.
- decided:  A composed node may carry `grow` and `min-width`; the binding ones are refused ("does not take"), since a composed tree already holds every value. Its `item` is accepted only beside an `action` naming a declared action. The node cap is the binder's, 2,000.
- rejected: Accepting `show`/`each` against the answer's own values: an expression language through the back door.
- blast:    `compose::COMPOSED_COMMON`.

## D9 — A click on a composed surface within 300 ms of a new tree is dropped, measured on the host's clock

- status:   pending
- task:     3.2
- commit:
- trigger:  D15 asks to ignore clicks within 300 ms of a re-layout; the engine draws through `&self` and does not know when a frame was laid out.
- decided:  The engine stamps a source's answer with the host's clock (`Pass::now`) whenever its digest changes, and a node hit on a surface of a composed source within 300 ms of that stamp is `Stay`. Keys are not held back: a key acts on the selection, not on what is under the pointer.
- rejected: Holding back every surface's clicks: a bound view's nodes do not move under the pointer the way a program's tree may.
- blast:    `DeclaredExtension::{set_data, just_laid_out, hit}`.

## D10 — An event rides with its question and is no part of what the question is remembered by

- status:   pending
- task:     6.1
- commit:
- trigger:  `input.event` changes the input, and the engine asks again whenever a source's input differs from its latest question's, so the next frame (without the event) would ask a second time.
- decided:  The event is added to the `Question` only, inside `ask`, when the question is a refresh and an event waits for that source; `Asked.input` keeps the declared input alone. An event is `{name, paths?, checkout}` plus `dropped` when a source delivered `each` missed some. No time is in it: the engine reads the host's clock and nothing else.
- rejected: Keying the session cache by the event: every save would be a new question for the runner's memory too.
- blast:    `DeclaredExtension::ask`, `events::event_value`.

## D11 — `deliver "each"` queues 16 events per window and asks again once each answer is in

- status:   pending
- task:     6.1
- commit:
- trigger:  S8 runs a source once per event, bounded per window, without saying how the runs are paced.
- decided:  `events::Delivery` queues at most `EACH_PER_WINDOW` (16) events per source and counts the rest as `dropped`, said on the next event handed over. When the window closes the source is asked with the first; each later pass asks it again with the next once its last question was answered (never two out at once) and no new window is open for it.
- rejected: One run with every event as a list: `deliver "each"` is for programs that handle one at a time.
- blast:    `declared/events.rs`, `DeclaredExtension::{pass, awaiting}`.

## D12 — An `events` source is answered by the engine, from every event it saw

- status:   pending
- task:     6.2
- commit:
- trigger:  The design keeps the last events "in memory for the session", and only the engine sees an extension's events; the runner never does.
- decided:  The engine logs every event the host hands it (newest first, at most 200) and answers each `events <n>` source with the last `n` itself, as data, whenever the log moves and when it is built; such a source is never a job, so the host is never asked about one. The runner refuses one as "answered by the workspace" if asked.
- rejected: A host function or a store-backed log: a log the operator did not ask to keep would outlive the session.
- blast:    `DeclaredExtension::{gather, answer_event_sources, jobs}`.

## D13 — The slots are proven by a fixture plugin of their own, `deck`

- status:   pending
- task:     7.1
- commit:
- trigger:  Task 7.1 gives `gsd-band` a composed band and `notes` an overlay; `gsd-band`'s fixture backs dozens of tests and snapshots of change 1, and `notes` lives only in the hand proof.
- decided:  `tests/_fixtures/canonical/deck` holds one extension exercising this change whole: a 2-row top `band` composed by `phases.rhai` (one `Segment` per phase directory, weighted by its plans and filled by its summaries) whose label reads a `toggle` preference, a `status` row from the same source, a `view` over an `events 20` source with an `open` action to a `picker` `overlay` and a `done` action whose script answers `notify` and `sound`. `gpu-meter` gains a `status` row and a `warn-at` preference its script reads from `input.preferences`. The packages gate checks `deck` with `gpu` and `gsd`. A check rehearses with each preference at its default.
- rejected: Growing `gsd-band`: every snapshot of change 1 would be redrawn for a change that is not about it.
- blast:    Two fixture plugins; `runner::rehearse_over`.

## D20 — The status order is kept per extension, in the management section of the client layout

- status:   pending
- task:     4.1
- commit:
- trigger:  S2 keeps the operator's order of status rows in the client's remembered layout and moves them from the Extensions screen, while the rows are drawn by the workspace.
- decided:  `ManagementLayout::status_order` holds the switch keys (`id · plugin@marketplace`) of extensions, not surfaces: an extension's rows move together, in the order it declares them. The Extensions screen owns and edits it; the workspace reads the modal's copy while it is open (a move is drawn behind it at once) and the last one closed otherwise, and the layout recorder persists it with the rest of the modal's section. An extension the order does not name stands after those it does, in identity order, which both sides use as discovery order (`src/ui/status_order.rs`).
- rejected: A list per surface in `WorkspaceLayout`: Move up and Move down act on the selected extension, which may declare several rows, and the workspace section would need an intent crossing from the modal for every move.
- blast:    `uze-workspace::client_layout`, `src/ui/{status_order,model,input}.rs`, `orchestrator/declared/status.rs`.

## D21 — Status rows stand under the header's rule, not above it

- status:   pending
- task:     4.1
- commit:
- trigger:  S2 puts the rows "in the sidebar's header", whose hairline lands on the row the tab strip's own rule does.
- decided:  The rows are drawn right under the rule, before the first space's card, inset one column as the header's name is and short of the column's trailing pad.
- rejected: Rows between the header and its rule: every row would push the rule off the tab strip's, and the two columns would stop sharing one divider.
- blast:    `orchestrator/render/{sidebar,declared}.rs`.

## D22 — A status row and a band draw only a tree that has something in it; a row is folded by cutting every text

- status:   pending
- task:     4.1, 4.2
- commit:
- trigger:  S2 frees the row of an empty answer and folds a long one; an engine also answers `Waiting for data`, a refusal or a failure as a message.
- decided:  Only a `Content::Tree` with a visible node is drawn (`drawn_tree`): an empty stack, blank text, a waiting, a withheld or a failed answer take no status row and leave a band's rows blank (the Extensions screen and the inspector say why). A status view has `truncate` set on every `Text` before it is drawn into its one row, so the tree renderer cuts it with an ellipsis rather than wrapping it out of sight.
- rejected: Drawing the engine's message in the row: a sidebar header that says `Waiting for data` for every extension on every switch of checkout.
- blast:    `orchestrator/declared/status.rs`.

## D23 — A band is laid over the agent's own tab, judged enabled from the record the last discovery read

- status:   pending
- task:     4.2
- commit:
- trigger:  S3 draws a band on every agent pane whose checkout's project has the extension enabled, and `compute_layout` must know it without asking the filesystem.
- decided:  The rows are reserved while the tab selected is the agent whose context is in front (`agent_tab_in_front`), never one of its shells, and the extension is switched on and enabled in that agent's project: always for one that runs nothing (`uze_application::enabled_per_project`, kept on `HostedExtension::per_project`), else when the record's `enabled_in` holds the project as the session placed it, compared as written. Only the focused pane is ever drawn and sized, so its band always draws the engine's answer about the checkout in front. Bands stack in slot order, top ones down and bottom ones up, and one that would leave the pane fewer than 6 rows is not laid out.
- rejected: Canonicalizing the project per frame: a filesystem call on the drawing thread. Laying bands over shells: a shell is no agent's pane.
- blast:    `orchestrator/declared/band.rs`, `orchestrator/render.rs::{Reserved, take_bands}`, `orchestrator/agents.rs`.

## D24 — A band keeps its rows while a view or the overview stands in the pane

- status:   pending
- task:     4.2
- commit:
- trigger:  A view and the overview are drawn in `layout.pane`, which is now the band-less rect the terminal is sized to.
- decided:  The band stays drawn above (or below) whatever stands in the pane; the view takes the rest. Opening a view never changes the band's reservation, so it never resizes the agent behind it either, and every surface keeps measuring clicks against the one `layout.pane`.
- rejected: Giving a view the band's rows back: a second agent-area rect for every view, overview and hit path to agree on.
- blast:    `orchestrator/render.rs`.

## D25 — The band's id mark keeps a column the band's whole height

- status:   pending
- task:     4.2
- commit:
- trigger:  D4 has the host draw the extension's id at the end of the band's first row, where nothing it draws can cover it.
- decided:  The id (`identity.id`, muted, cut at half the band's width) and one cell of air are taken off the right of every row of the band, and the band's view is drawn in the rect left; its first row ends in the id.
- rejected: Narrowing the first row alone: the view is one rect, and a tree laid out in a ragged one is not something the renderer knows.
- blast:    `orchestrator/render/declared.rs::render_band`.

## D26 — An overlay is modal through its own slot field, closed by a bare mark rather than a footer Close

- status:   pending
- task:     4.3
- commit:
- trigger:  S4 makes an overlay modal over whatever is open, a view included, and the design system closes a modal with a bare `×` and Esc, naming no close in its foot.
- decided:  `WorkspaceModel::open_overlay` holds its slot beside `open_slot`: while it stands the key scopes stop at `Declared` (its declared actions and the moves every declared surface answers), `chrome_answers` is false, and a press is its own, its mark's, a footer key's, or a click outside that closes it. The engine's `Close` is dropped from its footer. An `open` action reaches it as the engine's `Outcome::Show`, and `show_surface` opens an overlay or, for a view, puts the overlay away first. The index offers it as an `open:<surface>` door, as a view's. Esc and a click outside leave everything behind it as it was, which is what restores the focus.
- rejected: Opening an overlay in the pane's slot: a view could not stand under it.
- blast:    `orchestrator/declared/{overlay,doors}.rs`, `orchestrator/session/{keys,mouse,surface}.rs`, `orchestrator/render/declared.rs::render_overlay`.

## D27 — Move up and Move down are the existing list actions, offered as drawer buttons

- status:   pending
- task:     4.1
- commit:
- trigger:  S2 asks for Move up and Move down on the Extensions screen.
- decided:  `Action::MoveItemUp` and `MoveItemDown` ("Move the entry in focus up its list") are offered in the drawer of an extension that declares a status row (`offers::status_order_offers`), unavailable at either end, and answered on the Extensions route. They hold no key there: the overview's `shift+up`/`shift+down` stay its own, and the buttons and the index reach them.
- rejected: Two new actions for one list: the vocabulary already says it.
- blast:    `uze-application::offers`, `src/ui/{model,input}.rs`, `tests/architecture/affordances.rs`.

## D28 — A consent step names the rows all of an extension's bands take

- status:   pending
- task:     4.4
- commit:
- trigger:  The risk in design.md lists a band's cost on the consent screen as "draws N rows over each agent's pane".
- decided:  `ExtensionReport::band_rows` is the sum of its bands' rows, `None` without one, and both consent steps (the Extensions screen's and the workspace's offer) add the line after the grants. An extension that runs nothing asks no consent, so its band is said only on the Extensions screen's surface list (`· band, N rows`).
- rejected: A consent step for a fixtures-only extension with a band: consent is about what runs, and a band runs nothing.
- blast:    `uze-application::extensions::report`, `src/ui/{overlay,model}.rs`, `orchestrator/extension_offer.rs`.

## D30 — The declared engine gates and rates a notification as the answer lands; the host takes it only under `notify`

- status:   pending
- task:     5.2
- commit:   b82150b7
- trigger:  S6 has the host apply `notify` and `sound` under the grant and within a rate, and counts the excess in the surface's notice, which only the engine keeps.
- decided:  `DeclaredExtension::apply` hands the envelope to `ask_to_notify`: without `notify` in the manifest it says so in the notice of the surfaces the answer was about; with it, `Rate::admit` lets it through on the host's clock of the last pass (one every 10 s, 30 in any hour), else counts it. What goes through waits for `Extension::notifications`, a new method with a default that hands nothing. The frame loop takes them after the clipboard (`take_notifications`), dropping whatever an engine whose `Hosted::may_notify` is false asked, and sends each through `send_notification`. Neither the script's envelope decoder nor the manifest check refuses `notify` without the grant: it is enforced here, twice. A request and its cue count as one against the rate; with no clock yet, nothing is counted as sent, so nothing goes.
- rejected: A rate kept by the host per owner: the count belongs in a notice only the engine holds, and a second contract method would carry it back.
- blast:    `uze-extensions::{contract::Notify, declared/notify.rs}`, `orchestrator/declared/notify.rs`, `orchestrator.rs` (the frame loop).

## D31 — A notification reads `id · plugin@marketplace: title`, and a cue is one bell

- status:   pending
- task:     5.2
- commit:   b82150b7
- trigger:  S6 asks the title to begin with the extension's attribution and every cue to be the bell.
- decided:  The title is led as every third party's word is (`attributed`): the attribution, a colon, the extension's title, or the attribution alone when the title is empty. `uze_platform::notify::route` decides the way out with `TERM_PROGRAM` read once before the loop: the desktop's tool is spawned on a thread of its own with nothing reaching the terminal, OSC 9 or the bell is written between frames. A cue rings the bell unless the notification already fell back to it, so one answer never rings twice.
- rejected: Distinct sounds per cue: the host plays no audio file (S6, D20).
- blast:    `orchestrator/declared/notify.rs`.

## D32 — What was dropped is counted since the last one sent, after the answer's own notice

- status:   pending
- task:     5.2
- commit:   b82150b7
- trigger:  S6 counts the excess in the surface's notice, and the answer may carry a notice of its own.
- decided:  The notice says `N notifications dropped: at most 1 every 10 s and 30 an hour`, counting from the last notification that went out, in the warning role; an answer's own notice stays first, ` · ` before the count. A request without the grant says `not notified: needs the notify grant`. An answer that asks nothing clears it, as it clears any notice.
- rejected: A toast per drop: a burst would be a flood of toasts, which the design system forbids.
- blast:    `uze-extensions::declared/notify.rs`.

## D33 — A segmented `Progress` is drawn as answered; what moves on its own is `Activity` and `Elapsed`

- status:   pending
- task:     6.3
- commit:
- trigger:  Parent 13.5 and the design speak of segmented `progress` transitions driven by the host's clock; the renderer keeps nothing between frames.
- decided:  No transition is added. A bar moves when its answer does, and the test proves it holds still while the activity mark and the elapsed time move, with no run between two frames, and that such a frame asks the loop for the next one (`marquee`). The renderer already moved both from the clock it is handed; the test drives the whole client's frame with that clock stopped at a chosen instant (`tree::stopped_at`, test-only), on its own thread.
- rejected: Easing a bar from its last fill to its new one: state across frames that the renderer refuses by design, or a wall clock in the engine, for a motion of a fraction of a second.
- blast:    `src/ui/extension_view/tree.rs`, `orchestrator/tests/declared_motion.rs`.

## D40 — A source that reads the preferences is asked again when one changes, by the engine forgetting its question

- status:   pending
- task:     7.3
- commit:   fcbf6259
- trigger:  The slots journey turned `deck`'s `detail` off and the band and the status row kept naming the phases: the runner adds `input.preferences` as it asks (D6), so the question the engine remembers (`asked`) never changed with a setting, and nothing asked again.
- decided:  `DeclaredExtension::configure`, when the admitted preferences change, forgets the question of every source declaring `preferences`; the next pass asks it, and the runner hands the new setting. A source that does not read them is not asked again.
- rejected: Putting the preferences into the engine's own input of the question: they would ride twice, once from the engine and once from the runner, which reads the record as written.
- blast:    `uze-extensions::declared.rs` (`configure`), `declared/source_tests.rs`.

## D41 — The slots journey proves `deck`'s band, row and toggle; a number preference and the overlay and notification are not in it

- status:   pending
- task:     7.3
- commit:
- trigger:  Task 7.3 names a band, a status row and a preference; `gpu` carries the number preference but answers from `nvidia-smi`, which no journey world has.
- decided:  `journeys/suites/04-workspace/18-…` stages `deck` (a copy of the fixture: a container mounts only `journeys/`), enables it from the offer, gates on the band over the agent and the row in the sidebar's header naming the phases, turns `detail` off in the Extensions screen's preferences editor, and reads `state/extensions/preferences/deck@studio/deck.json` and the checkout the script never wrote. What the band and the row draw is a gate, never a check: nothing on disk says it. A number preference typed through the host's prompt, the picker overlay and the `done` notification are left to the client's own tests.
- rejected: Pointing the journey at `tests/_fixtures/canonical/deck` through `{repo}`: the Docker run cannot reach it, so the copy can drift from the fixture, which the extension check still proves.
- blast:    `journeys/suites/04-workspace/18-a-band-and-a-status-row-a-preference-redraws.yml`.

## D42 — Closed on make check and the hand proof's automatic part; the TUI checklist waits for the operator

- status:   pending
- task:     7.4
- commit:
- trigger:  The task asks for `make check`, `openspec validate --all --strict` and the hand proof script.
- decided:  `make check` passes on the branch rebased onto `main` (`f5a7972e`) but for the three failures known to this machine's environment (opencode's `bun` shim, two `foreground_status_*` in `uze-terminal`); `openspec validate --all --strict` passes. The proof script (`run6.sh`, kept with the plan's artifact, shared by 6a and 6b) rebuilds a world from the branch's `deck` and `bridge` fixtures; its automatic part passes (install disabled, nothing allowed unasked, both plugins check clean, a report from outside an agent refused with exit 0). Its checklist a–i in the real TUI is the operator's.
- rejected: Waiting for the operator to close the task: the run is unattended.
- blast:    None.

## D43 — Found driving the TUI by hand: a script action's waiting notice outlived its answer

- status:   pending
- task:     7.4
- commit:
- trigger:  Driving `run6.sh` item i in the real TUI: "Say done…" stayed in the view's foot after the action had run (its bell rang and a second press was counted as dropped), because the host kept the notice the gesture returned in `open_notice`, which only the engine's own notices clear.
- decided:  An answer from the extension standing in the pane clears the notice its gesture left, unless the answer is itself a notice. `an_answered_actions_script_is_handed_to_the_host` holds that the script is asked at all.
- rejected: Keying the waiting notice to the action: one more piece of state for a notice that any answer supersedes.
- blast:    `src/ui/orchestrator/answers.rs::absorb_engine_answer`.
