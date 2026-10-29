## Purpose

The settings an extension declares, which the operator edits and the
extension reads, kept on the machine as the operator's own answer.

## ADDED Requirements

### Requirement: An extension declares typed preferences
A manifest MAY declare `preferences` of kinds `text`, `number` (with
`min` and `max`), `toggle` and `choice` (with `values`), each with a
`label` and a `default`. A view SHALL read them as `{pref.<name>}`, and a
source SHALL receive them in `input.preferences` only when it declares
`preferences`.

#### Scenario: A threshold changes what the view draws
- **WHEN** the operator sets `gpu-meter`'s `warn-at` to 50
- **THEN** its gauge turns to the warning role at a load of 50% instead of 70%

### Requirement: The operator edits preferences on the Extensions screen
The Extensions drawer SHALL list each preference with its value; a key
and a click SHALL edit it the same way; the value SHALL be kept per
extension on the machine, and a value outside its declaration SHALL read
as the default.

#### Scenario: A toggle is flipped by key or by click
- **WHEN** the operator presses Enter on a `toggle` preference, or clicks it
- **THEN** its value flips and the surfaces that read it are drawn again
