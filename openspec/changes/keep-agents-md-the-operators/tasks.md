## 1. The launch section

- [x] 1.1 `WorktreePolicy::instructions` takes the agent's situation (isolated or in place, slot directory, branch, target, completion) and renders only the bullets that apply, naming clause first. Drop the "this section is for an agent `uze workspace` launched" opening. Unit tests per situation, plus byte-stability across two renders
- [x] 1.2 `agent_context::compose` gains the instructions part ahead of the notes preamble (D2). The preamble's wording is unchanged and introduces only the sections after it. Test the order, and that a context with instructions and no notes still composes
- [x] 1.3 Read the branch from the checkout at compose time, never from the slot name, so a resume after `uze agent work name` states the new branch. Test: name, resume, assert the section
- [x] 1.4 The launch composes the section for every launch and resume the workspace owns, through each harness's existing channel, and for no launch it does not own. Integration tests for an isolated launch, an in-place launch, and a shim launch outside the workspace (no section)
- [x] 1.4a Antigravity (D2a): write the instructions part as an `always_on` rule in the agent's context directory, and keep the notes on the `PreInvocation` hook. Lab: `systemInstruction` on launch, resume and `invoke_subagent`
- [x] 1.4b OpenCode (D2b): the `context` plugin falls back to the parent session's context through `ctx.session.get(...).parentID`. Lab: a child session receives the instructions
- [x] 1.5 Degraded channel: compose `context.md` on every launch whatever the channel (today a degraded launch composes nothing). A launch that starts a new conversation on a degraded or unavailable channel gets one first turn pointing at `context.md` (beside today's switch-briefing first turn, never two). A resume gets none. Both are reported as degraded. Unit tests in `continuity.rs` for new, resumed and switched launches
- [x] 1.6 `uze agent work split` prints, beside the path, the two lines a parent hands to its subagent: commit in this checkout, and never switch the branch or make a worktree

## 2. Shares of the bound

- [x] 2.1 Add the instructions share and reduce the briefing share as in D4. Keep `the_shares_add_up_to_the_bound`, and add a test that a full briefing and an overfull memory never cut the instructions
- [x] 2.2 Test that the composed document stays at or under `BOUND` with multi-byte text in every section
- [x] 2.3 Measure the largest briefing in the Lab's recorded runs and the rendered policy for the longest declared vocabulary. If 4,000 bytes cuts a real briefing, shorten the policy text, never raising `BOUND`

## 3. Stop writing AGENTS.md

- [x] 3.1 Remove the policy region sync at launch (`services/policy_region.rs`), the client's "AGENTS.md's workspace section" warnings, and the policy-region read in `src/ui/orchestrator/reads.rs`. The context popup keeps showing the policy in force and its provenance
- [x] 3.2 Remove `package::authoring::region` and its reconcile step
- [x] 3.3 Retire the `project:worktree-policy` and `project:plugin-authoring` prefixes: in the primary checkout, `uze install` and `uze agent context reconcile` remove a region whose body matches the digest in its identity, report one that does not, and attach nothing. In an isolated checkout they remove nothing. Tests: both regions removed byte-exactly around the operator's text, an edited region kept and reported, an isolated checkout left unchanged and free, a project without `AGENTS.md` left without one
- [x] 3.3a Replace `PolicyRegionView`'s `in_step`/`state` and `project_environment.rs`'s "region behind" read model with one "retired regions present" report naming `uze install`, in `uze status` and the client
- [x] 3.4 (after 4.1, decisions.md I3) `checkout-accounting`: drop the managed-region discount and keep the lock discount. Update `accounting_tests.rs` and `shelf_tests.rs` to the new scenarios
- [x] 3.5 (with 4.1, decisions.md I5) Architecture suite: a rule that no production code outside the retired-prefix cleanup writes `AGENTS.md`

## 4. Plugin instructions become rules (after `plugin-rules` ships)

- [x] 4.1 Remove the package `AGENTS.md` resource from `project::context`, and retire the `package:` prefix: removed structurally with `text_region::remove_unconditionally`, primary checkout only
- [x] 4.1a When the last package region goes, remove the `instruction-bridge` region of `CLAUDE.md` and delete a `CLAUDE.md` that held only it. A no-op where `resolve-instruction-files` already removed the bridge. Tests for a bridge-only file and a file with operator lines
- [x] 4.2 `uze agent plugin create --instructions` fails, naming `--rules` with an `always_on` rule. Remove `authoring/instructions.md`
- [x] 4.3 `uze agent plugin check` reports a plugin-root `AGENTS.md` as not delivered and names the replacement
- [x] 4.4 `plugins/uze/plugin.json`'s description and any official plugin resource that relied on instructions

## 5. Conformance

- [x] 5.0 Precondition, fixed on its own before this change: Antigravity takes the first path in `workspacePaths` (sorted by path) as its working directory, so the agent's `--add-dir` context directory must never sort before the checkout. Add a Lab check that the "Command Working Directory" is the checkout
- [ ] 5.1 Contract scenario per harness: an agent given the policy only through the launch context runs `uze agent work name` as its first command, and the checkout has no UZE region
- [ ] 5.2 Contract scenario per harness: an isolated agent asked to parallelize uses `uze agent work split|join` and makes no worktree with Git
- [x] 5.3 Promote the pre-implementation subagent probe to a recorded result per harness (Claude and forked Codex receive the context; Codex `fork_turns: "none"`, OpenCode child sessions and Antigravity `invoke_subagent` do not), with Antigravity's control run without UZE's marker. Record each in the integration's README. A harness whose subagent misses it is documented, not worked around. Recorded on 2026-10-19 (Lab contract `instructions`): Claude, Codex (default spawn, forked), OpenCode (after 1.4b) and Antigravity (after 1.4a) receive the instructions; Codex with `fork_turns: "none"` does not
- [x] 5.4 Antigravity signed out: the instructions arrive in the first turn and status reports the channel as degraded

## 6. Skills, docs and this repository

- [x] 6.1 `plugins/uze` Skills: `init` stops describing UZE regions in `AGENTS.md`, and reports and cleans a project carrying them. `worktree` and `author` drop their references to the projected region
- [x] 6.2 `docs/capabilities/context-manager.md`, `docs/capabilities/uze-skill.md` and `web/content/docs/` (context, installation, glossary, FAQ): what UZE writes into a project, following D1, with no em dashes in user-facing copy
- [x] 6.3 `docs/architecture/invariants.md`: replace every region invariant (the bridge protocol around line 111, "A package's instructions reach only a project that declares it", the managed-region accounting around line 1598, the policy-region identity around line 1862, "Each region of `AGENTS.md` has one owner") with "no UZE operation changes `AGENTS.md` except to remove a retired region in the primary checkout", tied to 3.3 and 3.5
- [x] 6.4 (with 3.4, decisions.md I3) Update `docs/architecture/checkout-ownership.mmd`. Run `make artifacts` and `cargo test -p uze-extensions`
- [x] 6.5 This repository's own `AGENTS.md`: remove the "Authoring plugins" and "Concurrent work isolation" sections, and update the "`uze agent …` is an audience" bullet to name the launch context
- [ ] 6.6 Journeys: the workspace chapter checks that a launch leaves `AGENTS.md` byte-identical, and the packages chapter checks that `uze install` removes old regions
- [ ] 6.7 At archive, rewrite the Purpose of `openspec/specs/plugin-authoring/spec.md`, which still says the verbs are "documented in the projected `AGENTS.md` region"
