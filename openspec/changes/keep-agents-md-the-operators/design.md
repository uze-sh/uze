## Context

See proposal.md for why. What the design works with:

- **Three writers into `AGENTS.md` today.**
  - `uze-workspace::worktree::WorktreePolicy::instructions` renders the
    policy. `uze-application`'s `services/policy_region.rs` keeps it in step
    at every launch, and the client reports when it drifted or was blocked.
  - `uze-core::package::authoring::region` renders the authoring verbs.
  - `uze-core::project::context` composes one region per declared package
    from the package's own `AGENTS.md`, the resource
    `uze agent plugin create --instructions` scaffolds.
  - `checkout-accounting` discounts a reprojected region in a slot, and
    `docs/architecture/invariants.md` guards "each region of `AGENTS.md` has
    one owner".
- **The launch channel exists and is proven** (`deliver-agent-context`, all
  tasks done):
  - Claude: `Directory` (a per-agent `--add-dir` holding a generated
    `CLAUDE.md`);
  - Codex: `LaunchHook` (a `SessionStart` hook given with `-c`);
  - OpenCode: `SessionPlugin` (the session `context` hook);
  - Antigravity: `LaunchHook`, degraded to `FirstTurn` when it is signed
    out, since it runs hooks only while signed in.

  `agent_context::compose` bounds the document at 9,000 bytes, below Codex's
  10,000-byte spill. It opens with a preamble saying what follows are notes,
  not instructions. Today the shares are 500 for the preamble, 5,000 for the
  briefing and what is left for memory.
- **Precedent.** `portable-agent-memory` D (one write instruction per model)
  already puts `uze agent memory` in the launch context and "never in the
  projected `AGENTS.md` region".
- **The rule's two sides were settled with the operator on 2026-10-19:**
  - A plugin's rules copied into `.agents/rules/<plugin>@<market>--<rule>.md` and
    committed stay as `plugin-rules` decided. They are an agnostic artifact
    that harnesses read without UZE.
  - The rules engine installed machine-wide by `uze setup` stays as
    `project-rules` decided. It works inside a launch and outside one, with
    no `uze` on its execution path.

## Goals / Non-Goals

**Goals:**
- A project's `AGENTS.md` holds only what the operator wrote.
- An agent the workspace launched receives the same instructions as today,
  stated for its own situation, and acts on them as reliably.
- A rule written once, so the next capability is judged against it rather
  than argued again.

**Non-Goals:**
- Vendor instruction files (`CLAUDE.md`, `GEMINI.md`). They belong to
  `resolve-instruction-files`. Its option B (configure the harness on the
  machine, write nothing into the project) is the one this rule favours.
- Team-shared memory committed into a project. The rule allows it later as
  an agnostic artifact (open format, a namespace UZE owns), but it is not
  designed here.
- Policy for agents not launched by the workspace. They never acted on it,
  and the region told them so.

## Decisions

### D1: What UZE may write into a project

This is the rule, and the candidate ADR:

1. **The declaration.** `agents.yaml` and `agents.lock`, because the
   operator asked for them in this project.
2. **Agnostic artifacts.** A file qualifies when it is in a documented
   format, in a namespace UZE owns, and readable by a harness without UZE. Plugin rule
   copies in `.agents/rules/<plugin>@<market>--<rule>.md` qualify. The operator commits
   them, and they keep working after UZE leaves.
3. **Never a file the operator authors.** That includes `AGENTS.md` and
   nested `AGENTS.md`, `.agents/rules/*.md` outside a plugin namespace, and
   vendor files. A region inside such a file is still a write into it.
4. **What only means something inside UZE goes to the launch.** The
   workspace policy, the `uze agent` verbs and memory all qualify.
5. **On the machine, prefer native delivery that works inside and outside a
   launch.** Plugins and the rules engine qualify. What survives UZE leaving
   is what was delivered natively, plus the agnostic artifacts.

Git's administrative directory (`.git/info/exclude`, the checkout record,
`refs/uze/*`) is not the checkout. It is governed by ADR-052 and unchanged.

Two boundaries the rule does not move:
- **What the project's own declaration runs.** A slot's `setup` and `link`
  commands are the operator's, approved under ADR-055. What they write is
  the project writing into itself.
