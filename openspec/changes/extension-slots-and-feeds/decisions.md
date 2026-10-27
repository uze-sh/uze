<!-- Decisions taken while implementing, one entry each, judged at archive. -->

## D1 — Change 6 is split in two: slots and feeds first, the agent bridge second

- status:   pending
- task:     (planning)
- commit:
- trigger:  `add-extension-platform`'s *Proposed split* puts every slot, compose, notify, preferences, delivered events and the whole agent bridge in one change 6 named `agent-bridge`.
- decided:  `extension-slots-and-feeds` takes what needs no agent (the `status`, `band`, `overlay` and `notify` slots, compose, `notify`/`sound` and `uze_platform::notify`, preferences, `deliver: each` and the `events` source, host-animated nodes). `agent-bridge` keeps `uze agent report` and its journal, the `agent.*` events, the `agents` source, `agent.prompt`/`agent.restart` with their rates, `focus_agent`, global keys, the ADR-056/040/059 amendments, and the gallery entries that need them (`agents-hud`, `flightdeck`, `chimes`, `jump`, `plan-review`, `gsd-band`'s next step).
- rejected: One change: it would amend three ADRs and every integration in the same review as the sidebar's header.
- blast:    The parent's *Proposed split* reads as 6a and 6b; tasks 13.x, 14.10, 14.11 and 15.3 land here, 14.1–14.9 there.
