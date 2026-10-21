## Purpose

States how an `AGENTS.md` in a subdirectory below the launch directory
reaches every supported harness the same way, and how that delivery is
reported and proven.

## ADDED Requirements

### Requirement: A nested AGENTS.md reaches the agent when it works there
The system SHALL deliver an `AGENTS.md` in a directory below the launch
directory to every harness the first time in a session the agent reads,
edits or writes a file under that directory through a file tool, and again
after a compaction where the harness has one. With several nested files,
each one between the file and the launch directory SHALL arrive, outermost
first.

#### Scenario: Reading a file under a nested AGENTS.md
- **WHEN** an agent launched at the checkout root reads `sub/notes.txt` with its file-reading tool, and `sub/AGENTS.md` exists
- **THEN** the request after that read returned carries the content of `sub/AGENTS.md`
- **AND** this holds for Claude Code, Codex, OpenCode and Antigravity, or the harness reports it unsupported with its reason

#### Scenario: Editing a file under a nested AGENTS.md
- **WHEN** the agent edits or creates `sub/new.txt` and `sub/AGENTS.md` has not arrived in this session
- **THEN** the request after that tool call carries the content of `sub/AGENTS.md`

#### Scenario: Two nested levels
- **WHEN** the agent reads `a/b/notes.txt` and both `a/AGENTS.md` and `a/b/AGENTS.md` exist
- **THEN** both arrive, `a/AGENTS.md` first

#### Scenario: Once per session
- **WHEN** the agent reads a second file under `sub/` in the same session
- **THEN** `sub/AGENTS.md` is not delivered again

#### Scenario: After a compaction
- **WHEN** the session is compacted and the agent then reads a file under `sub/`
- **THEN** `sub/AGENTS.md` is delivered again

#### Scenario: A project with no rules
- **WHEN** the checkout has `sub/AGENTS.md` and no `.agents/rules`
- **THEN** a read under `sub/` still delivers `sub/AGENTS.md`

#### Scenario: A file outside any nested AGENTS.md
- **WHEN** the agent reads a file with no `AGENTS.md` between its directory and the launch directory
- **THEN** no instruction file is delivered with that read

### Requirement: A path recovered from a shell command is best effort
The system SHALL deliver a nested `AGENTS.md` for a path the agent touched
through a shell command only where the harness's own behavior or the rules
engine recovers that path. A harness whose delivery differs between file
tools and shell commands SHALL state the difference in its reported reason.

#### Scenario: Reading through a shell command
- **WHEN** the agent runs `cat sub/notes.txt` in a harness delivered through the rules engine
- **THEN** `sub/AGENTS.md` arrives, as a glob rule would for the same command

### Requirement: Each nested file reaches the model once
The system SHALL leave nested delivery to a harness that performs it
itself and SHALL deliver through the rules engine only to one that does
not, so a nested `AGENTS.md` never reaches the model twice in one session.

#### Scenario: A harness that reads nested files itself
- **WHEN** Claude Code reads a file under `sub/` with the rules engine attached
- **THEN** `sub/AGENTS.md` reaches the model exactly once

#### Scenario: A harness that needs the engine
- **WHEN** Codex reads a file under `sub/` with the rules engine attached
- **THEN** `sub/AGENTS.md` reaches the model through the engine

### Requirement: Nothing is written into the project
The system SHALL NOT write a rule, an import line, a copy of a nested
`AGENTS.md` or any other file into the project to deliver nested
instruction files.

#### Scenario: A session that delivered a nested file
- **WHEN** a session delivered `sub/AGENTS.md` through the engine
- **THEN** the checkout's working tree is unchanged

### Requirement: Nested delivery is reported per harness
The system SHALL report, per harness, how a nested `AGENTS.md` reaches it
(native, rules engine, or unsupported with its reason) in `uze status`,
the harness drawer and `uze agent context inspect`. A harness that needs
the engine where setup has not delivered it SHALL point at `uze setup`.
Where the harness's project context is shadowed, the nested row SHALL say
so too.

#### Scenario: Status for a harness on the engine
- **WHEN** `uze status` runs on a machine where Codex is set up
- **THEN** Codex's nested-instructions row names the rules engine

#### Scenario: The engine not delivered yet
- **WHEN** Codex is detected but setup has not delivered the rules engine to it
- **THEN** its nested-instructions row points at `uze setup`

#### Scenario: Claude shadowed by a CLAUDE.md
- **WHEN** a `CLAUDE.md` without an import of `AGENTS.md` sits on Claude's walk
- **THEN** Claude's nested-instructions row reports the same shadowing as its `AGENTS.md` row, not native
