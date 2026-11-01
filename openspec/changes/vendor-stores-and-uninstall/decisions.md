## D1 — Prove store ownership in the acceptance suite, ignored until delivery moves

- status:   pending
- task:     1.3
- commit:
- trigger:  the task names `tests/integrations/vendor_neutral.rs`, whose tests use in-process fake integrations; only the acceptance suite drives the real binary against the fake vendor CLIs, which is what writes real harness files. A test that "must be red on the current code" also cannot land enabled without breaking every commit's gate.
- decided:  `tests/acceptance/store_ownership.rs::no_harness_artifact_names_outside_its_store`, on a new canonical fixture `store-tie` (a skill, an MCP server and a hook, each naming `${PLUGIN_ROOT}`), marked `#[ignore]` with the task that removes the mark (4.9). Run with `--ignored` it is red today and lists every tie.
- rejected: an in-process test in `vendor_neutral.rs` (it would prove the fakes, not the delivery); landing it red (breaks the gate for the whole run).
- blast:    moving one file between suites; removing the `#[ignore]` at 4.9.

## D2 — Move each marketplace directory whole, not each package

- status:   pending
- task:     2.4
- commit:
- trigger:  the task asks for a per-package rename; the layout being moved is `store/plugins/<marketplace>/<plugin>` beside `store/superseded/<id>`, and `sources/` keeps exactly that layout.
- decided:  `package::sources::carry_into_sources` renames each child of `store/plugins` and `store/superseded` into `sources/` (one rename per marketplace and per superseded revision), never overwriting what already sits on the new side, then removes the emptied old directories. Same guarantees — resumable, nothing copied, nothing overwritten — in fewer renames. It runs from `MutationLock::acquire`, with the lock held, after the fence and before the digest backfill.
- rejected: a rename per package (more renames for the same guarantee); one rename of `store/` to `sources/` (would also carry anything else under `store/`, and `store/` is about to hold the vendor stores).
- blast:    the loop in `carry_entries`.

## U1 — A live agent is named by count

- status:   pending
- task:     5.1
- commit:
- trigger:  the spec says uninstall "names the agent"; the only answer the terminal runtime gives a process that is not an attached client is its usage report, which names no tab, space or agent on purpose (ADR-056: a process inside a pane may ask it).
- decided:  the refusal names what runs by kind and count — `1 agent, 1 open workspace, 2 shells` — from `uze_terminal::usage()`, with `end the agents, then uze workspace stop` as the next step. A runtime holding no pane and no client is stopped rather than refused, since it serves nothing.
- rejected: attaching as a client to read tab labels (it can start a server, and seats a client into a space); widening the usage report with names (reopens what ADR-056 closed).
- blast:    `refuse_while_running` in `src/cli/uninstall.rs`.

## U2 — UZE's marketplaces are withdrawn through a port method, not a receipt

