## Why

`AGENTS.md` is the operator's file, and UZE writes three kinds of text into
it:

- the workspace's isolation policy (`project:worktree-policy`);
- the plugin authoring verbs (`project:plugin-authoring`);
- one region per declared plugin that ships instructions
  (`package:<plugin>:<market>:instructions`).

None of it means anything to a reader without UZE. The policy region even
opens by telling every agent the workspace did not launch to ignore it. The
text still costs the operator:

- every reconcile is a diff in their repository;
- a hand edit beside a region has to be told apart from UZE's own bytes, in
  every slot (`checkout-accounting`);
- a project that stops using UZE is left with instructions for a tool it no
  longer runs.

UZE is meant to be a layer the operator never depends on. What only means
something inside the workspace now has a channel of its own:
`deliver-agent-context` gives every agent UZE launches a context composed
for that launch, and `portable-agent-memory` already documents its verbs
there, "never in `AGENTS.md`". This change finishes that move and states the
rule behind it.

## What Changes

- **The rule, stated once.** UZE writes into a project only:
  - the declaration the operator asked for (`agents.yaml`, `agents.lock`);
  - agnostic artifacts: files in an open format, in a namespace UZE owns,
    that harnesses read without UZE (plugin rules under
    `.agents/rules/<plugin>@<market>--<rule>.md`, from `plugin-rules`).

  It never writes into a file the operator authors. What only means
  something inside UZE goes to the launch. On the machine, a native
  delivery that works inside and outside a launch (plugins, the rules
  engine) is the ideal. The rest of the rule is in design.md and is a
  candidate ADR.
- **BREAKING: the isolation policy moves to the launch context.** The policy
  becomes a section of the context `deliver-agent-context` composes, and is
  no longer a region of `AGENTS.md`.
  - The section is labeled as instructions, apart from the "notes, not
    instructions" framing memory keeps.
  - The text is concrete, because the launcher knows the answers. It names
    the agent's checkout, its branch and the target, where the region had to
    say "if your working directory is inside `.worktrees/`…".
  - The byte bound is shared out again so the policy, the briefing and
    memory all fit.
- **BREAKING: the plugin authoring region goes.** The `uze:author` Skill,
  delivered natively with the official plugin, already guides the whole
  loop inside the workspace and out of it.
- **BREAKING: a plugin no longer contributes instructions.** The plugin's
  `AGENTS.md` resource and `uze agent plugin create --instructions` are
  removed. Text a plugin wants every agent of a declaring project to read is
  an `always_on` rule, delivered by `plugin-rules`. This waits for
  `plugin-rules` to ship.
- **Regions already written are removed.** The next `uze install` or
  `uze agent context reconcile` removes every UZE region from `AGENTS.md`,
  so the file is left as the operator wrote it. A region edited by hand is
  reported and left in place, as today.
- **Proof.** The Lab shows, on each harness, that an agent given the policy
  through the launch still names its work as its first command and still
  uses the `work` verbs. A degraded channel carries the policy in its first
  turn and is reported as degraded.

## Capabilities

### New Capabilities
- `project-footprint`: what UZE may write into a project and what it never
  writes, and the removal of the regions it wrote before.
- `launch-instructions`: the workspace's instructions as a section of a
  launched agent's context: what they say, how they are bounded and framed,
  and how a degraded channel carries them.

### Modified Capabilities
- `worktree-policy`: the declaration is no longer projected into the shared
  instruction file. Its rendering is now the launch-instructions section.
- `plugin-authoring`: the verbs are documented by the `uze:author` Skill,
  not by an `AGENTS.md` region. The `--instructions` flag goes.
- `agent-work-naming`: "first item of the projected region" becomes the
  first item of the launch instructions.
- `command-grammar`: the agent audience reads the launch instructions.
- `checkout-accounting`: no managed region is left to discount in a slot.
  The projected policy that names the verbs is now the launch section.

## Impact

- `uze-workspace`: `worktree::WorktreePolicy::instructions` renders the
  launch section; `agent_context` gains the instructions section and new
  shares.
- `uze-application`: `services/policy_region.rs` and the region sync at
  launch go; `context.rs` and `project_environment.rs` stop composing
  package and authoring regions and remove the old ones once; the read
  models drop the region's standing.
- `uze-core`: `package/authoring/region.rs`, the `instructions` scaffold and
  `project::context`'s package contributions go. The `text_region` removal
  path stays for the one-time cleanup.
- `src/ui`: the "AGENTS.md's workspace section" warnings and the policy
  region read in `orchestrator/reads.rs` go.
- Depends on `plugin-rules` for the instructions replacement, and on
  `deliver-agent-context`'s channels, already implemented.
- `plugins/uze` (`init`, `worktree`, `author`), `docs/capabilities/`,
  `web/content/docs/`, `docs/architecture/invariants.md` and this
  repository's own `AGENTS.md`.
- Conformance Lab: the naming-first and `work` verb scenarios run from the
  launch context on all four harnesses.
- Other open changes edited to match: `project-agent-environment`
  (agents-manifest: a policy change reaches the next launch),
  `prove-plugin-delivery` (9.2 becomes a leak check),
  `track-touched-repositories` (a nudge would go in the launch
  instructions) and `shelve-work-not-checkouts` (derived content is the lock
  only). `resolve-instruction-files` owns the `CLAUDE.md` bridge, and this
  change's cleanup is a no-op once that change removes it.
- No new dependency.
