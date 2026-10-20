## D1 — A launch takes a space with a shell in the project when none has an agent there

- status:   pending
- task:     3.2
- commit:
- trigger:  The server's first space has only a shell in the launch project, and a launch matched by agent tabs alone would miss it and open a second space on the very next attach.
- decided:  `Session::space_for` looks for a space with a launched tab in the project first, then for one with any tab there; the spec's launch requirement was amended to say so. Caption, recent and the occupancy sweep still count agents only.
- rejected: Remembering which project a space was opened for (a root again, under another name); never counting shells (every launch before an agent ran opened a space).
- blast:    Launch selection in `crates/uze-terminal/src/state.rs` and its tests.

## D2 — "An agent tab" in the runtime is a tab launched with an environment

- status:   pending
- task:     3.2
- commit:
- trigger:  The runtime must tell agent tabs from shells without knowing what an agent is.
- decided:  `Tab::launched` is true when the tab's launch put an environment of the client's into its process, which every agent tab UZE starts carries and no shell does.
- rejected: Reading `Launch::Program` (a shell whose typed harness was restored as a program would count) and naming the identity variable (the runtime would know about agents).
- blast:    One method in `state.rs`.

## D3 — The harness step reads no project

- status:   pending
- task:     1.4
- commit:
- trigger:  The plan had the harness picker describe the chosen project's harnesses, but what the picker shows (set up or not, continuity gap) is the machine's, not a project's.
- decided:  The requirement "The harness picker reads the chosen directory" was removed from `space-groups`; the picker is unchanged.
- rejected: Inventing a per-project harness view to satisfy the requirement.
- blast:    None in code.

## D4 — A directory in no project is its own; only home creates nothing

- status:   pending
- task:     3.7
- commit:
- trigger:  The spec said a launch outside any project creates nothing, but telling "no project" from "a plain directory somebody chose" needs a second resolution rule beside `space_root`.
- decided:  `space_root` already answers a plain directory with itself, so it is a project like any other; a launch from the home directory selects what the client last had (`Seating::WhereItLeftOff`) and a nested one from home opens nothing and says so.
- rejected: A new "is this a project" query in `uze-application` for one call site.
- blast:    `seating_at` and the nested launch in `src/main.rs`.

## D5 — The client layout keeps its shape; the old fold field is ignored

- status:   pending
- task:     3.4
- commit:
- trigger:  The plan had the client layout climb to shape 2, but the layout file carries no shape field and is read best-effort with serde defaults.
- decided:  `collapsed_space_roots` became `collapsed_spaces: BTreeSet<u64>`; a file still holding the old field reads with every space unfolded, which is what a rung dropping it would do. A test pins it.
- rejected: Adding a schema field and a one-step ladder to a best-effort preferences file for a field nothing reads.
- blast:    `crates/uze-workspace/src/client_layout.rs`.

## D6 — The agent's directory is its own tab's, not the task record's

- status:   pending
- task:     1.1
- commit:
- trigger:  The design said the task record's checkout, else the launch directory; the client already reads an agent tab's directory everywhere else (`named_checkout(pane.cwd)`), and a finished agent's respawned shell is no longer an agent tab, so its moving `cwd` is never read as the agent's.
- decided:  The agent in front's directory is its agent tab's `pane.cwd` through `named_checkout`; a shell's directory is never read for it.
- rejected: A second source of the same fact, read from the task store on the UI thread.
- blast:    `WorkspaceModel::note_space_projects` and `agent_directory_in_front`.

## D7 — Put-away overviews stay keyed by space, checked by project

- status:   pending
- task:     2.3
- commit:
- trigger:  The tasks said to key put-away overviews by agent; the resolutions that update a put-away overview (worktree removal, declaration save) carry its space.
- decided:  `put_away_overviews` stays keyed by `SpaceId`; reopening reuses a put-away overview only when its project is the agent's, and starts afresh otherwise.
- rejected: Re-keying every resolution type by agent for no behaviour a person sees.
- blast:    `space_overview.rs`.

## D8 — A directory that is gone resolves to its project by spelling

- status:   pending
- task:     2.7
- commit:
- trigger:  Resolving a missing directory walked up from its parent and answered whatever sat above, `/` included.
- decided:  A directory that does not exist is mapped by `slot_key` alone (a slot to its project, anything else to itself).
- rejected: Dropping agents whose directory is gone from the space's projects (the row that lost its checkout would lose its project too).
- blast:    `project_of_directory` in `src/ui/orchestrator/agents.rs`.

## D9 — "+ space" creates the space and puts it up for its name

