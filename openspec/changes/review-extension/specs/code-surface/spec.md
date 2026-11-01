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
another surface hands it SHALL open in files, at the line given.

#### Scenario: The modes on offer
- **WHEN** the operator opens the code surface
- **THEN** its modes are `files` and `map`, and no `changes`

#### Scenario: Opened from the review
- **WHEN** the review hands it `naming.rs` at line 53
- **THEN** the code surface shows `naming.rs`'s contents with line 53 in view
