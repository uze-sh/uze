## MODIFIED Requirements

### Requirement: Plugin install never provisions harness executables implicitly

`uze add <name>@<marketplace>` SHALL prepare and attach to already
detected harnesses but SHALL NOT install, update, or remove a harness
executable.

#### Scenario: A supported harness is absent during plugin install

- **WHEN** a plugin is installed and a harness executable is absent
- **THEN** UZE installs the package once in its Store
- **AND** delivers it to the detected harnesses only
- **AND** does not invoke networked provision commands.
