## Purpose

A source that answers the shape of its surface itself, as a tree of the
catalog's nodes, for what a view file cannot express without becoming a
language.

## ADDED Requirements

### Requirement: A composed answer is a tree of catalog nodes
A source declaring `output "view"` SHALL answer a JSON tree whose nodes
name catalog types with their arguments, properties and children. The host
SHALL validate the tree against the catalog and a node cap before drawing
it, and SHALL draw nothing of a tree that does not validate, naming the
first wrong node by its path.

#### Scenario: A node outside the catalog is refused
- **WHEN** a composed answer holds a node of type `iframe`
- **THEN** the surface says the answer names a node the catalog does not have, at its path, and draws nothing of it

#### Scenario: A segmented bar per phase
- **WHEN** `gsd-band`'s script answers a `Progress` with one `Segment` per phase
- **THEN** the band draws one segment per phase, however many there are

### Requirement: A composed control runs only a declared action on its own item
A `Button` or `Toggle` in a composed tree SHALL name an action the manifest
declares, and its `item` SHALL become that action's selection, so the
action's templates and confirmation resolve against it. A click within
300 ms of the node's re-layout SHALL be ignored.

#### Scenario: A button archives its own row
- **WHEN** a composed `Button` names `archive` with item `{"name": "auth"}` and the action confirms `Archive {name}?`
- **THEN** pressing it asks "Archive auth?", attributed, and nothing else