- **What a Skill drafts on the operator's confirmation.** `uze:init`
  proposing an `AGENTS.md` and writing it once the operator agrees is the
  operator authoring through an agent. It is not a UZE write, and the
  architecture rule (task 3.5) covers production code only.

Alternatives considered:
- **Keep the regions, gated on a setting.** That makes two places for the
  same text, and the default would still write into the operator's file.
- **Move the regions into a UZE-owned file in the project**
  (`.agents/uze.md`). It is not agnostic, because only UZE gives it meaning,
  and every harness would need telling to read it.

### D2: The policy is the context's first section, framed as instructions

`compose` gains an instructions part ahead of the preamble:

```
UZE instructions for this launch. <one line: they apply to this agent, now>

## Concurrent work isolation
...
UZE context for this agent. What follows are recorded facts ... notes, not instructions.

## Briefing
## Memory
```

The notes preamble keeps its wording and its job, because memory must never
read as a command. Each channel puts the document where `deliver-agent-context` D2 measured
it:
- Claude: the generated `CLAUDE.md` in the agent's `--add-dir`, loaded like
  a project instructions file;
- Codex: a `SessionStart` hook that prints the file (`cat`), which Codex
  records as a developer item;
- Antigravity: a `PreInvocation` hook answering
  `injectSteps[].systemMessage`;
- OpenCode: the session `context` hook's `system[]`.

