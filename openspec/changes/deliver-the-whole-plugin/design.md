## Context

See [proposal.md](proposal.md). State on `254daf6c` (v1.0.0-beta.3):

- `package_plan` (`uze-integrations/src/shared/marketplace.rs:211`) decides
  by what exists on disk: `.claude-plugin/plugin.json` → explicit envelope
  (`uze-local`, the Store as marketplace); else root `skills/` or `mcp.json`
  → generated envelope (`uze-store`, `runtime/attachments/claude/generated/`);
  else no plan, and every resource goes loose.
- `materialize_envelope` (`claude/generate.rs:51`) writes only
  `.claude-plugin/plugin.json` and `skills/`; skill directories are symlinks
  into the Store (`shared/skill.rs:450`, `link_or_repair`), and a rewritten
  skill links its extra files (`link_extras`).
- `claude_exact_coverage` (`claude/plugin.rs:306`) counts a skill covered
  only when listed in a `skills` array, MCP only when `mcpServers` is inline;
  agents and hooks are never covered (`_ => {}` at `:400`, `_ => false` at
  `marketplace.rs:257`). Anything uncovered is delivered loose
  (`attach.rs:214`).
- Agents are never named through `resolve_exposure_name` (`attach.rs:283`)
  and land as `~/.claude/agents/<stem>.md` (`shared/agent.rs:13`).
- MCP loose labels come from `default_exposure_name_candidates`
  (`uze-core/src/delivery/integration.rs:605`) as `<name@market>-<server>`;
  the only check is `is_cli_safe_token` (no leading `-`).
- `${PLUGIN_ROOT}` is resolved by `shared::mcp::resolve_package_root` for
  MCP and by the hook wrapper; nothing touches skill or agent text.
- When `republish_all` fails for a harness, native delivery is `Skipped`
  (`install.rs:143`) and every capability falls through to loose delivery,
  with only a generic "could not publish" warning.

Why the tester saw three routes: R1 packages carried `.claude-plugin/` with
`mcpServers` pointing at `.mcp.json` → explicit envelope, MCP "uncovered" →
loose `claude mcp add std@cardinal-rtc` → refused. R2 dropped
`.claude-plugin/` → generated envelope with inline servers → worked. Detected
harnesses were not the variable.

## Goals / Non-Goals

**Goals:**

- The plugin a harness receives is the author's plugin: every file, every
  capability once, agents qualified.
- No label UZE hands a vendor can be refused for its spelling.
- One placeholder, one meaning, in every file an author writes.

**Non-Goals:**

- Choosing a route by harness version or by probing (`claude plugin` support
  is already a precondition of publication).
- Translating vendor-only agent frontmatter; that is
  `add-portable-agent-capability`'s portable-subset rule.
- Hook events beyond the current three (`session-start-hooks`).

## Decisions

### The generated envelope is a copy of the Store package

Copy regular files from the Store package into the envelope, then overwrite
the generated ones (manifest, policy-rewritten `SKILL.md`). Links are what
emptied Claude's cache; hard links were considered and rejected because a
harness writing into its copy would write into the Store, breaking the
invariant that integrations never mutate it. The envelope is in the
generated tier, rebuilt on every publication (it already is:
`marketplace.rs:310`), so its size costs disk, not correctness. The same rule
applies to every generated envelope (Codex, Antigravity), since
`shared/mcp.rs:24` already records that staging harnesses do not follow
links.

Receipts for a generated envelope fingerprint the envelope's content, not the
link target; inspect compares content identity, so drift detection keeps
working.

### Agents join the plugin plan

`generatable` gains `agents/`; the generated plan's provided identities
include every Agent resource; `materialize_envelope` writes `agents/` (a copy,
by the rule above). Claude namespaces plugin agents as `<plugin>:<agent>`,
which is the name a native plugin gets, so no Claude-specific label logic is
added. The loose agent route remains only for a harness or a route with no
plugin, where `add-portable-agent-capability` decides its label.

### Coverage mirrors the vendor's discovery, written down once

`claude_exact_coverage` becomes "what Claude would load from this envelope":
manifest field if present, else the documented default directory/file
(`skills/`, `agents/`, `hooks/hooks.json`, `.mcp.json`); `mcpServers` as
object, path string, or default file. The rule lives in the Claude
integration with the vendor documentation it follows cited beside it, and a
test per default. A canonical `hooks.json` with an envelope that has hooks is
reported as shadowed by the envelope (the author's own manifest wins,
Native > Generated).

### MCP labels are the integration's, not Core's

`default_exposure_name_candidates` keeps the qualified identity for
bookkeeping; the label passed to a vendor registry comes from the
integration's `exposure_name_candidates` for `Mcp`, which knows the vendor's
character set. Claude: `<plugin>_<marketplace>__<server>`, with the server
key mapped into `[A-Za-z0-9_-]` and the mapping refused at `plugin check` when
it would collide. Kebab-case package and marketplace names
(`standardize-package-names`) cannot contain `_`, which makes the separators
unambiguous. Existing receipts that recorded an `@` label were never attached
(the vendor refused them), so no migration is needed.

### `${PLUGIN_ROOT}` in skill and agent text

