## 1. Revise the open worktree-policy change in place

- [x] 1.1 In `add-portable-worktree-policy/specs/worktree-policy/spec.md`, rewrite "Existing checkouts are adopted at startup": reconciliation records checkouts on sight as `checkout-accounting` states and no longer adopts an unrecorded one; the legacy scenario keeps `agent-<n>`.
- [x] 1.2 In the same spec, widen "A worktree the system did not create is not a slot" to "without UZE's record, wherever it is"; make "Isolated checkouts are reusable slots" and "Nothing that can hold work is removed automatically" say a checkout in use is never reused or removed, and that derived content is not work.
- [x] 1.3 In the same spec, except a subagent's checkout joined into its own agent from "Sibling tasks share work only through the target", and add that an agent with unjoined children is not delivered.
- [x] 1.4 In the same spec, change "The declaration is projected without triggering foreign isolation" so subagent isolation is stated through the `work` verbs, not a Git command.
- [x] 1.5 Add to that change's `tasks.md` a pointer task naming this change as the one that carries these edits out, as 9.5 does for `add-space-kinds`.
- [x] 1.6 In the same spec, replace the removal requirement's "idle beyond a declared age" wording with a pointer to `checkout-accounting`'s spare-slot rule.
- [x] 1.7 Add a row for repository-side files (`.git/info/exclude`, the per-worktree record) to AGENTS.md "What UZE persists".

## 2. The record

- [x] 2.1 Add a shaped `CheckoutRecord` (checkout path, optional parent agent id, optional split commit) read and written in the worktree's Git administrative directory; `rev-parse --git-dir` through `uze-git`, the file with `fs` under the repository write lock; a newer or unreadable record is never written over; a path mismatch reads as no record; the primary is never recorded.
- [x] 2.2 Name the record's file name in `uze-core`'s worktree module.
- [x] 2.3 Record every checkout `create` makes, and rewrite the record for the new holder on every reuse.
- [x] 2.4 L1 tests: a made checkout is recorded; the record survives deleting UZE's state; `worktree remove` and `prune` leave no record; a copied checkout repaired onto the same admin directory is not taken for UZE's; a newer record blocks reuse and removal.

## 3. Recording on sight and classification

- [x] 3.1 On every accounting pass, record checkouts under the isolation directory that a launched agent's record names (non-empty harness) and legacy `agent-<n>` ones; list store-named checkouts without a record as to adopt.
- [ ] 3.2 Replace `registered_checkouts`' parent-directory filter and `CheckoutId::is_uze_made` with classification by record and location: agent slot, subagent checkout, harness isolation, operator's; a recorded checkout outside the isolation directory is foreign. Stop adopting unrecorded checkouts in `reconcile`.
- [ ] 3.3 Add `own_worktree_dirs` to `IntegrationPort` (default none), answer `.claude/worktrees` from the Claude integration, collect them in `uze-application` and pass them to core as data; match under any checkout of the project.
- [ ] 3.4 Conformance: a Claude vertical check that the harness's own worktree lands under the declared directory.
- [ ] 3.5 L1 tests: an upgrade keeps every launched agent's slot; an inferred adoption is not recorded and is listed; a slot an older build makes later is recorded on sight; a harness worktree inside a slot is classified as that harness's.

## 4. Derived dirt

- [x] 4.1 Decide "holds uncommitted work" for park/free only, with the derived exception: a changed `agents.lock` counts unless every plugin in both locks keeps its revision and digest; a changed `AGENTS.md` counts only when it differs from `HEAD` outside the regions UZE manages. Rebase, join and delivery keep requiring a Git-clean tree.
- [x] 4.2 L1 tests for the spec scenarios (entries added or dropped is free, a moved pin is parked, region-only is free, a hand edit beside a region is parked), and that reuse leaves the base's lock and instruction file.

## 4b. Spare slots

- [x] 4b.1 Declare `worktrees.spare` (default 2) and `worktrees.idle_days` (default 3) in the manifest the way `slots` is declared, and add them to the commented template beside it.
- [x] 4b.2 In collection, keep the `spare` most recently used free slots and remove every other free slot's directory; remove any free slot idle beyond `idle`; keep branches; never touch parked or occupied slots. Replace `IDLE_SLOT_AGE`.
- [x] 4b.3 L1 tests for the four spec scenarios.

## 5. In use

