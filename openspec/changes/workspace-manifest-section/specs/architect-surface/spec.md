## MODIFIED Requirements

### Requirement: A project declares where its architecture artifacts live
A project SHALL declare where its artifacts live as `workspace.artifacts`
in `agents.yaml`: one directory, or a list of them, each relative to the
project root. The declaration SHALL name places only, never kinds: what a
file is, it says by its own content, so the surface reads the Mermaid
files under every declared directory and ignores the rest. The declaration
SHALL be optional. A path that is absolute, or that leaves the project,
SHALL be refused rather than followed, and the refusal SHALL name that
entry; nothing SHALL be read until the declaration is fixed.

#### Scenario: The project declares a directory
- **WHEN** `agents.yaml` holds `workspace: { artifacts: docs }`
- **THEN** the artifacts are read from `docs` under the project root, as
  deep as it goes

#### Scenario: The project declares several directories
- **WHEN** `workspace.artifacts` is `[docs, design]`
- **THEN** the diagrams of both are listed together

#### Scenario: A declared directory holds files of other kinds
- **WHEN** a declared directory holds Markdown, images and Mermaid files
- **THEN** only the Mermaid files are listed, and nothing is reported about
  the others

#### Scenario: The project declares nothing
- **WHEN** the surface is opened in a project whose `agents.yaml` has no
  `workspace.artifacts`
- **THEN** the surface SHALL say the project declares no artifacts yet
- **AND THEN** it SHALL show how to declare them, and SHALL NOT present
  this as an error

#### Scenario: The declared path leaves the project
- **WHEN** `workspace.artifacts` is `[docs, ../elsewhere]`
- **THEN** nothing SHALL be read, and the surface SHALL say that the
  `../elsewhere` entry of `workspace.artifacts` needs fixing

#### Scenario: The declared directory holds no diagram
- **WHEN** every declared directory exists and none holds a Mermaid file
- **THEN** the surface SHALL name the declared paths and say they hold no
  Mermaid files yet

#### Scenario: An unknown key under artifacts
- **WHEN** `workspace.artifacts` is a mapping, such as `{ path: docs }`
- **THEN** the manifest SHALL be rejected, saying `workspace.artifacts`
  takes a directory or a list of them
