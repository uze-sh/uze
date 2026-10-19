# Context Manager

The full contract, for plugin authors and contributors. The user-facing
summary is the site's
[Project context](../../web/content/docs/plugins/context.mdx) page; change
both together.

The boundary that observes a *project's* instructions context, distinct from
the Package Manager that owns a *machine's* installed bytes. It exposes three
operations, `inspect`, `plan` and `reconcile`, and nothing else. It writes
nothing into the project's instruction files except to take out what an
earlier release wrote there.

## Principles

Invariants, not implementation details, and they hold for anything built on top
of this boundary later:

- **`AGENTS.md` is the operator's.** No UZE operation creates, changes or
  appends to it, except the cleanup that removes a region an earlier release
  wrote. What only means something inside UZE reaches an agent through its
  launch context; what a plugin tells every agent of a declaring project is an
  `always_on` rule (`plugin-rules`).
- **Deterministic.** Every operation produces the same output for the same
  filesystem state, every time. Nothing here is probabilistic.
- **No semantic merging.** Two files, a hand-written `CLAUDE.md` and a
  hand-written `GEMINI.md`, are never compared, judged equivalent, or combined.
  The Context Manager observes and reports; it never decides that two pieces of
  text mean the same thing.
- **No LLM.** No function in `text_region.rs`, `retired_regions.rs`, or the
  application's `Context` service calls a model, and none ever will as part of
  this boundary.
- **`AGENTS.md` is the baseline.** The one file every recognized harness reads
  natively. It is an existing external convention, preserved as plain content,
  not a uze format.
- **Vendor files stay valid.** `CLAUDE.md`/`GEMINI.md` are never treated as
  deficient. A harness's own file legitimately holding harness-specific
  instructions beside `AGENTS.md` is expected and supported.

## The call graph

```text
uze-core:
  text_region.rs     inspect / detach / remove_unconditionally / region_shape /
                     region_body / has_content_outside_managed_regions /
                     region_identities_present / without_regions
  retired_region.rs  the identities earlier releases wrote: is_retired
                     (project:*, digest-checked), is_retired_package
                     (package:<plugin>:<market>:instructions, by shape),
                     INSTRUCTION_BRIDGE (the @AGENTS.md region of CLAUDE.md)

uze-application:
  retired_regions.rs  plan / remove: the one production writer of AGENTS.md
  context.rs          Context::inspect(project_root)    read-only
                      Context::plan(project_root)       read-only
                      Context::reconcile(project_root)  the cleanup, nothing else

src/cli/agent.rs:
  uze agent context inspect | plan | reconcile [path] [--format json]
```

`uze install` runs the same cleanup when `uze status` would report a retired
region (`EnvironmentDrift::stale_projection`). Neither runs anywhere but a main
working tree, whose `.git` is a directory: in a linked worktree the files are
the branch's, and every slot cut from a target that still carries a region
would otherwise commit the same removal.

## Inspect

`ProjectContextStatus` carries the observed sources (`AGENTS.md` plus the files
each integration declares), the retired regions still present, per-harness
delivery, a portability verdict, and warnings.

`InstructionSourceObservation` splits each file three ways: **discovered**
(`exists`), **user-owned** (content outside every well-formed managed region)
and **managed** (the region identities present, which can only be ones an
earlier release left).

`HarnessContextDelivery` is four cases: `Native` (reads `AGENTS.md` directly),
`ReadsNatively` (reads it directly beside a file of its own, where an earlier
release wrote a bridge), `Projected` (the runtime shim hands it over from
outside the project), and `NotDetected`, which is never counted as a gap.

**Proven zero-write** by filesystem-snapshot equality
(`tests/memory/inspection.rs`).

## Portability

```rust
pub enum Portability {
    NoContext,
    Portable,
    VendorLocked { files: Vec<PathBuf> },
}
```

