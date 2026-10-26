<!-- Decisions taken while implementing, one entry each, judged at archive. -->

## D1 — An action's scalars are properties, what it does is its one child

- status:   pending
- task:     1.2
- commit:   cf2bf3a2
- trigger:  D22 fixes the nodes and the kebab-case of properties but not where an action's `label`, `on`, `key` and `confirm` go, and the specs write `confirm "…"` and `key "d"` loosely.
- decided:  `action "<id>" label="…" on="<surface>" key="…" confirm="…" { <does> }`, the block holding exactly one of `script "x.rhai"`, `refresh "<source>" …`, `clipboard "<template>"`, `open-path "<template>"` or `open "<surface>"`. A source's parts are child nodes: one of `script`/`run`/`fixture`, `input name="<template>" …` (properties, text only), `every "<n><ms|s|m|h>"`, and one `refresh-on "<event>" <paths…>` per event. `wants "<grant>" …` is written once, as arguments.
- rejected: Child nodes for an action's scalars (`label "…"`): one line per action reads as the operator's examples do, and a surface is already written that way.
- blast:    The LSP's manifest completions and the scaffold follow this spelling.

## D2 — A suggested key is spelled `ctrl-r`, and any unmodified character is bare

- status:   pending
- task:     1.3
- commit:   cf2bf3a2
- trigger:  The design and the specs spell a manifest's key `ctrl-r`/`ctrl-q`, while `keys.json` and `uze_keys::Chord::parse` join with `+`; and the check must refuse a "bare-letter" key on a `confirm` action.
- decided:  `uze_contract::KeySuggestion` reads lowercase modifiers joined to the key by `-` (`ctrl-shift-r`, `f5`, `ctrl--`), the key one printable ASCII character or a named key. A confirmed action refuses any single character without a modifier (a digit or `/` as much as a letter). Step 3 converts a suggestion into a `Chord`; uze-contract may not depend on uze-keys.
- rejected: The `+` spelling of `keys.json`: the manifest examples already use `-`. Refusing only letters: a stray `1` is as easy to press as a stray `d`.
- blast:    A manifest written with `ctrl+r` is refused with the `-` spelling as the suggestion.

## D3 — A copy or an open-path action needs its grant in `wants`

- status:   pending
- task:     1.3
- commit:   cf2bf3a2
- trigger:  The task lists the grants accepted but not whether a declared `clipboard` or `open-path` action without the grant is refused, and a grant nothing uses would mean consent never covers what runs.
- decided:  The check refuses an action whose `does` is `clipboard` or `open-path` when `wants` lacks that grant, naming it (`Does::needs`). Whether a script calls a host function its grants do not register stays with step 4.
- rejected: Letting the runner refuse it at the gesture: the author learns it from the check instead of from a dead button.
- blast:    None today: no manifest declared actions before this change.

## D4 — Later nodes are named with their change, and `events`/`deliver` wait

- status:   pending
- task:     1.2
- commit:   cf2bf3a2
- trigger:  The *Proposed split* lists `deliver` and `events` in step 4.1, while this change's task 1.2 and its Non-Goals leave `deliver: each` and the `events` source to change 6. The grant is spelled `open_path` in D5 and `open-path` in this change.
- decided:  `preferences` (6), `deliver` (6), `output` (5) and `agent.*` events (6) are refused as "arrives in a later change (change N)"; `events` is an unknown node. `run` is decoded into `SourceKind::Run` and refused by the check (5). Later grants: `exec:*`/`store` 5, `agent:*`/`notify` 6, `resident` 7, `net`/`secret`/`edit:checkout` 8; `write:checkout` does not exist. The grant is `open-path`, kebab-case as D22 asks of every manifest word.
- rejected: Decoding `deliver` now with only `latest`: a property that can take one value is a promise this change does not keep.
- blast:    Change 6 replaces the refusal of `deliver` and adds `events`.
