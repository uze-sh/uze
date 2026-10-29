## Purpose

The state a declared extension keeps between answers: one bounded JSON
document per extension and project, patched by answers and read by sources
and scripts that ask for it.

## ADDED Requirements

### Requirement: An extension keeps one bounded store per project
An extension holding `store` SHALL have one JSON object per project, at most
1 MiB, kept in UZE's state and removed with its plugin. An extension holding
`store:global` SHALL have one for the machine instead, and that grant SHALL be
consented to on its own.

#### Scenario: The store is per project
- **WHEN** `gpu-meter` stores a history while enabled in two projects
- **THEN** each project's surface reads only its own history

#### Scenario: The store leaves with the plugin
- **WHEN** the operator removes the plugin with `uze remove gpu -m`
- **THEN** its stores are deleted

### Requirement: Answers patch the store in arrival order
Any answer's envelope MAY carry `store`, an object whose keys replace the
store's keys, with `null` removing one. Patches SHALL be applied in the order
answers arrive. A patch that would take the store over its bound SHALL be
refused, named in the surface's notice, and the rest of the answer SHALL
still apply.

#### Scenario: A source keeps a history
- **WHEN** a source answers `{"data": {...}, "store": {"history": [3, 5]}}`
- **THEN** the store's `history` becomes `[3, 5]` and the surface draws `data`

#### Scenario: An oversized patch is refused
- **WHEN** a patch would make the store larger than 1 MiB
- **THEN** the store is unchanged and the notice names the bound

### Requirement: The store reaches only who asks for it
A program SHALL receive the store in its input only when its declaration
names `store`. A script SHALL read it with `store()`, only when its extension
holds `store` or `store:global`.

#### Scenario: A script draws from its history
- **WHEN** a script holding `store` calls `store()` after a patch set `history` to `[3, 5]`
- **THEN** it receives `#{ history: [3, 5] }`
