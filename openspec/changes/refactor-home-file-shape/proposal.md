## Why

The top-level tiers of `$UZE_HOME` hold up, but the files inside them do not
read as a design. Names carry their context twice (`state/machine-usage.json`)
or say the wrong thing (`agents.json` holds tasks, `update.json` holds the
binary's release check); one name sits in two tiers with opposite costs
(`state/machine-usage.json`, `cache/machine-usage.json`); a lock lives under
the swept runtime tree, so `doctor` deletes what a running agent holds and
the reports it wrote. UZE is in beta: this is the moment to make the shape
round, without carrying any earlier layout across.

## What Changes

- **BREAKING (on disk):** files under `state/`, `cache/` and `runtime/` take
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
- **BREAKING (authored):** `theme-overrides.json` is folded into
  `config.toml` as `[appearance.colors]`, `[appearance.symbols]` and
  `[appearance.syntax]`; `[memory] harness` becomes `[memory] native`. The
  root holds `config.toml`, `keys.json` and `themes/`, nothing else.
- `config.toml` gets a written shape: one section per concept, at most two
  header levels, one table per identified thing, vocabulary ids kept as they
  are, no secrets.
- **Removed (beta policy):** every carry-across and retired path:
  `pre_sources_layout` and `carry_into_sources`, `scattered_project_records`
  and `carry_across_the_move`, `retired_profiles_path` and its collector,
  `RETIRED_MARKETPLACE`, the `runtime/attachments/` tree nothing writes any
  more and the code that cleans it. Each `Shaped` ladder starts again at
  its current shape.
- Kept as they are: `mutation.lock` and `config.lock` (both builds on one
  machine must keep sharing them), `agents-md.lock`, `install.json`,
  `keys.json`, `themes/`, the refusal of a newer build's document, and
  `retired_regions`, which cleans operators' repositories, not the home.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `persisted-state`: during beta a document from an earlier shape is not
  carried across and a shape change is not a step; the release-upgrade
  proof applies from the first stable release; adds that no name is shared
  across tiers and that what a running agent reported survives maintenance.
- `agent-finished-chime`: the operator's settings file also holds their
  overrides of the active theme, and reads by concept.

## Impact

- `crates/uze-core/src/machine/home.rs`, `home/owned.rs`, `config.rs`,
  `appearance.rs`, `delivery/persistence.rs`, `package/sources.rs`,
  `project/record.rs`, `machine/harness_runtime.rs`; `uze-theme` gains a
  resolve-from-value entry; `src/theme.rs`, `src/shim.rs`,
  `src/cli/setup.rs`, `src/telemetry.rs`.
- `uze-workspace` (`reports.rs`, `task.rs`, prompt history, approvals),
  `uze-application` (catalogue, collectors, setup, uninstall, disk),
  `uze-integrations` (attachment cleanup, `RETIRED_MARKETPLACE`).
- Journeys and conformance seeds that name the old files; the
  `07-upgrade/01` journey is suspended until the first stable release.
- `.agents/rules/persistence-tiers.md`; `docs/` that name the old paths.
- An existing home keeps its old files unread: `doctor` reports them as
  unknown entries, and the operator deletes them or resets the home.
