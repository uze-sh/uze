## 1. Phase 1: the directory step and where an agent lands

- [x] 1.1 Add `agent_in_front` (the agent whose context is in front, its tab or a shell opened beside it, and the directory it was placed in: the task record's checkout, else the tab's launch directory, never the live `pane.cwd`)
- [x] 1.2 Add the directory step before the harness picker in `session/agents.rs`, reusing `root_picker`'s listing, prefilled with `agent_in_front`'s project, else the space's most recent agent's project, else home; cancelling either step places nothing
- [x] 1.3 Draw both steps in one popover anchored beside the space's `new`, over the pane (`render/popups.rs`): the directory listing first, then the harnesses in the same popover at the same place and size, up to ten rows, scrolling inside with the standard `Scrollbar`; nothing in the sidebar moves; Esc closes it
- [x] 1.4 Carry the chosen directory into `PlacementRequest::New { from }`
- [x] 1.5 Replace `land_agent_in_its_own_space` with D4's rule; the pending placement carries the origin `SpaceId`, and the client sends `SelectSpace` for it before `CreateTab`
- [x] 1.6 Record prompts by the agent's project (`agent_in_front`'s directory through `slot_key`) instead of `space_root_of_pane` (`activity.rs`)
- [x] 1.7 Tests in `src/ui/orchestrator/tests/`: one space holding agents of three projects; isolating keeps the space; an agent opens in its origin space after the operator selected another; resuming lands in the space working in the project, else the space in front; no placement sends `CreateSpace`; a prompt to an agent of `back` in a space whose root is `front` is recorded under `back`
- [x] 1.8 Run the design-system skill's checklist on the popover's two steps and their hint bar, with a click == key test
- [x] 1.9 Leave nothing phase 1 retired: `land_agent_in_its_own_space`, `pending_agent_tab`, `land_pending_agent_tab`, `space_rooted_at` and the root picker's drawing over the column (`render_root_picker` in `render/sidebar.rs`) are deleted with the tests that only proved them; `cargo clippy --all-targets -- -D warnings` is clean

## 2. Phase 2: project actions and the overview belong to the agent

- [x] 2.1 Offer the strip's work status and the spec, arch, code and agent buttons only while `agent_in_front` answers; `focused_cwd`, `open_architect`, `open_spec` and the code surface read its directory; selecting the space's row or a shell of the space closes an open surface (`answers.rs`, `surfaces.rs`, `render/tab_strip.rs`)
- [x] 2.2 The management modal's project is `agent_in_front`'s, else the space's most recent agent's, else none (`session/manage.rs`)
- [x] 2.3 The overview opens on `agent_in_front`: the `space` button becomes `agent`; the header reads `Agent`, the agent's label and directory; selecting another agent turns it, leaving the agent's context puts it away; `put_away_overviews` is keyed by agent (`render/tab_strip.rs`, `space_overview.rs`, `render/space_overview.rs`)
- [x] 2.4 The Agent section lists only the agent's prompts, filtered by `PromptEntry::agent`, with no "where" column and no space-wide toggle; drop the count of agents by state and its read; an agent outside a project shows the Agent section alone
- [x] 2.5 `uze-workspace`: clear one agent's prompts (rewrite the project's file without its lines, under the compaction lock), exposed through `uze-application`; the clear question names the agent (`overview_question.rs`)
- [x] 2.6 The overview's reads are keyed by agent, dropping answers for an agent no longer shown and keeping the last answer drawn until the new one lands
- [x] 2.7 Compute the projects each space works in (D3, agent tabs only) when a session update or an evaluation is absorbed, never while drawing; the sidebar captions a space with its project, or the number of projects, or nothing, and names each agent's project in a multi-project space (`render/sidebar.rs`)
- [x] 2.8 Recent: `projects_to_read` and the landing read the projects spaces work in; the selected row names the space resuming lands in (`recent.rs`, `render/recent.rs`)
- [x] 2.9 Occupancy sweep: `look_in` extends with the projects agent tabs work in, never a shell's directory (`tabs.rs`)
- [x] 2.10 Tests: a space's shell inside a repository shows no status or surface buttons; a shell beside an agent that `cd`s elsewhere keeps the agent's status; the overview lists one agent's prompts and not another's of the same project; clearing forgets one agent's prompts only; selecting another agent turns the overview, selecting the space's row puts it away; the agent button is absent on a space's row; caption and row naming for none, one and three projects; a shell `cd`'d into a project does not make the space work in it; worktree removal refuses a slot a pane of another space stands in
- [x] 2.11 Leave nothing phase 2 retired: the space-wide prompt toggle, the "where" column, the agents-by-state count and its read, `AgentSupportDropdown::space_root`, `prompt_history_pending` keyed by a space root, `space_directories` and `space_open_on` are deleted, not adapted, with the tests that only proved them

