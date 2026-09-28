## Context

See [proposal.md](proposal.md). `CapabilityKind::Agent` already exists for
import evidence, but package composition, routing, delivery, lifecycle, and
the support UI treat it as unimplemented. This change crosses the Core,
Application/Integration boundary, all four integration verticals, generated
support reporting, and isolated real-harness conformance.

## Goals / Non-Goals

**Goals:**

- Preserve one vendor-neutral `agents/<name>.md` definition in the Store and
  derive every harness artifact from it.
- Treat an Agent as a capability-level concern, so one unsupported or adapted
  target never prevents independent Skill or MCP delivery.
- Retain receipt-based inspect-before-detach safety and explicit route
  evidence.

**Non-Goals:**

- Define a common agent runtime, cross-harness delegation protocol, or tool
  permission schema.
- Convert arbitrary vendor-only agent directories into canonical UZE agents.
- Translate vendor-only model, tool, or sandbox policy beyond the portable
  Markdown subset.
- Add Hooks portability as part of this change.

## Decisions

### Canonical package surface is Markdown agent definitions

`agents/<name>.md` is the sole portable Agent surface. The file body is the
agent's prompt and its frontmatter carries portable metadata only where that
metadata is structurally compatible. The Store keeps it verbatim; integrations
produce wrapper files or configuration under UZE-managed derived locations.

This matches the Markdown definition surfaces documented by Claude, OpenCode,
and Antigravity and avoids inventing a UZE-specific manifest. JSON-only or
configuration-only agents are intentionally out of scope because translating
their tool/model/permission semantics would be lossy.

### Route every agent at capability level

The Engine discovers one Resource per agent definition. Integrations declare
their route in `HarnessCapabilities` and create a per-resource exposure plan.
This follows the existing Skill/MCP fallback architecture and means packages
may still use native package delivery for the resources it can cover while
Agents receive their own projection.

### Harness-specific projections

- Claude Code: generate/install its plugin-native `agents/<name>.md` content
  in the existing package generation or agent-specific artifact path.
- OpenCode: materialize a managed configuration-scope Markdown agent in the
  documented `agents/` discovery root.
- Antigravity CLI: generate its documented Markdown agent representation in
  the agent discovery root or generated plugin envelope, preserving only
  documented portable fields.
- Codex: generate a documented standalone TOML custom-agent file with the
  required `name`, `description`, and `developer_instructions` fields; expose
  it in `~/.codex/agents/` through a receipt-owned reference. This is
  Generated Native, not an adapter.

The concrete path and schema for each projection must be proven by the
respective real-harness vertical before the route is marked Native. Codex
generation must not introduce model, tool, or approval privileges absent from
the portable definition.

### Identity, naming, and lifecycle follow existing exposure machinery

Agent resource identity derives from package identity plus canonical agent
name. The existing collision naming machinery is reused where a global
discovery root needs a stable qualified label. Every generated file,
directory, or configuration entry has a typed receipt and participates in
existing inspect, detach, reconcile, and drift paths.

This is an architecturally significant format and projection choice; the
corresponding ADR artifact records it.

### Field evidence from the 2026-09-28 migration report

A real marketplace (14 agents) delivered on 1.0.0-beta.3 showed the gap
between this design and the code: every harness received the bare file stem
(`resolve_exposure_name` skips agents, `attach.rs:283`), three harnesses
received the Store bytes raw, all four claim Native with no Codex or OpenCode
scenario, and `plugin check` validates no agent. Probing OpenCode 2.0.15 in
an isolated `HOME` settled its surface:

- it reads `~/.config/opencode/agents/` (and the legacy `agent/`), follows
  symlinks, and picks up a file added while its background service runs;
- the agent id is the file stem; a `name:` field is ignored; a stem with `:`
  (`flow:colon.md`) is accepted as `flow:colon`;
- an agent with `model: haiku` or `tools: Read, Grep` is **dropped silently**
  (not listed, no error); `model: anthropic/<id>` is accepted. That alone is
  why the report saw 0/14 agents on OpenCode.

### Qualified labels, by the harness's own mechanism

The label is `<plugin>:<agent>` everywhere. Claude gets it by delivering
agents inside the plugin (`deliver-the-whole-plugin`), which is how Claude
names plugin agents natively. OpenCode gets it from the file stem
(`<plugin>:<agent>.md`, proven above). Antigravity and Codex: the Lab
decides whether the qualified stem / TOML `name` is accepted; if not, the
fallback label is `<plugin>-<agent>` and the route is Adapted with that
reason. Agents go through `resolve_exposure_name` like Skills, so ADR-029's
conflict detection covers them.

### The label follows Claude's scoped name

Claude names a plugin agent `<plugin>:<subdir>…:<name>`: every subdirectory
under `agents/` joined with colons, then the frontmatter `name`, or the file
stem when there is none (measured on 2.1.283: `agents/review/security.md`
loads as `kit:review:security`; documented: `name: audit` in the same file
loads as `kit:review:audit`). UZE adopts that rule as the portable label on
every harness. Today the stem alone is used, so `agents/a/x.md` and
`agents/b/x.md` collide everywhere, and a `name:` that differs from the stem
is ignored.

### An agent is delivered as a file, not a link

Every harness receives its agent definition as a regular file in its own
agents directory, the content carried by the receipt
(`ManagedArtifact::GeneratedFile`): attached when absent, recognised when
identical, refused when someone else's, never removed once edited. Codex
0.158 lists a linked agent file and answers "agent type is currently not
available" when it is dispatched; a regular file runs. Measured per harness:

| | where | name from | carried |
|---|---|---|---|
| Claude Code | inside the package plugin (loose only when no plugin) | Claude's scoped name | everything; four fields Claude ignores on plugin agents are reported |
| Codex | `~/.codex/agents/<label>.toml` | TOML `name` = label | name, description, body as `developer_instructions` |
| OpenCode | `<config>/opencode/agents/<label>.md` | the file | description, `mode: subagent`, body |
| Antigravity | its global agents directory, `<label>.md` | frontmatter `name` = label | name, description, body |

### Portable subset per harness

`name` (from the label) and `description` are portable; the body is the
prompt. Every other field is carried only where the integration declares it
means the same thing: Claude keeps its own fields (the canonical format is
Claude-shaped Markdown and Claude accepts it whole); OpenCode receives a
generated file without `model` unless it is `provider/model`, and without
`tools` unless it is a map; Codex keeps its current three fields. A
generated file lives in the generated tier and the Store keeps the canonical
bytes. What is dropped is route evidence (Adapted), reported by
`honest-delivery-reports`.

## Risks / Trade-offs

- [Different frontmatter schemas] → Project only a small verified portable
  subset; retain full canonical bytes and call out unsupported fields.
- [Codex custom-agent schema evolves] → Generate only its stable required
  fields and hold Native status to real-harness conformance.
- [Global discovery-root collisions] → Reuse qualified naming and inspect
  existing artifacts before mutation.
- [Vendor version changes] → Pin each vertical's evidence in its fixture and
  make conformance failures block matrix promotion.

## Migration Plan

1. Add discovery and routing behind the existing Agent capability kind.
2. Implement and test each delivery vertical and its receipt lifecycle.
3. Add real-harness conformance scenarios and promote only verified routes.
4. Regenerate the README matrix and expose the same status in the TUI.

Rollback removes derived projections through their receipts; Store package
bytes remain unchanged, so no package migration is necessary.
