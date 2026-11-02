## MODIFIED Requirements

### Requirement: The operator's settings are one file they can edit

UZE SHALL keep the operator's machine settings — the theme, the glyph set,
their overrides of the active theme, and when finished agents ring — in one
TOML file, `config.toml`, at the root of the UZE home, one section per
concept. Writing one setting SHALL leave every other part of the file
unchanged, including comments and sections this build does not know. A file
that does not parse SHALL be reported and SHALL NOT be written over.

#### Scenario: Choosing a theme keeps the operator's comments

- **WHEN** `config.toml` holds a comment and a section UZE does not know, and
  the operator chooses a theme
- **THEN** the file holds the new theme and the comment and the unknown
  section exactly as they were

#### Scenario: A broken file is not overwritten

- **WHEN** `config.toml` does not parse and the operator chooses a setting
- **THEN** the choice is refused with the file named, and the file is left as
  it was

#### Scenario: Overrides written in the settings file

- **WHEN** the operator writes a colour under `[appearance.colors]` and a
  glyph under `[appearance.symbols]` in `config.toml`, then selects another
  theme
- **THEN** both apply over the newly selected theme

#### Scenario: An override that does not resolve

- **WHEN** an override in `config.toml` names a colour that is not one
- **THEN** UZE reports it, naming `config.toml` and the section, and draws
  the active theme without that override
