## ADDED Requirements

### Requirement: Setup steps and gates are spelled per platform

Each `worktrees.setup` step and each `gate` command in `agents.yaml` SHALL be
a string, or a map holding a `posix` spelling, a `windows` spelling, or both.
A string SHALL mean the `posix` spelling. UZE SHALL run the `posix` spelling
through the POSIX shell on Linux and macOS, and the `windows` spelling through
Windows PowerShell 5.1 on Windows. A command's exit status SHALL be the
status of its last command, and a failing command within it SHALL fail the
command.

A command with no spelling for the platform SHALL NOT be run in another
shell:

- **Setup step:** it is skipped, and the checkout it was skipped for SHALL be
  reported as not ready, naming the step.
- **Gate:** it SHALL be reported when an agent is placed and when the
  workspace opens, and delivery SHALL fail closed on it.

`uze status` SHALL name every command the project declares that has no
spelling for the current platform.

#### Scenario: A POSIX-only setup step on Windows

- **WHEN** an agent's checkout is placed on Windows, and `agents.yaml`
  declares `setup: pnpm install && cp .env.example .env`
- **THEN** the step does not run, the checkout is placed, and the checkout is
  reported not ready, naming the step and its missing Windows spelling

#### Scenario: A gate without a Windows spelling

- **WHEN** an agent is placed on Windows, and a declared gate has only a
  `posix` spelling
- **THEN** the workspace reports the gate at placement
- **AND THEN** delivery of that agent's work stops before the target moves,
  naming the gate

#### Scenario: A failing first command fails the step

- **WHEN** a Windows setup step `pnpm install; Copy-Item .env.example .env`
  runs and `pnpm install` fails
- **THEN** the step is reported failed

#### Scenario: A step spelled for both platforms

- **WHEN** `setup` declares `posix: pnpm install && cp .env.example .env` and
  `windows: pnpm install; Copy-Item .env.example .env`
- **THEN** each platform runs its own spelling, and the placed checkouts are
  equivalent
