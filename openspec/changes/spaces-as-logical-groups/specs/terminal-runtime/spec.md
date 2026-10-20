## ADDED Requirements

### Requirement: A space is its id and its label
The terminal runtime SHALL identify a space by an id that survives a
restart and is never given to another space, and SHALL carry with it a
label and its tabs and nothing else: no directory. A tab's directory SHALL
be the only place a space's whereabouts are kept. Creating a space SHALL
never select another.

#### Scenario: Two spaces working in one directory
- **WHEN** a client creates a space while another holds tabs in the same directory
- **THEN** a second space is created, and both are kept

#### Scenario: The space round-trips through a restart
- **WHEN** a space is created and the server is restarted
- **THEN** the restored space has the same id, label and tabs, each tab in its own directory

#### Scenario: A closed space's id is not reused
- **WHEN** the space with the highest id is closed, the server restarts, and a space is created
- **THEN** the new space's id is one no space had before

### Requirement: Creating a space asks for its name
Creating a space explicitly SHALL ask for its name and nothing else. The new
space's first shell SHALL open in the operator's home directory.

#### Scenario: A named space
- **WHEN** the operator creates a space and names it `storefront`
- **THEN** a space labelled `storefront` exists, its shell standing in the home directory

#### Scenario: A name left empty
- **WHEN** the operator confirms the prompt without a name
- **THEN** the space is labelled `space` numbered as the labels already taken require

### Requirement: A launch selects the space working in its project
Starting `uze` in a project SHALL select, for that client, a space with an
agent tab standing in that project, its root or below it; else one with any
tab there. Only when none has one SHALL it create a space labelled from the
project, its first shell there. A directory in no project counts as its
own. Started in the home directory, it SHALL select what the client last
had and create nothing.

#### Scenario: A space already works there
- **WHEN** the operator starts `uze` in a project an agent of space `storefront` stands in
- **THEN** `storefront` is selected and no space is created

#### Scenario: A project no space works in
- **WHEN** the operator starts `uze` in a project no space has a tab in
- **THEN** a space labelled from that project is created and selected, its shell standing in the project

#### Scenario: An agent's space wins over a shell's
- **WHEN** one space has a shell in the project, another has an agent there, and the operator starts `uze` in it
- **THEN** the space with the agent is selected

#### Scenario: Starting again where a launch opened a space
- **WHEN** a launch in a project created a space, nothing was started in it, and `uze` is started there again
- **THEN** that space is selected and no space is created

#### Scenario: A space standing above the project
- **WHEN** the only space holds a shell standing in the home directory, and the operator starts `uze` in a project below it
- **THEN** that space is not selected, and a space for the project is created

#### Scenario: A launch from home
- **WHEN** the operator starts `uze` in their home directory
- **THEN** no space is created, and the client lands where it last was

## MODIFIED Requirements

### Requirement: A launch inside a pane opens a space
The system SHALL mark every pane it spawns so a `uze` started inside one
can tell, and such a `uze` SHALL NOT open a client inside the client: it
SHALL ask the running server for the space a launch in its directory
selects, created when none has a tab in that project, and exit reporting
the space.

#### Scenario: Nested launch opens a space and leaves
- **WHEN** `uze` is started inside one of the server's own panes, in a project no space has a tab in
- **THEN** a space whose shell stands in that project appears in the running client and the nested `uze` exits without attaching a client

#### Scenario: Nested launch in a project a space works in
- **WHEN** `uze` is started inside a pane, in a project a space already has a tab in
- **THEN** no space is created, and the nested `uze` exits naming that space

### Requirement: A workspace written by an earlier build opens on this one

The persisted workspace SHALL be carried across to the shape this build
reads, keeping every space, its label, its tabs and each tab's directory
and launch. A field an earlier shape carried that this one dropped SHALL
be the only thing lost. Spaces SHALL NOT be lost because the workspace was
written by an earlier build whose shape this one knows.

#### Scenario: Upgrading with spaces open

- **WHEN** UZE is upgraded and the persisted workspace was written by an
  earlier shape this build knows
- **THEN** the same spaces SHALL open, with the same labels, tabs and
  directories
- **AND THEN** nothing SHALL be reported

#### Scenario: The shape that carried a kind per space

- **WHEN** the workspace was written when a space carried a kind of its
  own
- **THEN** every space SHALL open without it
- **AND THEN** nothing else about the workspace SHALL change

#### Scenario: The shape that rooted every space

- **WHEN** the workspace was written when a space was identified by its root
- **THEN** every space SHALL open without its root and with an id of its own
- **AND THEN** every tab SHALL stand where it stood, the space's first
  shell included

## REMOVED Requirements

### Requirement: A space is a root, and the runtime knows nothing else about it
**Reason**: A space is a group of agents, not a directory; one directory no longer names one space.
**Migration**: Replaced by "A space is its id and its label". A persisted root is dropped; the tabs keep their directories.

### Requirement: A space has a root
**Reason**: Agents choose their directory when they start, and nothing else needs the space to have one.
**Migration**: Replaced by "A space is its id and its label", "Creating a space asks for its name" and "A launch selects the space working in its project", and by the directory step in `space-groups`.

### Requirement: A new space's root is chosen, not typed
**Reason**: Creating a space no longer asks for a directory.
**Migration**: The same narrowing directory prompt now chooses a new agent's directory: see "A new agent chooses its directory before its harness" in `space-groups`.
