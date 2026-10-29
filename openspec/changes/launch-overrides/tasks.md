## 0. Remove Profiles

- [x] 0.1 TUI: remove `src/ui/view/profiles.rs`, `Route::Profiles`, `ProfilePanel`/`ProfileColumns`, the `NewProfile` overlay, `DeleteProfile` confirmation, profile hits, intents and worker paths, and `src/ui/tests/profiles.rs`; move navigation and badge tests that sample `Route::Profiles` to `Overview`
- [x] 0.2 Keys: remove `NewProfile`, `DeleteProfile`, `ApplyProfile`, `PreviewProfile`, `ToggleProfileHarness`, the `Profiles`/`ProfileEditor` scopes and their defaults; give `NextValue`/`PreviousValue` a new control in `tests/architecture/affordances.rs`
- [x] 0.3 Application: remove `profile.rs`, the `Profiles` service, its re-exports, `MachineSnapshot.profiles`, `preference_adapters` (fold `new_with_runner_and_preferences` into `new_with_runner`); drop `"profiles.list"` and `"Profiles"` from the tracing and instrumentation tests
- [x] 0.4 Integrations: remove each `preferences.rs`, `shared/preference.rs`, the `PreferencePort` impls, Antigravity's `preferences_config_path`, the registry's preference adapters and `RegistryParts`; drop helpers in `json_config`/`toml_config` left without callers
- [x] 0.5 Core: remove `preference.rs`, `profile_state.rs`, the profile errors, `UzeHome::profiles_path` and its tier-test entry; `Feature::Profiles` (tests move to `Overview`)
- [ ] 0.6 Remove `state/profiles.json` once where the application retires what earlier releases left; test that a machine with one ends without it and nothing else under `state/` changes
- [ ] 0.7 Remove `tests/integrations/preferences.rs` and the journey `01-first-run/03-a-profile-each-harness-can-read.yml`; fix `layout.json` fixtures (`.demo/_shared/workspace.yml`) and docs (`README.md`, `invariants.md` unfinished surfaces, doc comments in `client_layout.rs`, `appearance.rs`, `dialog.rs`, `mark.rs`)
- [ ] 0.8 CHANGELOG: the Profiles screen is gone; keys it applied to harness configuration stay and are the operator's
- [ ] 0.9 `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, and `rg -i 'profile' --type rust` reviewed for leftovers (shell, PowerShell, seatbelt, hook and Cargo profiles are unrelated and stay)

## 1. Catalog contract (uze-core)

- [ ] 1.1 Add `FlagSpec`, `FlagValue` (`None`/`Free`/`OneOf`) and `FlagOwner` (`Operator`/`Weakens`/`Degrades`/`Uze`/`Keyed`) as plain data, naming no harness
- [ ] 1.2 Add `IntegrationPort::flag_catalog()` and `managed_environment()` with empty defaults; unit-test the defaults

## 2. Catalogs (uze-integrations)

- [ ] 2.1 Commit a `--help` snapshot per harness, plus `codex resume --help`, beside each integration
- [ ] 2.2 Claude Code catalog, reusing `session.rs` constants; owned: `--session-id`, `--resume`/`-r`, `--continue`/`-c`, `--fork-session`, `--print`/`-p`, `--settings`; `Degrades`: `--strict-mcp-config`, `--bare`, `--setting-sources`, `--disable-slash-commands`; `Weakens`: `--dangerously-skip-permissions`; managed env: `CLAUDE_CODE_ADDITIONAL_DIRECTORIES_CLAUDE_MD`, `CLAUDE_CONFIG_DIR`
- [ ] 2.3 Codex catalog, with `-c`/`--config` as `Keyed` (always `agents`, `hooks.SessionStart`, `hooks.state`; conditionally `notify`, `features.memories`), `--disable` as `Degrades`; managed env `CODEX_HOME`
- [ ] 2.4 OpenCode catalog (owned `--session`, `--continue`, `--prompt`); managed env `XDG_CONFIG_HOME`, `XDG_DATA_HOME`
- [ ] 2.5 Antigravity CLI catalog (owned `--conversation`, `-i`; `--add-dir` operator, repeatable)
- [ ] 2.6 Unit test per harness: every catalog spelling appears in its snapshot; every Codex catalog flag appears in the `resume` snapshot
- [ ] 2.7 Lab: diff each snapshot against the live binary's `--help` and fail naming the flags that drifted

## 3. Check and record (uze-workspace)

- [ ] 3.1 Line splitter (quotes, backslash, no expansion) and tokenization into variables, flags and values against a catalog
- [ ] 3.2 `overrides::check` with the shared rule (`UZE_*`, `PATH`, `HOME`, `TRACEPARENT`, `TRACESTATE`), spellings (`--f v`, `--f=v`, short, attached), `Keyed` parent-table matching, positional and `--` refusal; plan-less and plan-aware strictness; tests for each spec scenario
- [ ] 3.3 `UzeHome::launch_overrides_path` (`state/launch-overrides.json`) and its lock; `Shaped` record (shape 1) with `agents` (by project, agent, harness) and `recent` (by harness), written `0600`
- [ ] 3.4 Record an agent's overrides per harness; drop an agent's entry when its record goes
- [ ] 3.5 Recent sets: dedupe by normalized tokens, move to top taking the project last used in, cap 20 per harness, this project's sets listed first, withhold secret-looking variables with a count, forget one set
- [ ] 3.6 `LaunchPlan.overrides`, filled in `plan_with` from the same load that verifies the claim; refused or now-owned entries left out with a note
- [ ] 3.7 Spans carry the count of overrides only; a test that no token reaches a span field

## 4. Launch (shim and application)

- [ ] 4.1 Shim splices override flags between contribution args and the trailing turn, variables before `UZE_SHIM_*`/`extra_env`; note on a `Weakens` launch
- [ ] 4.2 Under `UZE_BYPASS`, or for a claim `owner_of` refuses, launch without overrides and say so once
- [ ] 4.3 Application: catalog and recent-sets read models, a check service, launches and resumes that take overrides; `Continue in` launches with the target harness's recorded set (none on first switch) and says the earlier ones were left behind
- [ ] 4.4 Bare-binary launch (no shim): the client appends flags to argv and variables to the launch environment
- [ ] 4.5 Integration tests per harness: final argv order with resume, with first turn, with `memory_off`; continuity still resumes with overrides present

## 5. Popup (src/ui)

- [ ] 5.1 Read `design-system` first; new RootPicker-scoped `Action` for overrides (default chord decided by the operator), never `ctrl+o`
- [ ] 5.2 `overrides` field row under `harness`, summary cut at its end, `›`; click or key opens the step
- [ ] 5.3 Overrides step in the list's place: `‹ overrides` heading as target, esc back, popup height fixed at open as the larger of both steps
- [ ] 5.4 Typed line with caret scope (`←→` caret), entries with `x` in `mark::close_tone`, Backspace on empty removes last, refused token stays in the line in the danger hue
- [ ] 5.5 List: recent sets (nothing marked), catalog completions, `OneOf` values after a flag and a space; fixed-height detail line; `Weakens`/`Degrades` marked with `Symbol::Warning`
- [ ] 5.6 Draft per harness inside the popup; applying a recent set replaces the entries; forgetting a set from its row
- [ ] 5.7 Foot hints for both steps with click == key tests; empty, narrow-width and light-mode `TestBackend` tests
- [ ] 5.8 The agent's overview names its overrides under its harness

## 6. Close

- [ ] 6.1 `design-system` skill: write the rules this settled (field row opening a step ends in `›`, popup height fixed across steps, history not prefilled)
- [ ] 6.2 `docs/architecture/invariants.md`: overrides never displace what UZE composes, tied to the check's tests
- [ ] 6.3 `make check` and `openspec validate --all --strict` green
