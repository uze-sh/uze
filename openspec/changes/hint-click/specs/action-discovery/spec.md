## ADDED Requirements

### Requirement: A hint is the action it names
Every key a surface advertises in a hint SHALL be a pointer target for the
action it names. Pointing at the key SHALL light it before it is clicked,
and clicking it SHALL perform that action exactly as pressing the key does,
through the same gate that decides whether the action is offered there.
What a hint says beside its keys SHALL NOT be a target of its own. A hint
written inside text that scrolls is exempt, and says so where it is drawn.

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

#### Scenario: A run offers each of its keys
- **WHEN** a hint names a run of keys as one entry, such as `↓↑ move`
- **THEN** each key of the run is a target for its own action, and the
  run's label is not a target

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
