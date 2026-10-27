## Purpose

A declared script that keeps its state while its surface is open, reads raw
input when focused and renders the frames it asks for, inside budgets.

## ADDED Requirements

### Requirement: A resident source keeps its script's scope while a surface shows it
A source declaring `resident "<file>"` under the `resident` grant SHALL
start the script (`start`) when a surface showing it comes into view, SHALL
keep its scope between calls, and SHALL stop it (`stop`) when no surface
shows it.

#### Scenario: A score survives between frames
- **WHEN** `snake` eats twice
- **THEN** its next render shows a score of 2, without the store

### Requirement: Renders are bounded and asked for
The host SHALL call `render` only after `start`, `event`, `input` answered a
redraw or a `timer` fired, at most once per drawn frame and 30 times a
second; a call past its operation bound or deadline SHALL stop the script,
and its surface SHALL name the bound; a late render SHALL keep the last
frame.

#### Scenario: A runaway loop is stopped
- **WHEN** a resident script's `render` loops forever
- **THEN** it is stopped at its bound, its surface says which, and the workspace keeps drawing

### Requirement: A focused resident surface reads raw input, never the host's chords
While a resident surface is focused it SHALL receive keys and pointer events
relative to itself, except `esc`, which unfocuses it, F1 and F8, which the
host keeps.

#### Scenario: Arrow keys steer, Esc leaves
- **WHEN** `snake` is focused and the operator presses `up`, then `esc`
- **THEN** the script receives `up`, and `esc` returns focus to the workspace
