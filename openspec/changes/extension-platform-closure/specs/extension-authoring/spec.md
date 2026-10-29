## Purpose

What an extension author is given beyond the skill: a schema, a reference,
and an inspector that shows and replays what an extension does.

## ADDED Requirements

### Requirement: The manifest and compose wire have a generated schema that cannot drift
The repository SHALL carry a JSON Schema of the manifest and of a composed
answer generated from the contract's types, with a worked example per slot
and feed, and a test SHALL fail when the generated schema differs from
the one committed.

#### Scenario: A contract change without the schema fails
- **WHEN** a field is added to the contract and the schema is not regenerated
- **THEN** the drift test fails naming the command that regenerates it

### Requirement: The inspector shows and replays what an extension does
The inspector SHALL show an extension's runs, events, grants and cost, and
SHALL let a linked extension's author rerun a source with edited input,
switch it to its fixture, send a synthetic event and fake agents.

#### Scenario: Rerunning with edited input
- **WHEN** the author edits a source's input in the inspector and reruns it
- **THEN** the surface draws the new answer, and the run is listed with its input
