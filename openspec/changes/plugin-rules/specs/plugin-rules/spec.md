## Purpose

An organization writes its agents' conventions once, as rules in a plugin,
and every project that declares the plugin receives them in its own
checkout. The rules are committed and verified against the lock, and
never apply in a project that did not declare them.

## ADDED Requirements

### Requirement: A plugin ships rules in the canonical rule format
A package MAY contain `rules/*.md` at its root. Each file SHALL be a rule in
the format `project-rules` defines (trigger, globs, description). Glob
patterns are relative to the root of the checkout that receives the rule.
`uze agent plugin check` SHALL validate them with the same validator as
`uze agent rules check`, and SHALL fail on a file that is not a deliverable
rule. `uze agent plugin create --rules` SHALL scaffold `rules/` with one
example rule that passes the check.

#### Scenario: A plugin with an invalid rule fails its check
- **WHEN** a plugin's `rules/ui.md` declares `trigger: glob` without `globs`
- **THEN** `uze agent plugin check` exits non-zero naming `rules/ui.md` and the missing field

#### Scenario: Scaffolded rules pass the check
- **WHEN** a plugin is created with `--rules`
- **THEN** `uze agent plugin check` on it succeeds

### Requirement: A project that declares the plugin receives its rules in its checkout
When a project declares a plugin that ships rules, `uze <plugin>@<market>`
and `uze install` SHALL copy the rules, byte for byte, into a location under
`.agents/rules/` namespaced by the plugin's identity in `agents.lock`. They
SHALL never write into the project's own rules or another plugin's. The
copies SHALL then be delivered by the project's rules route on every harness,
exactly as the project's own rules are. Nothing about them SHALL be
delivered to a location a harness reads for every project.

#### Scenario: Rules land in the declaring project only
- **WHEN** project A runs `uze acme-conventions@acme` and project B on the same machine does not declare it
- **THEN** A's `.agents/rules/` contains the plugin's rules in the plugin's namespace
- **AND** an agent editing a matching file in B receives none of them, on any harness

#### Scenario: The project's own rule with the same file name is untouched
- **WHEN** the project already has `.agents/rules/ui.md` and the plugin ships `rules/ui.md`
- **THEN** both exist, the project's file is unchanged, and both are delivered

### Requirement: The copies are committed and verified against the lock
The copies SHALL be ordinary project files meant to be committed. UZE SHALL
NOT add them to any ignore file. A copy's owner SHALL be derived from the
project alone: the plugin whose namespace contains it, declared in
`agents.lock`. A copy SHALL be current when its bytes equal the plugin's rule
at the locked revision, whose bytes the lock's `integrity` pins. This SHALL
hold on a machine that has never installed the plugin, with no UZE state
required to judge it.

#### Scenario: A fresh clone is judged without machine state
- **WHEN** a person clones a project with committed plugin rules on a machine with an empty `$UZE_HOME` and runs `uze status`
- **THEN** each plugin's rules are reported as current or not by comparison with the plugin at the locked revision

#### Scenario: A clone without UZE still has the rules
- **WHEN** a project with committed plugin rules is cloned on a machine without UZE and opened with Antigravity
- **THEN** Antigravity applies the plugin's rules from the checkout

### Requirement: An edited copy is reported, never overwritten
When a copy's bytes differ from the plugin's rule at the locked revision,
`uze status` SHALL report it as drifted, naming the file. `uze install` and
`uze update` SHALL refuse to overwrite or remove a drifted copy, SHALL name
it, and SHALL leave the rest of the operation's outcome reported. A copy
missing from the namespace SHALL be restored by `uze install`. A file in the
namespace that the plugin does not ship at the locked revision SHALL be
reported and left in place.

#### Scenario: A hand edit survives install
- **WHEN** someone edits `.agents/rules/<plugin namespace>/ui.md` and runs `uze install`
- **THEN** the file keeps their edit and install reports it as drifted from the lock

#### Scenario: A deleted copy is restored
- **WHEN** a copy is deleted and `uze install` runs
- **THEN** the copy is restored from the plugin at the locked revision

### Requirement: Updating and removing a plugin carries its rules
`uze update <plugin>` SHALL replace the plugin's copies with its rules at the
new locked revision. That includes adding new rules and removing rules the
new revision no longer ships, after inspecting that each copy it replaces or
removes is current. `uze remove <plugin>` in a project SHALL remove the
plugin's namespace under `.agents/rules/` after the same inspection, and
SHALL leave the plugin's machine-wide capabilities to the rules that already
govern them.

#### Scenario: An update is a reviewable diff
- **WHEN** `uze update acme-conventions` moves the lock to a revision that changes `rules/ui.md` and deletes `rules/legacy.md`
- **THEN** the working tree shows `ui.md` modified and `legacy.md` deleted in the plugin's namespace, beside the `agents.lock` change

#### Scenario: Removing the plugin removes its rules only
- **WHEN** `uze remove acme-conventions` runs in a project with its own rules and the plugin's
- **THEN** the plugin's namespace is gone and the project's own rules are unchanged

### Requirement: Machine-scope installation does not deliver rules
`uze plugin install` (machine scope) SHALL deliver a plugin's other
capabilities as before and SHALL report its rules as delivered per project
only, naming the command that declares it in a project. It SHALL write
nothing into any project.

#### Scenario: Machine install reports the rules route
- **WHEN** a plugin with rules is installed with `uze plugin install`
- **THEN** its rules are reported as not delivered on this scope, with the per-project reason
- **AND** no `.agents/rules/` is created anywhere

### Requirement: Status reports each plugin's rules
`uze status` SHALL report, per declared plugin that ships rules, the number
of rules and whether its copies are current, drifted, missing or carrying
extra files, naming each file that is not current.

#### Scenario: Status separates the plugin's rules from the project's
- **WHEN** a project has two rules of its own and three from `acme-conventions`, one of them drifted
- **THEN** status reports two project rules, and three plugin rules with one drifted file named
