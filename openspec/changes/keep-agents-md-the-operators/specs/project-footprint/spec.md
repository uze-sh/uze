## Purpose

States what UZE may write into a project's checkout and what it never
writes, so a project that stops using UZE keeps its own files as it wrote
them.

## ADDED Requirements

### Requirement: UZE writes only the declaration and agnostic artifacts into a project
Into a project's checkout UZE SHALL write only the declaration the operator
asked for (`agents.yaml`, `agents.lock`) and agnostic artifacts: files in an
open format, under a namespace UZE owns, that a harness reads without UZE.
Anything else UZE has to tell an agent SHALL reach it through the launch.

#### Scenario: Installing a project writes only what it owns
- **WHEN** `uze install` runs in a project that declares plugins, one of them shipping rules
- **THEN** the only files it creates or changes in the checkout are `agents.yaml`, `agents.lock` and the plugin's rules namespace under `.agents/rules/`

#### Scenario: Launching an agent writes nothing into the checkout
- **WHEN** the workspace launches an agent in a project that declares a workspace policy
- **THEN** no tracked file of the project's checkout is created or changed by UZE, apart from what the project's own declared `setup` and `link` commands do

### Requirement: UZE never writes into AGENTS.md
No UZE operation SHALL create, change or append to a project's `AGENTS.md`,
except to remove a region an earlier UZE wrote. A project with no
`AGENTS.md` SHALL get none from UZE.

#### Scenario: A declared plugin adds nothing to AGENTS.md
- **WHEN** a project declares a plugin and runs `uze install`
- **THEN** `AGENTS.md` keeps its bytes

#### Scenario: A project without AGENTS.md
- **WHEN** `uze install` and a workspace launch run in a project without `AGENTS.md`
- **THEN** no `AGENTS.md` exists afterwards

### Requirement: Regions an earlier UZE wrote are removed
In the primary checkout, `uze install` and `uze agent context reconcile`
SHALL remove every region an earlier UZE wrote into `AGENTS.md`, leaving the
bytes outside them as they were. A `project:` region whose content no longer
matches the digest in its identity SHALL be reported and left in place. A
`package:` region SHALL be removed without a content check, as today. In an
isolated checkout they SHALL remove nothing.

#### Scenario: The old regions go
- **WHEN** a project's `AGENTS.md` carries the worktree-policy, plugin-authoring and a package instructions region, and `uze install` runs
- **THEN** none of the three remains, and the text the operator wrote around them is byte-identical

#### Scenario: An edited region stays
- **WHEN** the worktree-policy region was edited by hand
- **THEN** it is left in place and `uze install` reports it as edited, naming `AGENTS.md`

#### Scenario: An isolated checkout is left alone
- **WHEN** `uze install` runs in an isolated checkout whose committed `AGENTS.md` carries a retired region
- **THEN** the file is unchanged, so no branch carries the removal and no slot is parked by it

### Requirement: The Claude bridge goes with the last region
When the cleanup removes the last package region, the `CLAUDE.md` bridge
region UZE wrote SHALL be removed too, and a `CLAUDE.md` that held nothing
but that region SHALL be deleted. A `CLAUDE.md` with other text SHALL keep
it.

#### Scenario: A bridge-only CLAUDE.md
- **WHEN** a project's `CLAUDE.md` holds only UZE's `@AGENTS.md` bridge region and `uze install` removes the last package region
- **THEN** `CLAUDE.md` no longer exists

#### Scenario: A CLAUDE.md the operator also wrote
- **WHEN** `CLAUDE.md` holds the bridge region and lines the operator wrote
- **THEN** only the region is removed
