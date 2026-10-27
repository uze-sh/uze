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
