<!-- Decisions taken while implementing, one entry each, judged at archive. -->

## D1 — Change 6 is split in two: slots and feeds first, the agent bridge second

- status:   pending
- task:     (planning)
- commit:
- trigger:  `add-extension-platform`'s *Proposed split* puts every slot, compose, notify, preferences, delivered events and the whole agent bridge in one change 6 named `agent-bridge`.
- decided:  `extension-slots-and-feeds` takes what needs no agent (the `status`, `band`, `overlay` and `notify` slots, compose, `notify`/`sound` and `uze_platform::notify`, preferences, `deliver: each` and the `events` source, host-animated nodes). `agent-bridge` keeps `uze agent report` and its journal, the `agent.*` events, the `agents` source, `agent.prompt`/`agent.restart` with their rates, `focus_agent`, global keys, the ADR-056/040/059 amendments, and the gallery entries that need them (`agents-hud`, `flightdeck`, `chimes`, `jump`, `plan-review`, `gsd-band`'s next step).
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
