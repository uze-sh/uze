## 1. Grammar

- [x] 1.1 `AddArgs` shared by `Command::Add` and `ShorthandArgs`; `add_package` behind both
- [x] 1.2 `Command::Install` takes only `[path]`, `--trust`, `--format`; a spec or a non-root path is refused with `uze add`
- [x] 1.3 Root help lists `add` and `install`; `add` examples include the shorthand; help spells a positional by its value name
- [x] 1.4 `add` classified in `src/command_performance.rs`; `install`'s reason rewritten

## 2. Callers

- [x] 2.1 Unit and domain tests, `uze-testkit`'s `marketplace_add_args`, telemetry fixtures
- [x] 2.2 Journeys, `.demo/`, `make try`
- [x] 2.3 Conformance scenarios, experiments and `DECISIONS.md`
- [x] 2.4 Docs, site, the official plugin's Skills, READMEs and the marketplace README template
- [x] 2.5 `cli-commands` rule and the `design-system` skill

## 3. Verify

- [x] 3.1 `cargo test` for the CLI, packages, project, acceptance and architecture suites
- [x] 3.2 `python3 journeys/journey.py validate journeys/suites`
- [x] 3.3 `openspec validate --all --strict`
