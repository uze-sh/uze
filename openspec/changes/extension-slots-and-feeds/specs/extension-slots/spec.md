## Purpose

The places beyond `view`, `section` and `drawer` a declared extension may
occupy, and what the host keeps fixed about each so no answer can move the
workspace around it.

## ADDED Requirements

### Requirement: A status surface is one row of the sidebar's header
A surface with `slot="status"` SHALL draw one row in the sidebar's header
for the checkout in front, folded to the sidebar's width, never wrapped,
in the order the operator set on the Extensions screen. An empty answer
SHALL free its row.

#### Scenario: A meter in the header
- **WHEN** `gpu-meter`'s status surface is enabled and answers a load of 9%
- **THEN** the sidebar's header shows one row with the meter, beneath any status rows ordered before it

#### Scenario: The operator orders the rows
- **WHEN** the operator moves a status surface up on the Extensions screen
- **THEN** its row is drawn above the one it passed, and the order is kept across launches

### Requirement: A band reserves its rows before the pane is laid out
A surface with `slot="band"` SHALL declare 1 to 6 `rows` and `place` `top`
or `bottom`, and SHALL draw on every agent pane whose checkout's project
has the extension enabled. Its rows SHALL be taken from the pane before
the terminal's size is computed, so no answer resizes the agent's
terminal, and the pane's mouse and selection SHALL be offset by them.

#### Scenario: An answer never resizes the agent
- **WHEN** a 2-row band answers, then answers nothing, then answers again
- **THEN** the agent's terminal is sent no resize after the band first took its rows

#### Scenario: A click below the band lands where it was aimed
- **WHEN** the operator clicks the pane's first row beneath a 2-row top band
- **THEN** the agent's terminal receives the click on its own first row

### Requirement: An overlay opens over the frame on a gesture
A surface with `slot="overlay"` SHALL open only from an `open` action or
the action index, centered over the frame under the host's attribution
strip, modal, and SHALL close with Esc.

#### Scenario: A picker opens and closes
- **WHEN** the operator presses the action that opens the `notes` overlay, then Esc
- **THEN** the overlay is drawn attributed to `notes · proof@proving`, then gone, with focus back where it was