Resolved at materialization for every delivered `SKILL.md` and agent
definition, on every route and harness, to the Store package root — the
same root `mcp.json` and `hooks.json` already resolve to, through one
resolver (`shared::package_root`). The Store root holds the whole package on
every harness and every route, including the ones that link a skill
directory rather than copy it, which an envelope root would not. A skill
whose text carries the placeholder is always a rewritten skill; Store bytes
keep the canonical text. Only the exact token is replaced; a vendor's own
placeholder is left to the vendor.

### One mirror

The Codex envelope already mirrored skills as real files, refusing a link
that escapes the package. That mirror moved to `shared::tree` and the Claude
envelope uses it with its exclusions, rather than a second copy routine that
would have followed a link anywhere.

### Upgrades retire what an earlier build named differently

"An existing receipt wins" made a resource keep the name it was first
attached under. An agent attached by 1.0.0-beta.3 as `reviewer.md` would
have kept that name forever. Before delivering a resource, a receipt whose
name is not one this build derives is retired (matched → detached; missing
→ forgotten); one that drifted is left and the capability is reported as
held back rather than delivered beside it. An install whose plugin was
already attached also retires the loose receipts its plan now covers.

### Loose fallback carries a reason

`deliver_package_to` records why a plan was not used (publication error,
receipt pinning the loose form) as route evidence; the install renderer
prints it. Reporting in general is `honest-delivery-reports`; this change only
guarantees the evidence exists.

## Risks / Trade-offs

- [Disk: a package is stored twice] → generated tier, rebuilt, bounded by the
  packages installed; `node_modules`-sized trees never reach the Store
  (ignored files are excluded at ingest).
- [Rewriting skill text changes the bytes a harness sees] → only the
  documented placeholder, and the Store keeps the canonical bytes; inspect
  shows the skill as rewritten.
- [Claude's default discovery changes] → coverage tests cite the vendor docs;
  the Claude conformance vertical asserts "no duplicate" on the real binary.

## Parity evidence (2026-09-28)

`conformance/experiments/claude/parity.py` installs one plugin twice into
the real Claude Code 2.1.283 (synthetic provider, no tokens): the native
twin written to the vendor's documentation, and the same plugin in UZE's
format delivered by uze 1.0.0-beta.3 (`_fixtures/parity/`). Each row
compares what the session exposed to the model, read from the wire. 9/15
at parity (`bin/` and `commands/` rows are Claude-only and out of scope,
see below):

| row | native | uze |
|---|---|---|
| skills listed, body, user-only refused, model-only body | yes | yes |
| MCP tool name and call (`mcp__plugin_kit_probe__…`) | yes | yes |
| PreToolUse deny, PostToolUse observe | yes | yes |
| `${…PLUGIN_ROOT}` in a skill body | resolved | **literal** |
| agent `kit:reviewer` / `kit:review:security` | runs (haiku / default) | **not found** |
| agent `reviewer` (bare) | not found | runs |
| SessionStart | runs | **not expressible** |
| plugin cache holds `skills/probe/SKILL.md` | yes | **no** (link lost) |

What the run settled beyond the migration report:

- **Claude substitutes its root placeholder in skill, command and agent
  bodies** (documented, and measured: `root=/work/native/plugins/kit`). The
  report's control, which saw it literal, was wrong: a literal
  `${PLUGIN_ROOT}` is a regression against native, not parity.
- **For a directory marketplace Claude loads the plugin from its source**,
  not from the cache. That is why the empty cache did not break the
  tester's session, and why it is still a defect: any load from the cache
  (another marketplace kind, a future version) sees skills that are not
  there. Only rewritten skills — real files — survive the copy today.
- **Plugin agents are named `<plugin>:<subdir>:<name>`** for nested files,
  and ignore `permissionMode`, `hooks`, `mcpServers` and `initialPrompt`,
  which a loose `~/.claude/agents` file honours. Moving agents into the
  plugin is the parity move, and those four fields become route evidence
  (not carried) rather than a silent change.

### Only the portable surface reaches the harness

The envelope carries every file of the package so a skill's supporting
files resolve from the root, but it SHALL NOT carry a path the harness
interprets as a component the canonical format does not define. For
Claude that is `commands/`, `bin/`, `output-styles/`, `workflows/`,
`themes/`, `monitors/`, `settings.json`, `.lsp.json`, and the vendor's
own `hooks/hooks.json` and `.mcp.json` (the canonical `hooks.json` and
`mcp.json` are compiled into the envelope instead). Claude scans those by
default, so a plain copy would deliver Claude-only behaviour UZE never
decided to deliver; the product delivers what is common to the harnesses
(ADR-030: no canonical Command). Each excluded path is listed by inspect
as not delivered, not portable. `bin/` is the clearest case: only Claude
puts a plugin's executables on the shell `PATH` (Codex, Antigravity and
OpenCode have no such mechanism).

## Candidate ADRs

- Generated envelopes are copies, never links into the Store — binds every
  future integration's envelope and the receipt fingerprint model.
- A file a harness reads definition by definition is delivered as the file
  itself, its content carried by the receipt (`ManagedArtifact::GeneratedFile`),
  never a link — Codex 0.158 lists a linked agent and cannot run it.
