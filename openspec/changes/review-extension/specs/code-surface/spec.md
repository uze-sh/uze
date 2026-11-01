## MODIFIED Requirements

### Requirement: The map is offered beside the file tree, never beside the changes
The map SHALL be offered only where the file tree is what the surface is
navigating. A checkout's changes are the review's, which SHALL NOT offer
the map, by chip or by key: a map of the whole checkout answers a question
nobody reviewing a change is asking.

#### Scenario: Beside the tree, and not beside the diff
- **WHEN** the review is showing the changes of a measured checkout
- **THEN** the map SHALL NOT be among the ways of showing it
- **AND THEN** the key that toggles it in the code surface SHALL do nothing in the review

### Requirement: A file on the map is the surface's selection
Activating a selected file's tile, or clicking it a second time, SHALL
select that file and leave the map for the file tree. The map is another
way of asking about the same path, so the selection SHALL survive the
switch.

#### Scenario: Down to the file
- **WHEN** the tile of `src/main.rs` is selected and activated
- **THEN** `src/main.rs` SHALL be the surface's selection
- **AND THEN** the file tree SHALL be on show again

### Requirement: Returning to an agent shows the code surface as it was left
When an agent comes back to the front with the code surface kept for it, the
surface SHALL show the mode it was left on, the selection, the opened
directories and the scroll. A fresh open through its door SHALL open on
the file tree.

#### Scenario: Files left open on an agent
- **WHEN** the code surface shows a file in Files on agent A, and the operator selects agent B, then agent A
- **THEN** the code surface stands on A in Files, on that file

## REMOVED Requirements

### Requirement: A changed image is drawn in the changes half
**Reason**: the code surface no longer has a changes half; a checkout's
changes are the review surface's.
**Migration**: the behavior is unchanged and now required by
`review-surface`'s "A changed image is drawn in the review".

## ADDED Requirements

### Requirement: The code surface navigates and edits, and does not review
The code surface SHALL offer files and map, with its search as the field
over the file tree. It SHALL NOT offer a changes mode or a diff. A path
another surface hands it SHALL open in files, at the line given when one
is.

#### Scenario: The modes on offer
- **WHEN** the operator opens the code surface
- **THEN** its modes are `files` and `map`, and no `changes`

#### Scenario: Opened from the review
- **WHEN** the review hands it `naming.rs` at line 53
- **THEN** the code surface shows `naming.rs`'s contents with line 53 in view

#### Scenario: A pane path with a line
- **WHEN** the operator ctrl+clicks `src/main.rs:42` in an agent's pane
- **THEN** the code surface shows `src/main.rs` with line 42 in view
