# Decisions

## D1 — One carry for every renamed record, called where records are reached

- status:   overturned
- task:     2.1, 2.2
- commit:   b74ab43f
- trigger:  the design ran each move from `MutationLock::acquire` or `record::ensure`; read-only surfaces (status, the workspace's first draw) read records without either
- decided:  each old name is a `retired(..).renamed_to(builder)` entry; one `uze_core::earlier::carry_renamed(home, project)` renames where the new name is free, run by `UzeApplication`'s constructor and `MutationLock::acquire` (machine) and by `record::ensure` and the three project readers' path helpers (project)
- rejected: one carry per record at its own reader — the rename table would be spelled again at every reader
- blast:    revert the commit; a home already carried keeps the new names, which this build's predecessor does not read

## D2 — Old lock names are retired but never swept

- status:   overturned
- task:     2.1
- commit:   b74ab43f
- trigger:  the design swept renamed locks (`Until(beta.22)`)
- decided:  each old lock name is a retired, unswept entry, `Unbounded`
- rejected: sweeping them — the installed beta.22 binary on the same machine takes them by name, and unlinking a held lock hands the next caller a fresh file (the `persistence-tiers` rule)
- blast:    none; the files are empty

## D3 — A theme override that does not resolve is left out, not fatal

- status:   accepted
- task:     6.3
- commit:   5555dbb3
- trigger:  the spec scenario says the active theme draws without the broken override; the code failed the whole stack
- decided:  `resolve` names `config.toml [appearance]` when only the overrides fail, and `chosen` draws the theme without them, reporting the problem on every run
- rejected: keeping the old all-or-nothing behaviour — a typo in one glyph then costs the operator their whole theme
- blast:    revert the commit

## D4 — The upgrade journey's release-reopen check reads this build's record

- status:   accepted
- task:     4.5
- commit:   b74ab43f
- trigger:  after the carry, beta.22 reopening finds no `agents.json` and adopts the checkouts under ids of its own, so the check reading its `agents.json` failed though both checkouts stayed with their work
- decided:  the check reads `tasks.json` (this build's record) for the agent's own checkout and asserts it is still there; the claim — the release neither collects nor takes it — is unchanged
- rejected: keeping `agents.json` for the release to read — the rename exists to retire that name
- blast:    the journey file only

## D5 — Loose families of an earlier layout are removed by their tier's collector

- status:   accepted
- task:     3.1, 3.2, 3.3
- commit:   8ac036d0
- trigger:  the retired collector removes only concrete paths, and compiled views, package copies and logs left by an earlier release are families (`{digest}.json`, `<package>/`, `gate-<agent>.log`)
- decided:  each family is a retired entry, and the collector that owns its tree removes it through an `earlier` module (`application/earlier.rs` for views and package copies, `src/earlier.rs` for logs); package copies stay while a receipt names one
- rejected: a pattern-aware retired collector — it would need each family's rule (a receipt keeps a copy, a log is any prefix), which only the owner knows
- blast:    revert the commits; the families stay on disk until removed by hand

## D6 — The home is carried once, when nothing of the floor release can be running

- status:   accepted
- task:     8.1
- commit:   4a82b884, 19f52bea
- trigger:  the updater renames the new binary over the old one and restarts nothing, so an open beta.22 client reads `agents.json` while any new `uze` in a pane moved it; reproduced with the real binaries, the client started a second record and the agents in it were no longer read after the restart
- decided:  `carry_all` runs when a client starts and the server says none is attached (the restart), or a command sees no workspace running; it skips a record whose earlier lock is held, and moves every project's records at once and removes the earlier locks, markers and the release answer. Until then `UzeHome`'s builders follow a record to where it is, with its lock, and `[memory] native` falls back to `harness`
- rejected: carrying where each record is read (what caused it); keeping both names until the next release (leaves files on every machine); staging the binary until the restart (beta.22's updater, already published, installs beta.23)
- limits:   during the window a new `uze` draws without theme overrides still in `theme-overrides.json`; a record under neither name (consent, an approval) that a new `uze` and the open client each create first ends under both names, reported by `doctor`; of the record writers, only tasks and prompts recheck their path under the lock
- blast:    revert the commit; records go back to being carried where they are read