A pure function of `sources`. `VendorLocked` fires when `AGENTS.md` is absent
but another recognized file has its own content. Two cases get a warning
instead of a verdict: two divergent vendor files with no `AGENTS.md`, and a
vendor file legitimately carrying content of its own beside `AGENTS.md`.

## Plan and reconcile

`plan` answers what `reconcile` would remove, and `reconcile` removes it:

| Region | Removed when | Otherwise |
|---|---|---|
| `project:worktree-policy/<digest>`, `project:plugin-authoring/<digest>` | its body still hashes to the digest | reported as edited and left |
| `package:<plugin>:<market>:instructions` | its markers are well-formed (no content check: no renderer is left to say what it held) | reported as malformed and left |
| `instruction-bridge` in `CLAUDE.md` | no package region is left, and it still holds `@AGENTS.md` | reported and left |

A `CLAUDE.md` that held nothing but the bridge is deleted; one with lines of
the operator's keeps them. A region no release of UZE wrote is not touched.

| Invariant | Where proven |
|---|---|
| Nothing but the cleanup writes `AGENTS.md` | `tests/architecture/layering.rs::a_retired_region_cleanup_is_the_only_writer_of_agents_md` |
| A plugin's `AGENTS.md` reaches no project | `tests/memory/reconciliation.rs::a_declared_plugin_shipping_agents_md_adds_nothing_to_the_project` |
| The operator's text stays byte-identical | `tests/memory/reconciliation.rs::a_package_region_and_a_bridge_only_claude_md_go_together` |
| A linked worktree is left alone | `tests/memory/reconciliation.rs::a_linked_worktree_is_left_as_its_branch_has_it` |
| The Store stays machine-scoped | `context_operations_never_alter_the_installed_package_set` |

The retired list is the one piece of code that exists only for old projects.
It goes once no project can still carry a region, the same deletability test
as a `uze-document` ladder rung.

## What it does to a project that already had files

The scenarios in `tests/memory/inspection.rs::scenario_*` pass end to end: only
a hand-written `CLAUDE.md`; an unrecognized vendor file; a hand-written
`AGENTS.md` with nothing installed; a hand-written `AGENTS.md` in a project
declaring a plugin that still ships one; a hand-written `CLAUDE.md`. In every
case manual content survives byte-for-byte and nothing is added.

**`CLAUDE.md → AGENTS.md` is explicitly not attempted.** Deciding that two
hand-written files mean the same thing is a semantic judgment, which belongs to
an agentic layer and never to deterministic reconciliation.

## The agentic layer sits on top, not inside

`uze:init` (see [uze-skill.md](uze-skill.md)) calls the context service's
`inspect` to *see* a project's real state, decides with a model and the user's
approval what a consolidated `AGENTS.md` should contain, and writes that file
the way a human editing it would. Nothing in that flow requires `uze-core` or
`uze-application` to grow LLM awareness.

## Why this shape

- **No harness's config array is edited.** OpenCode's `instructions` field and
  the Gemini-family `context.fileName` are real second mechanisms, and both
  were rejected: each needs a `ManagedArtifact` variant for a JSON-array edit
  with undocumented order and conflict semantics, and each risks overwriting a
  value the user set.
- **`AGENTS.override.md` is never written.** It *replaces* rather than merges,
  so writing one where a user override may later be expected is the silent
  destruction ADR-009 exists to prevent.
- **Plugins speak through rules, not regions.** A region is a write into a file
  the operator authors, and it said whatever the machine that last reconciled
  it held. An `always_on` rule copied under `.agents/rules/` is an agnostic
  artifact in a namespace UZE owns, committed by the operator and read by every
  harness without UZE.

## Vendor-neutrality

`text_region.rs`, `retired_region.rs` and `retired_regions.rs` name no
harness, and neither does `uze-application`. Which file a
harness keeps beside `AGENTS.md` is each integration's own
`IntegrationPort::context_delivery()` declaration in `uze-integrations`; the
observed sources are `AGENTS.md` plus whatever those declarations name.
