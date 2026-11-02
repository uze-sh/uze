## ADDED Requirements

### Requirement: Preferences are kept in the operator's config file
A preference's value SHALL be kept in `config.toml` under
`[preferences."<extension id>"]`, keyed by the id the extension's switch
uses. A value edited there by hand SHALL be read as one set on the screen;
a value the declaration does not admit SHALL read as its default. A
plugin leaving the machine SHALL leave its preferences in place. A secret
SHALL never be written to `config.toml`.

#### Scenario: A preference set on the screen is in the file
- **WHEN** the operator turns off the review's `mark-viewed-automatically`
- **THEN** `config.toml` holds `[preferences.review]` with `mark-viewed-automatically = false`

#### Scenario: A hand-edited value outside the declaration reads as the default
- **WHEN** `config.toml` holds `mark-viewed-automatically = "yes"`
- **THEN** the review reads it as `true`, its default

### Requirement: A built-in extension declares preferences like a declared one
A built-in extension MAY declare preferences of the same kinds as a
manifest, except `secret`. The Extensions drawer SHALL list and edit them
the way it does a declared extension's, and the extension SHALL receive
them when the workspace opens and whenever they change.

#### Scenario: The review's preference is edited in its drawer
- **WHEN** the operator selects Review on the Extensions screen
- **THEN** the drawer lists `Mark viewed once read` with its value, editable by key and by click
