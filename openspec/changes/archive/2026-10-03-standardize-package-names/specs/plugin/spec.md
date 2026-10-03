## ADDED Requirements

### Requirement: A name has one spelling, the one every harness accepts
The system SHALL hold every plugin name, marketplace name and install alias
to lowercase kebab-case: one or more runs of `a-z` and `0-9` joined by
single `-`, at most 64 characters. A name outside the rule SHALL be refused
before anything is written, and the refusal SHALL name the rule and, when
one can be derived, the corrected name.

#### Scenario: An uppercase plugin name is refused with the name it meant
- **WHEN** a marketplace's plugin carries `"name": "Flow"` in its
  `plugin.json` and the operator installs it
- **THEN** the install fails before any byte reaches the Store, and the
  error says `try \`flow\``

#### Scenario: A name the vendor CLIs would read as a flag is refused
- **WHEN** a plugin is named `-force`
- **THEN** it is refused by the same rule, never passed to a vendor CLI

#### Scenario: Two spellings never become two packages
- **WHEN** a package `flow@ai` is installed and the operator installs
  `Flow@AI`
- **THEN** it resolves to the same package `flow@ai`, and no second Store
  entry or registration is created

### Requirement: The case a person types is forgiven
The system SHALL lowercase a plugin, marketplace or alias name a person
types to refer to one that exists or to choose an alias (the
`<plugin>@<market>` install spec, the `update`, `remove` and `inspect`
plugin argument, the name argument of `uze market remove`, `link`, `unlink`
and `inspect`, `--alias`, the `--market` of `uze agent plugin create`, and a
name typed at the collision prompt) before resolving it. Resolution against
the record SHALL stay exact. A name that becomes an authored file's name,
the `<name>` of `uze agent market create` and `uze agent plugin create`,
SHALL NOT be lowercased but refused under the name rule, and neither SHALL
a name read from a file, because the harness reading the same file does
not forgive it.

#### Scenario: Install and remove in another case
- **WHEN** the operator runs `uze install -m Flow@AI` and then
  `uze remove -m FLOW`
- **THEN** `flow@ai` is installed and then removed