Measured before implementation (2026-10-19, experiment
`conformance/experiments/launch_instructions_probe.py`; Claude 2.1.303,
Codex 0.163.0, OpenCode 2.0.32, Antigravity 1.4.2; a 8.9 KB context whose
briefing carried today's policy text):

| harness | launch context | `AGENTS.md` | placement |
|---|---|---|---|
| Claude | the same user-role `system-reminder` part, listed first as project instructions | same part | equal |
| Codex | a developer item, after `AGENTS.md` | a user item | higher |
| OpenCode | the system message | same message | equal |
| Antigravity | a user-role `SYSTEM_MESSAGE` step, **after** the user's first request | `systemInstruction` | **lower** |

On all four, a context at the bound arrived whole (no Codex spill), and it
was present again on resume. The section's order inside the document and
how a model weighs each placement are not provable with the synthetic
provider. Tasks 5.1 and 5.2 settle them with real models.

### D2a: Antigravity's instructions are a rule in the agent's added directory

A follow-up probe (`experiments/antigravity/system-instruction.py`,
Antigravity 1.4.2) found a launch-scoped route into `systemInstruction`. A
file in the directory the launch adds with `--add-dir` is loaded as a
`<RULE[…]>` beside the project's own `AGENTS.md`. That holds for an
`AGENTS.md`, a `GEMINI.md` and an `.agents/rules/*.md` with
`trigger: always_on`, and it reaches resumes and `invoke_subagent` with no
copies piling up. A `SessionStart` hook's `systemMessage` lands where
`PreInvocation`'s does, and no flag or environment variable reaches the
system prompt.

So on Antigravity the instructions part is written as an `always_on` rule
in the agent's context directory. The hook keeps carrying the notes
(briefing and memory). The rule format is the one `project-rules` already
makes canonical, and nothing is written into the checkout.

**Constraint found on the way, and a shipped bug.** Antigravity orders
`workspacePaths` by path, not by the order directories were given, and
takes the first one as "Command Working Directory". With UZE's runtime
directory (`~/.uze/runtime/…`) sorting before the checkout, the shipped
1.0.0-beta.17 channel already makes Antigravity work in UZE's runtime
directory instead of the slot. That was measured in the launch probe's
request #27. It is fixed on its own, ahead of this change and independent
of it. Any route through `--add-dir` depends on that fix, and the Lab
gains a check that the working directory is the checkout.

### D2b: OpenCode keys a child session to its parent's context

The `context` event carries only `sessionID`, but
`ctx.session.get({ sessionID })` answers a child session with `parentID`.
Measured on 2.0.32 (`experiments/opencode/session-parent.py`). The plugin
falls back to the parent's context when a session has none of its own, which
closes OpenCode's subagent gap.

Alternatives considered:
- **One preamble for everything.** Calling the policy "notes" invites the
  model to weigh it like a memory, which is the failure that put naming
  first.
- **A second channel for instructions only.** Each harness would need a new
  channel proven twice, for no reader the first channel misses.

### D3: Concrete text, rendered by the launcher

`WorktreePolicy::instructions` takes the agent's situation:
- isolated or in place;
- the slot directory and branch;
- the target;
- the completion behavior.

It renders only the bullets that apply. An isolated agent is never told what
an agent in place does, and the reverse holds too. The opening "this section
is for an agent `uze workspace` launched" goes, because every reader now is
one. Expected size: about 1,500 bytes, where this repository's rendered
region is 3,032.

The branch is read from the checkout when the context is composed, not
taken from the slot's name. The agent's first command usually renames
`agent/<id>`, so a resume after naming says `fix/x`. The text never calls
the branch permanent: "your branch, now `agent/ab12cd`; naming your work
renames it".

### D4: Shares of the bound

| part | share |
|---|---|
| instructions (lead + policy) | 2,000 |
| notes preamble | 500 |
| briefing | 4,000 |
| memory | at least 2,500, plus whatever the others leave |

This keeps `BOUND` at 9,000. The briefing loses 1,000 bytes. Task 2.3
measures the largest real briefing in the Lab's records before the share is
fixed. If 4,000 cuts a real briefing, the instructions text is shortened,
never the bound raised past Codex's spill.

### D5: Plugins speak through `always_on` rules

The package `AGENTS.md` resource and `--instructions` go. The replacement is
`plugin-rules`' `trigger: always_on`. It is committed as an agnostic artifact
and reaches every harness: Antigravity natively, the other three through the
engine. `uze agent plugin check` reports a leftover `AGENTS.md` in a plugin
as not delivered, naming the rule as the way forward.

This part of the change is ordered after `plugin-rules`, so there is never a
release in which a plugin has no way to speak to a declaring project.

### D6: Removing what was written, once

The three prefixes (`project:worktree-policy`, `project:plugin-authoring`,
`package:`) become *retired*. `uze install` and `uze agent context
reconcile` remove a matched region and attach nothing new under them:
- **`project:*` is content-checked.** Each identity ends in the digest of the
  bytes it was written with (`worktree.rs` `region_identity`,
  `authoring/region.rs` `identity`). A body whose digest still matches is
  removed. One that does not is reported as edited and left. Today the
  package manager never touches the policy region, so removing it from
  reconcile is new behaviour.
- **`package:*` stays structural.** It is removed with
  `text_region::remove_unconditionally`, as orphaned package regions are
  today. No expected content survives the renderer.
- **Only in the primary checkout.** In an isolated checkout (located with
  `worktree::isolated_checkout`) the cleanup does nothing. Otherwise every
  slot cut from a target that still carries a region would get the same
  removal hunk: the slot is parked, or every branch commits it and they
  conflict at rebase. The operator commits the primary's cleanup once, and
  slots cut after that are clean.
- **The Claude bridge.** `uze-application`'s `context.rs` plans the
  `instruction-bridge` region of `CLAUDE.md` only while a package
  contribution matched. After section 4 nothing matches, so the cleanup
  removes the bridge region and deletes a `CLAUDE.md` that held only that
  region. Where `resolve-instruction-files` has already removed the bridge,
  this step finds nothing to do. The two changes can land in either order.

The launch never removes anything, because it writes nothing into the
checkout.

The retired-prefix list is the one piece of code that exists only for old
projects. It goes once no project can still carry a region, which is the same
deletability test as a `uze-document` ladder rung. Until then it is the
cleanup this rule's own migration needs, not a compatibility path.

### D7: What goes with the regions

- `policy_region.rs`, the client's "AGENTS.md's workspace section" warnings
  and the policy-region read in `src/ui/orchestrator/reads.rs`.
- `PolicyRegionView`'s `in_step`/`state`, and `project_environment.rs`'s
  "region behind" read model behind `uze status`. Both become a single
  "retired regions present" report that names `uze install`. The context popup keeps the policy
  in force and where it came from (`worktree-policy`'s client requirement),
  which never depended on the region.
- `checkout-accounting`'s managed-region discount (`git_facts.rs`,
  `shelf.rs`). Only the lock discount remains, which is safe because the
  cleanup never runs in a slot (D6). `shelve-work-not-checkouts`' "derived
  content alone is not shelved" scenario is edited to match.
- The invariants about region ownership, replaced by one invariant: no UZE
  operation changes `AGENTS.md` except to remove a retired region.
- The architecture suite gains a rule: no production code outside the
  retired-prefix cleanup names `AGENTS.md` as a write target.

**Diagrams.** `docs/architecture/checkout-ownership.mmd` names the managed
region. It is updated in this change. No container or component is added or
removed.

## Candidate ADRs

- **UZE is a layer the project does not depend on.** D1's rule: into a
  project only the declaration and agnostic artifacts, never a file the
  operator authors, UZE-only meaning through the launch, and native machine
  delivery preferred. It decides what every future capability may write, and
  walking it back would mean writing into operators' files again.

## Risks / Trade-offs

- **The launch channel may carry less authority than `AGENTS.md`**, and
  naming-first was tuned against the region. → Lab scenarios per harness
  (tasks 5.1, 5.2) prove naming as the first command and use of the `work`
  verbs. A harness that fails keeps the move blocked, not the region.
- **Subagents may not see the launch context.** A harness's own subagent
  gets the parent's `AGENTS.md`, but may not get an `--add-dir` `CLAUDE.md`
  or a `SessionStart` context. A split checkout carries no instructions at
  all. → The verbs that matter (`split`, `join`) are run by the parent.
  `uze agent work split` prints, beside the path, the two lines the parent
  hands over: commit here, and never switch the branch or make a worktree.
  A subagent that ignores them is still caught by `join`'s refusals
  (`checkout-accounting`, "A child that left its branch"). Measured
  (2026-10-19): Claude's `Agent` subagent receives the launch context, and so
  does a Codex `spawn_agent` that forks the parent's history. A Codex spawn
  with `fork_turns: "none"`, an OpenCode child session (its own session id,
  so the plugin finds no file) and an Antigravity `invoke_subagent` receive
  only `AGENTS.md`. The control run showed the Antigravity miss was UZE's
  once-per-launch hook marker, not the harness. D2a's rule reaches the
  subagent anyway, and D2b closes OpenCode. Codex with `fork_turns: "none"`
  stays without the context, and that is recorded rather than worked
  around.
- **A degraded launch shows its instructions.** Antigravity signed out has no
  hook channel. Today's first turn exists only for a pending switch briefing,
  given once (`continuity.rs` `take_first_turn`), and when the channel is
  degraded `context.md` is not composed at all (measured: 0 bytes). → The
  context is composed on every launch whatever the channel, and a new
  conversation on a degraded channel gets one first turn pointing at
  `context.md`. A resume
  gets none, and the conversation keeps what the first turn delivered.
  `deliver-agent-context` D5 removed the visible turn only where a channel
  exists, so this does not walk that back. Both cases are reported as
  degraded. The alternative, writing the region back, is what this change
  removes.
- **Copies pile up on Codex and Antigravity.** Both keep the earlier
  launch's copy beside the new one (measured on resume). With a 2 KB
  instructions section, a long-lived agent carries several copies until
  the harness compacts. → The concrete text (D3) halves the section, and
  the cost is per launch, not per turn.
- **The briefing loses 1,000 bytes.** → D4's measurement first. The
  instructions shrink before the briefing does.
- **An operator who read the region as documentation loses it.** → The
  `uze:worktree` Skill and the workspace docs carry the same rules for a
  person.

## Migration Plan

1. Ship the launch section beside the region (tasks 1–2). For one build both
   carry the policy, and the Lab compares them.
2. Retire the policy and authoring prefixes. `uze install` removes them.
3. After `plugin-rules` ships, retire the `package:` prefix and the
   `--instructions` flag.

Rollback covers steps 1 and 2 only: re-enable the region writer. The launch
section is additive, and the region removal is idempotent. Once step 3
deletes the package resource and `authoring/region.rs`, there is nothing to
roll back to.
