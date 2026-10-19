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
  - A plugin's rules copied into `.agents/rules/<plugin>@<market>/` and
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
2. **Agnostic artifacts.** A file qualifies when it is in an open format, in
   a namespace UZE owns, and readable by a harness without UZE. Plugin rule
   copies in `.agents/rules/<plugin>@<market>/` qualify. The operator commits
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
read as a command. On every channel, the instructions arrive where the
harness puts project instructions:
- Claude: the generated `CLAUDE.md`, which is instruction-grade by
  construction;
- Codex and Antigravity: `additionalContext` at `SessionStart`;
- OpenCode: `system[]`.

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
one. Expected size: about 1,500 bytes, where the region was about 3,500.

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

Reconcile already removes an orphaned region by prefix, inspecting before it
detaches (`text_region::remove_unconditionally` for package regions, content
identity for the others). The three prefixes (`project:worktree-policy`,
`project:plugin-authoring`, `package:`) become *retired*. `uze install` and
`uze agent context reconcile` remove any matched region and report an edited
one. Nothing new is attached under them. The launch does not remove them,
because it writes nothing into the checkout.

The retired-prefix list is the one piece of code that exists only for old
projects. It goes once no project can still carry a region, which is the same
deletability test as a `uze-document` ladder rung. Until then it is the
cleanup this rule's own migration needs, not a compatibility path.

### D7: What goes with the regions

- `policy_region.rs`, the client's "AGENTS.md's workspace section" warnings
  and the policy-region read in `src/ui/orchestrator/reads.rs`.
- `PolicyRegionView`'s `in_step`/`state`. The context popup keeps the policy
  in force and where it came from (`worktree-policy`'s client requirement),
  which never depended on the region.
- `checkout-accounting`'s managed-region discount. Only the lock discount
  remains.
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
  or a `SessionStart` context. → The verbs that matter (`split`, `join`) are
  run by the parent. A subagent working in a split checkout only needs to
  commit there, and the parent tells it so when it hands over the path. Task
  5.3 measures what a subagent receives on each harness.
- **A degraded launch shows its instructions.** Antigravity signed out has no
  hook channel, so the instructions ride the visible first turn. → It is
  reported as degraded already, and a signed-out Antigravity cannot work
  anyway. The alternative, writing the region back, is what this change
  removes.
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

Rollback: re-enable the region writer. The launch section is additive, and
the region removal is idempotent.
