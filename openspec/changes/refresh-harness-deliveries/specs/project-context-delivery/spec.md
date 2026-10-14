## Purpose

States how a project's `AGENTS.md` reaches each supported harness, which
files can shadow it, and that UZE never writes a bridge file into the
project to deliver it.

## ADDED Requirements

### Requirement: Every harness reads the project context natively
The system SHALL deliver the project's `AGENTS.md` to every detected
harness through that harness's own reading of the file, and SHALL NOT
write a bridge file, an import line or any file other than `AGENTS.md`
itself into the project to deliver it. A harness below the version that
reads `AGENTS.md` by default is reported by the harness version floor,
never bridged.

#### Scenario: A project with only AGENTS.md
- **WHEN** `uze agent context reconcile` runs in a project whose only context file is `AGENTS.md`
- **THEN** no file other than `AGENTS.md` is written into the project
- **AND** every detected harness reports the context as delivered natively

#### Scenario: The context reaches Claude's model with no CLAUDE.md
- **WHEN** Claude Code is launched in a project with `AGENTS.md` and no `CLAUDE.md`
- **THEN** the model receives the content of `AGENTS.md`

#### Scenario: The context reaches the model from an agent's checkout
- **WHEN** a managed agent is launched in its own checkout of the project
- **THEN** the model receives that checkout's `AGENTS.md`

### Requirement: A file the harness prefers is reported as shadowing the context
The system SHALL report the project context as shadowed for a harness when
a file that harness reads in place of `AGENTS.md` sits at the project root
with content of its own and no import of `AGENTS.md`, naming that file. A
shadowed context SHALL be a portability gap in `uze status`, `uze doctor`,
the workspace and `uze agent context inspect`, SHALL point the person at
`/uze:init`, and SHALL never cause UZE to write into the shadowing file.
An empty file, a file importing `AGENTS.md` (`@AGENTS.md` or
`@./AGENTS.md`), a file that resolves to the project's `AGENTS.md` itself,
or a region an earlier UZE build wrote that imports it SHALL NOT shadow.

#### Scenario: A CLAUDE.md with its own content
- **WHEN** the project root holds a `CLAUDE.md` with content and no import of `AGENTS.md`
- **THEN** Claude's context is reported as read from `CLAUDE.md` instead, with `/uze:init` as the remedy
- **AND** the file is left untouched

#### Scenario: A CLAUDE.md that imports AGENTS.md
- **WHEN** the project root holds a `CLAUDE.md` whose content imports `@AGENTS.md` or `@./AGENTS.md`
- **THEN** Claude's context is reported as delivered natively

#### Scenario: A CLAUDE.md that is a link to AGENTS.md
- **WHEN** the project root's `CLAUDE.md` is a symbolic link resolving to the project's `AGENTS.md`
- **THEN** Claude's context is reported as delivered natively

#### Scenario: A region from an earlier build
- **WHEN** a `CLAUDE.md` holds only the bridge region an earlier UZE build wrote
- **THEN** it does not shadow and is left as it is

### Requirement: A vendor context file is migrated into AGENTS.md by the person's agent, never by reconcile
`AGENTS.md` SHALL be the one canonical project context. Moving content
from a vendor context file into it SHALL be proposed by the `/uze:init`
Skill and applied only after the person confirms; `uze install`, `uze
agent context reconcile` and every other deterministic command SHALL NOT
move, merge, rewrite or delete a vendor context file. A migration SHALL
move every instruction that helps any agent into `AGENTS.md`, SHALL leave
the vendor file importing `AGENTS.md` and holding only what is specific
to that harness (or remove it when nothing is left and the person
agrees), and SHALL NOT carry a vendor-only import line into `AGENTS.md`
verbatim. A personal, uncommitted vendor file (`CLAUDE.local.md`) SHALL
NOT be merged into `AGENTS.md`.

#### Scenario: Only CLAUDE.md exists
- **WHEN** `/uze:init` runs in a project with a `CLAUDE.md` and no `AGENTS.md`, and the person confirms the proposal
- **THEN** `AGENTS.md` holds the portable instructions, `CLAUDE.md` imports it and keeps only Claude-specific lines
- **AND** Claude's context is reported as delivered natively afterwards

#### Scenario: AGENTS.md and a shadowing CLAUDE.md both exist
- **WHEN** `/uze:init` runs where `CLAUDE.md` shadows an existing `AGENTS.md`, and the person confirms
- **THEN** the portable instructions of both end up once in `AGENTS.md`, with every conflict between them put to the person rather than decided
- **AND** `CLAUDE.md` imports `AGENTS.md` and no longer shadows it

#### Scenario: Reconcile meets a shadowing CLAUDE.md
- **WHEN** `uze agent context reconcile` or `uze install` runs where `CLAUDE.md` shadows `AGENTS.md`
- **THEN** neither file's authored content is changed and the shadowing is reported

#### Scenario: A personal CLAUDE.local.md
- **WHEN** the project has a `CLAUDE.local.md`
- **THEN** its content is never proposed for `AGENTS.md`

### Requirement: The runtime shim projects only what the harness does not read
The runtime PATH shim SHALL NOT project `AGENTS.md` into a harness that
reads it natively. For Claude Code it SHALL activate only when the project
has an `.agents/` directory, and SHALL project only the parts of `.agents/`
that Claude does not read itself.

#### Scenario: No .agents directory
- **WHEN** Claude Code is launched through the shim in a project with `AGENTS.md` and no `.agents/`
- **THEN** the shim adds no directory and no environment to the invocation

#### Scenario: Project skills are still delivered
- **WHEN** the project has `.agents/skills/<name>/SKILL.md`
- **THEN** that skill reaches Claude's model through the shim