- status:   pending
- task:     5.3
- commit:
- trigger:  a purge must leave "no UZE marketplace" in the vendor registries, and today no receipt names a marketplace registration: `detach_package` never removes one, and the vendor-store work (3.4, 5b.1) has not given it a receipt yet.
- decided:  `IntegrationPort::withdraw_registrations` (default: nothing), implemented for Claude and Codex by `shared::withdraw::marketplaces`, which unregisters each of UZE's marketplaces still registered at the root UZE gave it. A purge calls it once per harness, after that harness's plugin deliveries and before its configuration entries, and only when that harness's CLI is present and nothing of UZE's is left installed there.
- rejected: a marketplace receipt (belongs to the delivery core's 3.4/5b.1, worked in parallel); removing the registration by editing vendor files (the spec forbids it).
- blast:    one defaulted trait method, one shared module, two four-line impls; the fake vendor CLI learnt `plugin marketplace remove`. Once 3.4/5b.1 record the marketplace by receipt, the method can go.

## U3 — The order a purge detaches in, by what each artifact is

- status:   pending
- task:     5.3
- commit:
- trigger:  "readers before material" needs a total order over receipts without the application naming a harness.
- decided:  per harness, by artifact: what the harness discovers and loads (plugin installs, skill and agent copies, links), then the marketplace registrations (U2), then configuration entries (MCP entries, text regions, the rules engine), then hook entries (UZE's Codex groups last), then builds (`IntegrationOwned { kind: "build" }`), then `$UZE_HOME` with every store in it.
- rejected: one generic pass in ledger order (detaches a marketplace before its plugins).
- blast:    `Reads::of` in `uninstall.rs`.

## U4 — A failed detach stops the purge before the binary

- status:   pending
- task:     5.3
- commit:
- trigger:  the design runs the binary step after the receipts; a run whose detach failed and then removed the binary could only be finished by reinstalling.
- decided:  what inspection leaves (drifted, conflicting, blocked, CLI missing) is listed and never stops the run; a detach that fails stops it after every harness was tried, before the links, the binary and the home, and says to run it again.
- rejected: removing everything regardless (the ledger that a rerun reads would be gone with artifacts still in place).
- blast:    `detach_everything`.

## U5 — The binary's provenance is read, not copied

- status:   pending
- task:     5.3
- commit:
- trigger:  the design says the binary's provenance is copied before anything runs; the binary step precedes the removal of `state/`, so nothing removes `install.json` before it is used.
- decided:  the plan reads `state/install.json` under the mutation lock, before any step, and carries it; no second file is written.
- rejected: a copy outside `$UZE_HOME` (a path `UzeHome` does not name, and nothing reads it).
- blast:    `binary_plan`.

## U6 — Where the binary came from, when the installer did not place it

- status:   pending
- task:     5.3
- commit:
- trigger:  "print the command for the way it was installed"; only the installer leaves a record.
- decided:  a binary under `~/.cargo/bin` or `$CARGO_HOME/bin` prints `cargo uninstall uze`; one under a Homebrew prefix prints `brew uninstall uze`; any other is named with "remove it the way it was put there". When the installer's receipt names a binary that exists, that one is removed even if a different `uze` is running, and the running one is named as left.
- rejected: guessing package managers from more paths (a wrong command is worse than none).
- blast:    `removal_command`, `binary_plan`.

## U7 — The default mode asks no vendor CLI

- status:   pending
- task:     5.2
- commit:
- trigger:  the plan's per-harness receipts and inspections only decide anything in a purge.
- decided:  in the default mode every receipt is planned as kept without being inspected, so the plan is a read of the ledger and the home; a purge inspects each receipt live (never from the inspection cache) and detects each harness's CLI without writing the detection cache.
- rejected: inspecting in both modes (seconds of vendor CLI calls to say "kept").
- blast:    `plan_receipt`, `harnesses_to_uninstall`.

## U8 — A background upgrade is seen through the update lock, now held across the install

- status:   pending
- task:     5.1
- commit:
- trigger:  the release check held `state/update.lock` only while asking for the latest release, and installed after letting it go, so nothing could tell an install was under way.
- decided:  a pass keeps its claim through `install_over`, taking it then if it had not asked; `self_update::underway` is that lock being held by another process, and uninstall refuses while it is.
- rejected: a second lock file for installs.
- blast:    `pass` and `underway` in `src/self_update.rs`.

## U9 — `install.sh` gains `--uninstall [--purge]`

- status:   pending
- task:     5.5
- commit:
- trigger:  the task asks `install.sh`'s uninstall to delegate, and `install.sh` had none.
- decided:  `curl … | sh -s -- --uninstall [--purge]` execs `uze uninstall --yes [--purge]` from the installer's directory when the binary is there; otherwise it removes only `state/install.json` and prints the reinstall-then-purge path. `--purge` alone is refused.
- rejected: an environment variable switch (the PowerShell installer already spells it as options).
- blast:    the options block at the top of `install.sh`; `tests/scripts/installer-test.sh`.

## U10 — `install.ps1` still takes back its own `Path` entry, and stops no workspace

- status:   pending
- task:     5.4, 5.5
- commit:
- trigger:  receipts written before this change carry no `path_entry`, so `uze uninstall` cannot know the installer added one; and the installer used to stop the workspace itself, which would end agents uninstall now refuses to end.
- decided:  after `uze uninstall` returns (or when there is no binary) the script removes `$binDir` from the user `Path` itself, which is idempotent; it no longer runs `workspace stop`, so a running workspace makes `uze uninstall` refuse and say how to stop it. The receipt is now written after the `Path` step and records `path_entry` when this run added it or an earlier receipt said so.
- rejected: leaving old installs' `Path` entries behind.
- blast:    the `-Uninstall` block and the receipt step of `install.ps1`; `tests/scripts/installer.Tests.ps1` (not run here: no PowerShell on this machine).

## U12 — Asking whether a workspace runs writes nothing

- status:   pending
- task:     5.1
- commit:
- trigger:  `uze_terminal::usage()` created `state/terminal/workspace.lock` when no server had ever run, so a declined or refused uninstall changed the machine ("nothing on the machine has changed").
- decided:  `workspace_is_claimed` answers "not claimed" when the lock file does not exist, instead of creating it to ask.
- rejected: skipping the check when no socket exists (an older build's server listens elsewhere, which the claim is what finds).
- blast:    one early return in `uze-terminal`'s `runtime/lock.rs`.

## U13 — The marketplace registrations in the fake vendor CLIs

- status:   pending
- task:     5.7
- commit:
- trigger:  the fake Claude and Codex CLIs knew no `plugin marketplace remove`, so a purge could not be shown to leave no UZE marketplace.
- decided:  `VendorMarketplace` answers `plugin marketplace remove <name>` the way the vendors do: the registration goes, and with it every plugin installed from it.
- rejected: asserting only on UZE's report.
- blast:    `crates/uze-testkit/src/fake_harness/stand_in.rs`.

## U11 — Scenarios that need the delivery core land ignored, naming the task

- status:   pending
- task:     5.7
- commit:
- trigger:  three scenarios can only pass once groups 3–4 move delivery into the vendor stores: every harness still loading after the default uninstall, no harness file naming the home after a purge (OpenCode's `uze.ts` has no receipt yet, so a purge leaves it), and an interrupted purge on OpenCode. Each is red today when run with `--ignored`.
- decided:  each is written now and marked `#[ignore]` with the task that removes the mark (4.9, 4.3/4.6, 4.3), the way D1 did for 1.3. The operator's-Codex-hooks scenario already holds and runs.
- rejected: weakening them to what passes today.
- blast:    three `#[ignore]` lines in `tests/acceptance/uninstall.rs`.