- status:   pending
- task:     3.6
- commit:
- trigger:  The spec asks for the name and nothing else; the workspace already has an inline rename on a space's header.
- decided:  The chord and the control send `CreateSpace` with no label and its shell at home, and the client opens the inline rename on the space the session shows, with an empty buffer; a name left blank keeps `space`.
- rejected: A separate name prompt duplicating the rename buffer.
- blast:    `open_new_space` and `name_the_new_space`.

## D10 — `CreateTab` carries its space from the start

- status:   pending
- task:     1.5, 3.5
- commit:
- trigger:  The plan sent `SelectSpace` before `CreateTab` in phase 1 and replaced it in phase 3; the change was implemented in one pass.
- decided:  `CreateTab` names its space; no `SelectSpace` is sent before it, in landing an agent, replacing a closed tab, or opening a space's shell.
- rejected: Writing the phase-1 workaround only to delete it in the same change.
- blast:    Protocol 22.

## D11 — The agent in front is read once per session update

- status:   pending
- task:     2.1
- commit:
- trigger:  Everything that describes a project (badge, surfaces, overview, prefill) needs the agent whose context is in front, and the model holds no harness identities to work it out on demand.
- decided:  `note_space_projects` stores the agent in front's directory alongside the space projects, on every session update; `focused_cwd` and the strip read it.
- rejected: Storing the identities in the model.
- blast:    `WorkspaceModel::agent_in_front`.

## D12 — A space's caption stays under the minimized space in front, and counts its agents

- status:   pending
- task:     2.7
- commit:
- trigger:  The spec captioned every space with its project, while the sidebar draws a caption only under the minimized space in front, a choice the operator made before this change: a path under every name is a second column to read past.
- decided:  The spec now says what the sidebar does: only the minimized space in front has a caption. It counts the space's agents (`2 agents`), which is what folding hid; the operator found the directory there less useful than the count. Rows name their project in a space of several.
- rejected: Drawing a caption under every open space, which reverses that choice.
- blast:    The spec only.

## D13 — An overview answer carries the project it was read for

- status:   pending
- task:     2.6
- commit:
- trigger:  D7 kept overviews keyed by space; one space now holds the overviews of several projects, each counting its reads from its own start, so an answer could land in another project's overview.
- decided:  Every overview resolution (status, worktrees, fact, save, removal) carries its project, and is taken only by the overview of that space and that project.
- rejected: One read counter shared across overviews, which would hide the mismatch rather than name it.
- blast:    The resolution types in `space_overview.rs`, `worktree_removal.rs` and `reads.rs`.

## D14 — A tab asked in another space does not move the client

- status:   pending
- task:     3.1
- commit:
- trigger:  `CreateTab` naming a space moved the asking client into it, so an agent placed after the operator moved on pulled them back.
- decided:  The tab is put in front in its own space; the client is moved only when that is the space it is looking at. Work resumed or isolated is asked for now, so the client follows it with `SelectSpace` wherever it lands.
- rejected: Leaving the jump, which takes the keyboard away mid-typing.
- blast:    `CreateTab` handling in `runtime/server.rs`.

## D15 — The harness is a field, not a step

- status:   pending
- task:     1.2
- commit:
- trigger:  The operator chose a one-step popup: the harness rarely changes, so asking for it after every directory cost a second confirmation in the common case.
- decided:  The popup asks for the directory, with the harness in a field prefilled with the last one used (kept in the client layout) and moved with `←` `→` (first `Tab`, which the operator rejected: `Tab` is how a directory is completed); its foot names its keys only (a sentence saying where and with what crowded them out, the operator found), and rows say `space · <branch>` or `git`.
- rejected: The two steps with a way back, which made the common case two keys and the steps row a second thing to read.
- blast:    `DirectoryStep`, `session/agents.rs`, `render/popups.rs`, the `RootPicker` scope's keys, `WorkspaceLayout.last_harness`.

## D16 — An older release does not restore this build's workspace

- status:   pending
- task:     4.1
- commit:
- trigger:  The nightly upgrade journey opened beta.15 after this build: it cannot read the workspace document's shape 3, so it opens with no tabs, finds this build's agent not running and closes its task.
- decided:  Accepted as the cost of shape 3 (the ladder step 2→3, a one-way door). What still holds, and what the journey now proves, is that the older release neither collects nor takes the checkout named after the agent: the directory and its work stay. The journey's `released:` moves past this change once a release carries shape 3.
- rejected: Writing shape 2 beside shape 3 for older releases, which keeps a space root this change exists to remove.
- blast:    `journeys/suites/07-upgrade/01-the-previous-release-hands-over-its-work.yml`.