- [x] 5.1 Add `machine::process_cwd` for Linux and macOS: skip per-process EACCES/ENOENT, fail closed only when enumeration fails, exclude the caller and its Git children.
- [x] 5.2 Make the terminal server `chdir("/")` at start, with a test that a server started from inside a directory holds nothing there.
- [x] 5.3 Switch `slots`, `take`, `collect`, `remove_idle_slots` and the operator's remove to the probe; keep the pane list for `release_abandoned_tasks` and `parked_with_agent`.
- [x] 5.4 L1 tests: a process sleeping inside a free-looking slot keeps it from reuse and collection; once it exits the slot is free; an unreadable single process is skipped; enumeration failure holds every checkout. 

## 6. Children on the agent record

- [ ] 6.1 Add `parent: Option<AgentId>` to `Agent` (serde default, no shape bump); the child's split commit is its `base_commit`.
- [ ] 6.2 Exclude children from `release_abandoned_tasks`, reconcile's revival, follow-the-target rebases, naming from a commit, readiness evaluation and delivery.
- [ ] 6.3 At a parent's end: release clean children; park children holding work and park the parent with them, keeping its checkout.
- [ ] 6.4 Refuse delivery of an agent with a child holding commits its branch lacks or uncommitted changes, naming the child.
- [ ] 6.7 On accounting, restore a child's `parent` from its record when the store's holder has none and its `base_commit` equals the record's split commit; L1 test simulating an older build's save.
- [ ] 6.5 Prune a released child's branch once it is reachable from its parent's branch.
- [ ] 6.6 L1 tests: a child with no process inside is never offered to a new agent; a parent's end releases clean children and parks the rest with the parent; delivery waits for children.

## 7. The `work` verbs

- [ ] 7.1 `uze agent work split <topic>`: resolve the caller as `work name` does; refuse outside an agent, for an unisolated caller, from inside a child, during a rebase or merge, and at the cap, each with its reason; acquire, materialize, record as a child with the split commit, print the path alone; answer an existing live child for the same topic.
- [ ] 7.2 `uze agent work join <topic>`: refuse over uncommitted changes on either side, a replay paused in the child, a child off its recorded branch, a process other than the caller in the child's checkout, and another agent's child; fast-forward only when the caller's `HEAD` is an ancestor of the child's tip, otherwise `rebase --onto` in the child and `merge --ff-only` in the caller, under the write lock; after a replay, move the child's split commit to the `HEAD` it was replayed onto; on conflict leave the rebase paused in the child, print the paths, exit non-zero; complete on a call after `rebase --continue`; release the child.
- [ ] 7.3 `uze agent work list`: one tab-separated line per child.
- [ ] 7.4 Classify the leaves in `command_performance.rs` (`list` budgeted with its `BUDGETED_COMMAND_TESTS` entry and timing test; `split`, `join` justified slow).
- [ ] 7.5 Acceptance: a hand-made checkout and a `work split` child both survive a new agent launched beside them; split, commit in the child, rebase the parent onto the target, join: the parent gains exactly the child's commits and no merge commit; the child's slot goes to the next agent.

## 8. Projection and the Skill

- [ ] 8.1 Replace the subagent block of the projected worktree-policy region with the three verbs; drop `.worktrees/subagents/` and the Git command; update projection tests.
- [ ] 8.2 Update `plugins/uze/skills/worktree/SKILL.md` to match, and reconcile this repository's `AGENTS.md`.

## 9. The operator's view

- [ ] 9.1 `CheckoutsView` read model in `uze-application` with each checkout's owner and facts, including what holds a slot in use and its size on disk (measured in the background read), and the total.
- [ ] 9.2 Draw a subagent checkout under its parent in the agent column.
- [ ] 9.3 Checkouts view in the space's menu through `spawn_checkouts`/`absorb_checkouts`: open a space, adopt (isolation directory only, with the "becomes free" notice), remove (inspect first, keep the branch), clean up (the operator's class only: clean, unused, in the target; harness isolation left to its harness), and join a parked child into its parked parent.
- [ ] 9.4 `TestBackend` tests for grouping, refusal reasons and the clean-up summary; architecture suite green.

## 10. Gate

- [ ] 10.1 `make check` green; `openspec validate --all --strict` green.
- [ ] 10.2 The operator validates by hand; then a journey in `04-workspace` proving a hand-made checkout survives an agent launch and a split/join round trip, checked against Git and the process table rather than UZE's output.
