# Decisions — launch-overrides

## D1 — Retire `state/profiles.json` in the seeding every command runs first

- status:   pending
- task:     0.6
- commit:
- trigger:  The only retirement path the application has, `retired_regions`, cleans AGENTS.md regions in a project's main working tree on a context reconciliation; nothing retires a file under `$UZE_HOME`.
- decided:  `UzeApplication::retire_records_of_earlier_releases` removes `UzeHome::retired_profiles_path` at the start of `ensure_default_plugins` (run ahead of every CLI command and by the workspace's startup worker) and of `setup_through`; a missing file is a no-op, so a settled machine pays one failed unlink.
- rejected: Hanging it on `retired_regions`: that runs per project and only on a reconciliation, so most machines would keep the file. A one-off marker recording that the retirement ran: one more record for a file that, once gone, nothing writes again.
- blast:    `setup.rs` (two call sites and the function), `UzeHome::retired_profiles_path`, two tests in `setup_tests.rs`.
