## Why

The top-level tiers of `$UZE_HOME` hold up, but the files inside them do not
read as a design. Names carry their context twice (`state/machine-usage.json`)
or say the wrong thing (`agents.json` holds tasks, `update.json` holds the
binary's release check); one name sits in two tiers with opposite costs
(`state/machine-usage.json`, `cache/machine-usage.json`); a lock lives under
the swept runtime tree, so `doctor` deletes what a running agent holds and
the reports it wrote. UZE is in beta: this is the moment to make the shape
round, and the `earlier-release-support` change gives every rename one way
to carry a machine across.

## What Changes

- Files under `state/`, `cache/` and `runtime/` take
  one-word names whose directory carries the context. A record and its guard
  share a stem (`tasks.json` + `tasks.lock`); an activity lock is named for
  its activity in its scope's directory. No name is used in two tiers.
  - `state/`: `update.*` → `release.*`, `launch-overrides.*` → `launches.*`,
    `machine-usage.*` → `usage.*`, `marketplace-fetch.lock` → `fetch.lock`,
    `marketplace-mirrors/` → `mirrors/`, `extensions.*` (consent) →
    `extensions/consent.*`, beside the stores it no longer shares a stem
    with.
  - `state/projects/<p>/`: `agents.*` → `tasks.*`, `approved-commands.json`
    → `approvals.json`, `prompt-history.*` → `prompts.*`.
  - `cache/`: `harness_detection.json` → `detection.json`,
    `machine-overview.json` → `overview.json`, `machine-usage.json` →
    `transcripts.json`, `release-notes.md` → `changelog.md`;
    `logs/setup-<harness>.log` → `logs/setup/<harness>.log`,
    `logs/gate-<agent>.log` → `logs/gate/<agent>.log`.
  - `runtime/`: `packages/` → `servers/`, loose compiled extensions →
    `extensions/compiled/`, `projects/<p>/project.json` → `root.json`.
- **Fix:** an agent's reports survive maintenance. `runtime/agents/` becomes
  a runtime tenant with its own collector, and its lock moves to
  `state/reports.lock`, out of the swept tree.
- `theme-overrides.json` is folded into
  `config.toml` as `[appearance.colors]`, `[appearance.symbols]` and
  `[appearance.syntax]`; `[memory] harness` becomes `[memory] native`. The
  root holds `config.toml`, `keys.json` and `themes/`, nothing else.
- `config.toml` gets a written shape: one section per concept, at most two
  header levels, one table per identified thing, vocabulary ids kept as they
  are, no secrets.
- Every rename and move is carried across: each old path is a retired
  entry of the owned map with the step that moves it, naming
  `1.0.0-beta.22` as the last release that wrote it, so a machine at the
  floor keeps its agents, prompts, approvals, settings and overrides. The
  overrides file and `[memory] harness` are carried into `config.toml` once.
- Kept as they are: `mutation.lock` and `config.lock` (both builds on one
  machine must keep sharing them), `agents-md.lock`, `install.json`,
  `keys.json`, `themes/`, the refusal of a newer build's document, and
  `retired_regions`, which cleans operators' repositories, not the home.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `persisted-state`: adds that no name UZE owns stands for two things, and
  that what a running agent reported survives maintenance.
- `agent-finished-chime`: the operator's settings file also holds their
  overrides of the active theme, and reads by concept.

## Impact

- Depends on `earlier-release-support` (the floor, `retired(..).carried_by(..)`
  and the single bootstrap carry), which lands first.
- `crates/uze-core/src/machine/home.rs`, `home/owned.rs`, `config.rs`,
  `appearance.rs`, `machine/harness_runtime.rs`; `uze-theme` gains a
  resolve-from-value entry; `src/theme.rs`, `src/shim.rs`,
  `src/cli/setup.rs`, `src/telemetry.rs`.
- `uze-workspace` (`reports.rs`, `task.rs`, prompt history, approvals,
  consent), `uze-application` (catalogue, collectors, uninstall, disk).
- Journeys and conformance seeds that name the old files; `07-upgrade`
  proves the floor release's home is carried across.
- `.agents/rules/persistence-tiers.md`; `docs/` that name the old paths.
