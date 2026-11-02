## 1. Preferences in config.toml

- [x] 1.1 `uze-core::config`: scalar values of a table, read and written as JSON values
- [x] 1.2 `uze-workspace::extensions::preferences` over `[preferences."<id>"]`; secrets' set marks in their own record; `UzeHome` paths
- [x] 1.3 `uze-application`: declared preferences through it; built-in preferences by id and declarations

## 2. Built-in preferences

- [x] 2.1 `BuiltinExtension::preferences`; the review declares `mark-viewed-automatically`
- [x] 2.2 The Extensions drawer lists and edits a built-in's preferences
- [x] 2.3 The workspace configures built-ins when it opens and when they change

## 3. The review marks viewed

- [x] 3.1 Track the end seen and the time shown; mark as the file is left or the review closes
- [x] 3.2 Tests for each scenario

## 4. Proof and docs

- [x] 4.1 Docs and the extensions rule say where preferences are kept
- [x] 4.2 `make check`, `openspec validate --all --strict`
