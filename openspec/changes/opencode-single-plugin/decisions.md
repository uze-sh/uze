## D1 — Hand the plugin the Rust dialect as data instead of a hand-written copy

- status:   pending
- task:     2.3, 2.4
- commit:
- trigger:  D5 planned a second implementation of the OpenCode dialect in TypeScript, held to Rust only by a shared fixture table.
- decided:  `dialect::to_json` serializes `OPENCODE_AGENT_DIALECT` (each known field and its shape, `carriesUnknown`) into the section's source, and the TypeScript is a generic interpreter of those shapes. Which fields are carried is decided in one place; the fixture table (`the_section_registers_exactly_what_the_rust_reading_projects`) holds the interpreter, the file walk and the conversion into `Agent.Info`.
- rejected: a hand-written field list in TypeScript: a field added to the Rust dialect would reach package agents and silently not project agents.
- blast:    one function; reverting puts the field list back in the template.

## D2 — Watch each directory of the agents tree, plus the two shallow parents

- status:   pending
- task:     2.3
- commit:
- trigger:  `fs.watch(root, { recursive: true })` on Bun (1.3.12, Linux) reports only the root's own entries, never a subdirectory's, and the agents walk descends.
- decided:  one non-recursive watch per directory under `.agents/agents`, re-synced on every event, plus shallow watches on the checkout (filtered to `.agents`) and on `.agents` (filtered to `agents`), so a directory created after setup is picked up.
- rejected: a recursive watch (misses nested agents); watching the whole checkout recursively (every build output would reload the registry).
- blast:    the section's `sync`; reverting loses nested and newly created directories until the service restarts.

## D3 — `Blocked` names no file in the read model; the context shortfall does

- status:   pending
- task:     2.6
- commit:
- trigger:  `ContextMechanism` is `Copy` and drawn as one word; the spec asks `uze doctor` to name the file holding the plugin's path.
- decided:  `SetupShortfall::Blocked(path)` carries the path in the core; `ContextMechanism::Blocked` and `UndeliveredReason::Blocked` do not. `uze doctor` names the file through the agent-context shortfall, which now reads the same plugin state (`runtime_plugin::shortfall`).
- rejected: a path inside `ContextMechanism` (loses `Copy` for every drawn row, and the drawer has no room for a path).
- blast:    none beyond the enum variants.

## D4 — Converge an earlier release's hook plugin at that package's attach, not at setup

- status:   pending
- task:     3.5
- commit:
- trigger:  D6 had setup delete every `hooks-*.ts` carrying the bridge header, but a package's document exists only once that package is attached again: deleting first leaves its deny groups unrun until then.
- decided:  `attach_hook_bridge` writes the package's document and takes out that package's `plugins/hooks-<package>.ts` (when it carries the header) only once `uze.ts` is UZE's, since until then the old file is what runs the groups; setup then takes it out for every package with a document. Inspection still reads such a file (`bridge_carries_groups`) for a receipt that names it. Detaching one of its groups while others remain is Blocked until setup, the last group's detach removes it, and a receipt whose file a sibling's detach took out follows its groups into the document. Setup deletes `uze-agent-context.ts` and `uze-rules.ts` only when it writes `uze.ts`; an operator's `uze.ts` leaves them, and refuses the hooks and rules that would ride it.
- rejected: deletion at setup (a window with no deny hooks); keeping both (a package's groups would run twice).
- blast:    `retire_package_bridge`, `retire_bridges_with_documents`, `hook_place` and the legacy branches of inspection and detach; reverting brings back the window.

## D5 — The rules receipt names the runtime plugin and holds its section

- status:   pending
- task:     3.4
- commit:
- trigger:  the rules engine's receipt (setup-owned) carried the whole `uze-rules.ts` as `expected`; the runtime plugin is now written by `install`, not by the rules delivery.
- decided:  `RulesDelivery::Plugin` keeps one receipt whose `place` is `plugins/uze.ts` and whose `expected` is the rules section; attach writes only the engine; inspection is Matched while the plugin contains the section, Missing otherwise; detach removes the engine and leaves the plugin to setup.
- rejected: a second writer of `uze.ts` inside the rules delivery (two writers of one file); dropping the receipt (`rules_engine_delivered` reads it).
- blast:    the plugin arm of `rules.rs`.

## D6 — The hook documents live under the attachment root, beside the wrapper

- status:   pending
- task:     3.1
- commit:
- trigger:  task 3.1 asked for a `UzeHome` entry.
- decided:  `hook-groups/` under `shared::path::attachment_root(home, "opencode")`, the same root the other harnesses' `hooks/exec` and the rules engine use: `UzeHome` already names it (`generated_attachments_dir`), and the layering test accepts a child composed from it.
- rejected: a new `UzeHome` method for one integration's child directory.
- blast:    none.

## D7 — The Lab scenes are a vendor phase promoted from the experiment

- status:   pending
- task:     4.2
- commit:
- trigger:  the cross-package order, a document corrupted mid-session and a package installed while the service runs are OpenCode's alone; the contract names no vendor.
- decided:  `experiments/opencode/hooks-cross-package` asserts them on the shared service, and `harnesses/opencode/scenarios.py` runs it as the `runtime-plugin` phase, the way `skill-files` is promoted.
- rejected: contract checks every harness would have to declare unsupported.
- blast:    one phase of the vertical.