## 3. Phase 3: the runtime's space model and its persistence

- [x] 3.1 `uze-terminal`: remove `Space.root`; `CreateSpace` takes a label and its first shell's directory with no dedupe; `CreateTab` gains `space` and opens in it, and its `cwd: None` opens in home; the server's first space is created from a label and a directory (`terminal serve`); bump `PROTOCOL_VERSION`
- [x] 3.2 `uze-terminal`: `open_space` selects a space with a `Launch::Program` tab whose `cwd` is the launch project or below it, else creates one whose first shell opens there; `SpaceSeat` is replaced by that request
- [x] 3.3 `uze-terminal`: `SpaceSeed` keeps the space's id and the persisted workspace keeps `next_space_id`; shape 3 with a ladder step from 2 (root dropped, ids assigned in order, `next_space_id` past them)
- [x] 3.4 `uze-workspace`: `client_layout` keeps folded spaces as `BTreeSet<u64>` (shape 2, the step from 1 drops the folded set), pruned of ids no space has
- [x] 3.5 Client: drop the `SelectSpace` before `CreateTab` from 1.5; a shell opened with no agent in front opens in the agent in front's directory, else home; `CloseSpace`'s replacement is a space with a shell in home
- [x] 3.6 Creating a space asks for its name only, its first shell in home; the new-space chord opens that prompt
- [x] 3.7 Launch and nested launch (`src/ui.rs`, `orchestrator.rs` `seating_at`, `src/main.rs`): a project selects or creates by 3.2; home or no project selects `WhereItLeftOff` and creates nothing; the nested launch reports the space
- [x] 3.8 Leave nothing phase 3 retired: `SpaceSeat`'s root, `space_label(root)`, `unrepeated_label`, `space_cwd`, `space_own_tab`'s root fallback, `space_roots`, `space_root_of_pane`, `home_seat` as a root, `collapsed_space_roots`, the root picker's new-space mode, `seating_at`'s home rule and the overview's `RootGone` / `NotAProject` named for a root; the only mention of a space root left is the ladder step that drops it
- [x] 3.9 Tests in `crates/uze-terminal`: two spaces over one directory; a space survives a restart with its id; a closed space's id is not reused after a restart; shape 2 climbs to 3; a launch selects by an agent tab's containment, never by a shell's and never by a space above; a launch from home creates nothing; `CreateTab` with `space` lands there without moving another client's selection

## 4. Journeys and verification

- [x] 4.1 Update `journeys/suites/04-workspace/05-a-picked-checkout-opens-its-project.yml` and `07-a-repository-the-prompt-has-to-walk-to.yml` to the directory step, and add a journey for one space holding agents of two projects (after the operator validates by hand)
- [x] 4.2 No dead code: `grep` for every name 1.9, 2.11 and 3.8 retired finds only the ladder step; no `#[allow(dead_code)]`, no unused parameter, no compatibility branch for the old shape outside `uze-document`'s ladder; then a `code:polish` pass over the branch, fixing what it confirms
- [x] 4.3 `make check` green: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, `openspec validate --all --strict`
- [x] 4.4 `make install` and validate by hand: a space `storefront` with agents in `front`, `back` and `landing`, a restart, a `uze` launched in `back`, and one launched in home
