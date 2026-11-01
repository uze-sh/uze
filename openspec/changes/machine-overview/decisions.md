# Decisions — machine-overview

## D1 — Tokens are estimated by a calibrated heuristic, not a tokenizer

- status:   pending
- task:     1.3
- commit:   aa1770f7
- trigger:  The design rejected a BPE table; the estimator still needed a rule and a calibration.
- decided:  `inventory::tokens` reads text as runs (a Latin word ≈ 1 token, digits three to a token, punctuation two characters to a token, an ideograph 0.6), calibrated once against `o200k_base` counts computed outside the build; the test holds four samples (English prose, technical prose, Portuguese, Japanese) within 15%.
- rejected: chars/4: 24% off on short descriptions, which is most of what is measured.
- blast:    `machine/inventory/tokens.rs` only.

## D2 — A hook entry is uze's when it runs the wrapper a hook receipt names

- status:   pending
- task:     1.4
- commit:   013b8158
- trigger:  A hook in a harness's shared settings carries no name the receipt's `entry_name` could be compared with.
- decided:  `HookConfigEntry` owns a hook whose command runs the receipt's `wrapper`; the rules engine is claimed by the integration as `Claim::Owned { kind: "rules-engine", selector: <event> }` and listed under one name, `uze-rules`, on every harness.
- rejected: Matching by path shape (`~/.uze/…` in the command): the spec forbids deciding ownership from a path's shape.
- blast:    `assemble.rs::receipt_owns`, each integration's hook reader.

## D3 — Two entries of one configuration file are never each other's duplicates

- status:   pending
- task:     1.5
- commit:   8ab6e5ce
- trigger:  The real machine listed the rules engine's `SessionStart` and `PostToolUse` entries, both `engine` in one `settings.json`, as duplicates.
- decided:  A duplicate is the same kind and name with different content at a different path.
- rejected: Including the event in a hook's name: two hooks of one event running one script would still collide.
- blast:    `assemble.rs::rows`.

## D4 — A directory is a skill only when it holds a `SKILL.md`

- status:   pending
- task:     3.1, 3.3
- commit:   8ab6e5ce
- trigger:  Claude Code keeps `~/.claude/skills/synced/<id>/` (account-synced skills) beside the operator's; read as a skill it was "broken".
- decided:  Every integration lists a directory holding a `SKILL.md`, or a link whose target is gone (which reads as broken).
- rejected: Listing every directory: the harness itself does not read one without a `SKILL.md`.
- blast:    each integration's `skills_in`.

## D5 — Removing what uze did not install types the harness's command into a shell

- status:   pending
- task:     3.5, 4.4
- commit:   d367057c
- trigger:  The spec said "copy"; the workspace already has a gesture for a command uze never runs: `Intent::TypeInShell`, used for a requirement the machine lacks.
- decided:  Enter on such a resource, or a click on its `remove` line, types the command into a new shell, unrun; what uze installed leads to the Marketplace (`Hit::ShowPackage`); a file placed by hand says "delete its file". The spec's requirement and scenario now say so.
- rejected: The clipboard: the manage modal has no clipboard path of its own, and a copied command is one more step than a typed one.
- blast:    `machine.rs::machine_offer`, the spec's read-only requirement.

## D6 — The feature-flag machinery goes with the Overview's flag

- status:   pending
- task:     4.5
- commit:   d367057c
- trigger:  `Feature::Overview` was the only feature; retiring it left `uze_core::features`, `UZE_FEATURES`, `Route::feature`/`badge`, the tab strip's badge and `text::small_caps` with no use. The operator asked for the old Overview's dead code to go.
- decided:  All of it is removed, with the invariant "What a person downloads carries no half-built screen" and its tests; the git history keeps it for the next unfinished surface.
- rejected: Keeping an empty `Feature` enum: its tests and invariant citations need a feature to exist, and an empty mechanism is the dead code asked to go.
- blast:    Restoring `crates/uze-core/src/machine/features.rs`, `Route::feature`/`badge`, `Tab::badge`, `text::small_caps` and the invariant section from d367057c~.

## D7 — A plugin whose directory is gone is one broken row under skills

- status:   pending
- task:     3.1
- commit:   013b8158
- trigger:  The spec makes a missing plugin directory a broken fact, and a plugin is not a kind the tree groups by.
- decided:  Claude Code and Codex list it as one skill-kind row named after the plugin, noted "plugin", broken "plugin directory missing". Antigravity cannot (its record lists no resources), so its missing plugins are not shown.
- rejected: A sixth "plugins" kind for one fact.
- blast:    the two integrations' plugin readers.

## D8 — Tab moves through the Overview's fields and on to the screens' strip

- status:   pending
- task:     4.1
- commit:   d367057c
- trigger:  The fields need a keyboard path, and `tab` toggles between the strip and the screen everywhere else.
- decided:  On the Overview, `tab` walks list → harness → headings → strip (the period field went with D12), `shift+tab` walks back; `←→` turn a field the keys are on; the foot names where `tab` goes next.
- rejected: Bare letters: "every bare letter here acts on a selection" (`uze-keys/src/load.rs`).
- blast:    `machine.rs::machine_next_field`, `management.rs::hints`.

## D9 — A menu of kinds and a table of each, instead of one folded tree

