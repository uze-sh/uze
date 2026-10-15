## ADDED Requirements

### Requirement: A hint is the action it names
Every hint of one key a surface draws SHALL be a pointer target for the
action it names, over its key and its words. Pointing at it SHALL light it
before it is clicked, and clicking it SHALL perform that action exactly as
pressing the key does. A run of keys SHALL NOT be a target. A hint written
inside scrolling text is exempt.

#### Scenario: Clicking a key is pressing it
- **WHEN** an operator clicks the `d` of a `d delete` hint in a surface's
  footer
- **THEN** the outcome is the one pressing `d` in that surface produces,
  including its confirmation when the action is destructive

#### Scenario: A key answers the pointer before it is clicked
- **WHEN** the pointer rests on a key in a hint
- **THEN** that key's hint is drawn lit, and every other hint on the line is
  drawn as before

#### Scenario: Every hint lights the same way
- **WHEN** hints are hovered on different surfaces, in the management modal
  and in the workspace
- **THEN** each lights identically, because the look belongs to the hint and
  not to the surface that drew it

#### Scenario: A hint's words are as much its target as its key
- **WHEN** an operator clicks the word `delete` of a `d delete` hint
- **THEN** the outcome is the one pressing `d` produces

#### Scenario: A run is read, not pointed at
- **WHEN** a hint names a run of keys as one entry, such as `↓↑ move` or
  `1–5 section`
- **THEN** no part of it is a target and it never lights, because it is
  the keyboard's way through a list the pointer walks by pointing at what
  it wants

#### Scenario: A rebound key is still the target
- **WHEN** an operator rebinds an action that a hint advertises
- **THEN** the hint names the new chord and the click still performs that
  action, because the target is the action and not the letter

#### Scenario: A dialog's answer is never taken from a hint beneath it
- **WHEN** a dialog is open over a surface whose footer shows hints
- **THEN** a click on one of those hints is not read as the dialog's answer
  and performs nothing the dialog did not offer

#### Scenario: A hint cut for width offers only what it drew
- **WHEN** a hint line is too narrow for every entry and drops the last ones
- **THEN** only the keys actually drawn are targets
