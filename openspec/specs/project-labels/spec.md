# project-labels Specification

## Purpose
Lets the operator name a project the way they think of it, on their own
machine, so every surface of the workspace says that name instead of the
directory's.

## Requirements

### Requirement: A project's label is the operator's, kept on this machine
A project's label SHALL be kept in the operator's `config.toml`, under the
project's root (its nearest `agents.yaml` or `marketplace.json`, else its
Git repository, else the directory itself), and SHALL NOT be written into
the project. Every checkout of one project SHALL share its label. A label
written into the file by hand SHALL read as one set from the workspace.

#### Scenario: Two agents in their own checkouts
- **WHEN** the operator labels a project while two of its agents work in checkouts of their own
- **THEN** both agents' rows name the project by the new label

#### Scenario: Nothing reaches the project
- **WHEN** the operator labels a project
- **THEN** the project's working tree and its `agents.yaml` are unchanged

#### Scenario: A label written by hand
- **WHEN** the operator writes a label for a project into `config.toml` and opens the workspace
- **THEN** the project's agents' rows name it by that label

#### Scenario: A directory outside Git
- **WHEN** the operator labels the project of an agent working in a directory that is no Git repository
- **THEN** the label is kept under that directory and its rows name it by the label

#### Scenario: Forgetting work keeps the label
- **WHEN** the operator forgets, in recent, the last work of a labelled project
- **THEN** the project's label is still in `config.toml`

### Requirement: Every surface that names a project names it by its label
Every workspace surface that names a project SHALL name it by its label,
and by its directory's name when it has none: an agent's row, recent's
selected row and its filter, and the approval of a project's commands and
its toasts. A surface showing a project's path SHALL keep the path. The
command line SHALL be unaffected.

#### Scenario: A labelled project in recent
- **WHEN** the operator selects, in recent, work of a project labelled "uze core"
- **THEN** the row names the project "uze core"

#### Scenario: Filtering recent by label
- **WHEN** the operator filters recent by "core" and a project is labelled "uze core"
- **THEN** that project's work is listed

#### Scenario: Approval names the label
- **WHEN** a project labelled "uze core" has commands awaiting approval
- **THEN** the approval and its toasts name the project "uze core"

#### Scenario: A project with no label
- **WHEN** a project in a directory named `uze` has no label
- **THEN** every surface names it `uze`

### Requirement: The agent's menu renames the agent or its project
An agent's context menu SHALL offer one `Rename` entry opening `Agent`
and `Project`, by pointer and by keyboard as `Continue in` does, whenever
the agent's project is known. `Agent` SHALL rename the agent as before;
`Project` SHALL edit the project's label inline on that row, seeded with
the name the row shows. Elsewhere the entry SHALL stay a plain `Rename`.

#### Scenario: Renaming the project from the menu
- **WHEN** the operator opens an agent's menu, chooses `Rename` then `Project`, types "uze core" and confirms
- **THEN** every agent of that project names it "uze core", and the agent's own name is unchanged

#### Scenario: Renaming the agent from the menu
- **WHEN** the operator chooses `Rename` then `Agent`
- **THEN** the agent's name is edited inline, as it was before the submenu

#### Scenario: A shell's menu
- **WHEN** the operator opens the menu of a shell tab
- **THEN** it offers a plain `Rename`, renaming the tab

#### Scenario: An agent whose checkout was removed
- **WHEN** the operator opens the menu of an agent whose checkout was removed and whose project is known
- **THEN** `Rename` still offers `Project`

### Requirement: A menu entry that opens a submenu is marked by an arrow
A context menu entry that opens a submenu SHALL be marked by an arrow
against the menu's right edge, drawn from the glyph set in use, and SHALL
carry no ellipsis. An ellipsis SHALL mark only an entry that asks before
acting. A plain `Rename` that opens no submenu SHALL carry no arrow.

#### Scenario: The agent's menu
- **WHEN** the operator opens the menu of an agent whose project is known
- **THEN** `Rename` and `Continue in` each end in an arrow at the menu's right edge, and neither carries an ellipsis

#### Scenario: A shell's menu
- **WHEN** the operator opens the menu of a shell tab
- **THEN** its `Rename` carries no arrow

#### Scenario: An entry that asks first
- **WHEN** the operator opens recent's menu on work that can be discarded
- **THEN** the discard entry ends in an ellipsis and carries no arrow

### Requirement: A double click on the project renames it
A double click on the project an agent's row names SHALL edit that
project's label inline on that row. A single click, a right click and a
drag on it SHALL do what they do on the rest of the row, and it SHALL
answer the pointer as the row does. Editing SHALL work as renaming an
agent does, the caret never elided.

#### Scenario: Double click on the project
- **WHEN** the operator double clicks the project an agent's row names
- **THEN** the project's label is edited inline on that row

#### Scenario: The rest of the row is unchanged
- **WHEN** the operator right clicks or drags the project an agent's row names
- **THEN** the agent's menu opens, or the agent is dragged, as from its name

#### Scenario: Discarding the edit
- **WHEN** the operator presses escape while editing a project's label
- **THEN** the label is unchanged

### Requirement: Only a name other than the directory's is kept
Leading and trailing whitespace SHALL NOT be kept. Confirming an empty
label, or the directory's own name, SHALL take the project's label away,
leaving no trace of it in `config.toml`; confirming it for a project with
no label SHALL write nothing.

#### Scenario: Clearing a label
- **WHEN** the operator edits a project labelled "uze core" in directory `uze`, empties it and confirms
- **THEN** every surface names the project `uze` and `config.toml` holds no label for it

#### Scenario: Confirming the directory's name
- **WHEN** the operator begins renaming an unlabelled project and confirms the name unchanged
- **THEN** `config.toml` is not written

### Requirement: A label that cannot be read or kept is said
When labels cannot be read, surfaces SHALL name projects by their
directories and the workspace SHALL say why once. When a label cannot be
written, the workspace SHALL say why and keep the name it showed. A
`config.toml` that does not parse SHALL NOT be written over.

#### Scenario: Confirming while the file does not parse
- **WHEN** the operator confirms a project label while `config.toml` does not parse
- **THEN** the workspace says the file could not be read, the file is unchanged and the row keeps its previous name

#### Scenario: Opening while the file does not parse
- **WHEN** the workspace opens while `config.toml` does not parse
- **THEN** rows name projects by their directories and the problem is said once