- status:   pending
- task:     4.1
- commit:
- trigger:  On the real machine the folded tree opened on four near-empty rows, and its status column stayed blank wherever nothing was wrong; the operator asked for a side menu and a table per kind, and for a status that always says something.
- decided:  Menu of sections (uze's findings, then every kind, always listed, with counts and a `!` for problems), the chosen section's table with its own search, and the panel. Status reads `✓ ok` where a fact was checked (`OverviewResource::checked`), `—` where nothing could be, `! duplicate`, `✕ broken`.
- rejected: Hiding the status column when empty: it would come and go between sections and harnesses.
- blast:    `src/ui/view/overview.rs`, `src/ui/machine.rs`, the spec's layout and status requirements.

## D10 — Column headings order the table; the keyboard reaches them by tab

- status:   pending
- task:     4.1
- commit:
- trigger:  The operator asked for ordering by column, with chevrons for ascending and descending.
- decided:  A click on a heading orders by it (names and origins from A, tokens from the heaviest, status from what needs looking at); a second click turns it round. `tab` reaches the headings after the period field; `←→` choose a column, Enter turns it. Two symbols join the vocabulary, `sort.ascending` and `sort.descending`, in every glyph set (`▴ ▾`; the thin, centred `nf-fa-angle_up`/`_down` in nerd; `^ v` in ascii); the schema is regenerated.
- rejected: A letter key for sorting: bare letters act on the selection.
- blast:    `machine.rs::{Column, Sort}`, `view/overview.rs::render_headings`, `uze-theme` symbols and glyph sets.

## D11 — Uses are read from the harnesses' records, never counted by a hook

- status:   pending
- task:     7.1
- commit:
- trigger:  The operator asked for uses per skill, agent and MCP server, mostly typed by the person; the Lab showed where each use is recorded on all four harnesses.
- decided:  Read each harness's own session records after the fact, incrementally (stamp and offset per record in `cache/`), taking only names, invoker and time. Codex's MCP uses, recorded only inside the `exec` script, are an estimate shown with `~`.
- rejected: A uze hook per harness: synchronous cost on every turn (Codex's `async` lost 2 of 10 calls; Antigravity has none), delivery and upkeep in four configurations, empty until the day of install, and blind to typed uses in Antigravity and OpenCode. A hook only for Codex's MCP: kept for later, if the estimate proves poor.
- blast:    `IntegrationPort`, `machine/usage`, the four integrations' readers, the Overview's columns and statuses, the spec's privacy requirement.

## D12 — No period: every figure covers all the records a harness keeps

- status:   pending
- task:     7.8
- commit:
- trigger:  On the author's machine 30 and 90 days read the same: Claude Code deletes transcripts after 30 days by default (`cleanupPeriodDays`), so a 90-day period promised a span no record covers. The operator asked whether a period was needed at all.
- decided:  The period field is gone. Sessions and uses count every record a harness still keeps; the header says how many days of records the sessions span (from the oldest record's time), and each resource's panel says when it was last used. `unused` is no use in any record. `SessionCounts` and `Uses` carry a total and the oldest or last time instead of three windows.
- rejected: Keeping 7/30/90: the windows only differ inside the span each harness keeps, which differs per harness, so a window wider than it reads as a claim the records cannot support.
- blast:    `SessionCounts`, `Uses`, `IntegrationPort::session_counts`, the Overview's header, fields and `tab` order (D8), the spec's session and field requirements.

## D13 — The workspace collects in the background and keeps what harnesses delete

- status:   pending
- task:     8.1
- commit:
- trigger:  The operator saw the Overview read the harnesses on every opening, and asked for the counts to survive the harnesses deleting their own records, for a refresh, and for collection that does not need the modal open.
- decided:  The workspace collects on a thread of its own 20 s after it opens and every 15 minutes, under a `state/` lock waited on for at most two seconds; the Overview draws the last collection, says when it was made, collects only past 15 minutes, and offers `refresh` (f5) in its foot. Uses of a record a harness no longer lists move, per record, to `state/machine-usage.json`, a shaped record; a record listed again leaves it and is read live. The header says since when uses are counted.
- rejected: Collecting from the CLI or a hook: cost on paths the Overview promised to leave alone. Summing the archive by day: a record listed again would count twice. Keeping it in `cache/`: deleting it would lose uses no harness keeps any more.
- blast:    `usage::Archive`, `UzeHome::{machine_usage_path, machine_usage_lock_path}`, `collect_machine_overview`, `orchestrator::reads`, the session loop, `machine::SYNC_EVERY`, the architecture test that names who may collect, the spec's scheduling and privacy requirements.

## D14 — A status says one thing, and only what is wrong is coloured

- status:   pending
- task:     7.8
- commit:
- trigger:  Beside `unused`, a green `✓ ok` read as "used" and drew the eye to every row that needed nothing; the marks did not line up.
- decided:  Statuses are `broken`, `duplicate`, `unused`, `in use` (sound and used, `●`, the filled pair of `○`) and `ok` (sound, uses not counted or not known). Only broken and duplicate take a state hue; unused is held back; in use and ok are neutral. The column stands against its left edge, two cells clear of the uses figure. A `mark.in-use` symbol joins every glyph set.
- rejected: Leaving healthy rows blank: D9 asked for a status that always says something.
- blast:    `machine::Standing`, `view/overview.rs::{status_cell, standing_tone}`, `uze-theme` symbols and glyph sets, the design-system skill's colour and alignment rules.

