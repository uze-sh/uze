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
definition, on every route and harness, to the delivered package root — the
same root `mcp.json` and `hooks.json` resolve to, through one resolver
(`shared::package_root`). That root is `runtime/packages/<id>`: the whole
package, copied from the Store before any harness is handed it and copied
again only when the two differ. Only Claude's envelope holds the whole
package (Codex's and Antigravity's carry `skills/`, OpenCode has no package
tree), so no envelope can be the root; and the Store cannot either, because
a hook that builds into its root (`bun install` in `${PLUGIN_ROOT}/ui`, as
the aikit migration's forge hook did) would write the bytes `agents.lock`
pins. A write into the copy is undone by the next delivery that finds it
different, which is what a harness's own plugin cache does too. A skill
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

## Materialization

### Context

See the proposal's "How the bytes reach a harness". Evidence for every vendor claim below is in
the study runs of 2026-09-28 (observation-only experiments
`experiments/<vendor>/study_mechanics*.py` and `study_discovery*.py`, removed
once their facts were recorded here; recover them from commit `083571f7` to
measure again); versions are those of image `beta4-final`. The facts a
delivery depends on become Lab checks in `harness-dialect-table`.

#### What each harness does with each mechanic

| | Claude | Codex | OpenCode | Antigravity |
|---|---|---|---|---|
| loose skill, dir symlink | ok | ok | body ok, `<skill_files>` empty | ok |
| loose skill, `SKILL.md` symlinked | ok | not listed | ok | ok |
| agent file symlinked | ok loose, dropped in a plugin | listed, cannot run | ok | ok |
| plugin cache copy of a linked file | dropped | dropped | no plugin cache | dereferenced |
| plugin read from | source, live | cache snapshot | n/a | staged copy |
| hardlinked file | ok | ok | ok | ok |
| read-only file (0444/0555) | ok | ok | ok | ok, modes normalized |
| writes into delivered files | none | none | none | none |
| `${PLUGIN_ROOT}` expanded | only `${CLAUDE_PLUGIN_ROOT}` | no | no | no |

#### Where each harness discovers skills

| root | Claude | Codex | OpenCode | Antigravity |
|---|---|---|---|---|
| `~/.agents/skills` | no | yes | yes | no |
| `./.agents/skills` | no | yes | yes | yes |
| vendor root | `~/.claude/skills` | `~/.codex/skills` | `~/.config/opencode/skills` | `~/.gemini/config/skills` |

Codex lists a skill once per root it finds it in; OpenCode and Antigravity
deduplicate by their own precedence. Agents, MCP servers and hooks have no
shared root at all.

### Decisions

#### The mechanism follows the file kind

| file kind | mechanism | reason |
|---|---|---|
| rendered by UZE (`SKILL.md`, agent, manifest, MCP config, policy sidecar) | written | it has no Store twin to point at; a link to it is refused by Codex and by Claude plugins |
| carried unchanged (scripts, references, assets) | copied | the only mechanism every harness and every cache keeps; `fs::copy` clones where the filesystem can |
| a skill directory in a discovery root | a real directory, `GeneratedTree` | OpenCode does not walk a linked skill root |
| a whole plugin | a generated directory registered with the vendor CLI | unchanged |

Hardlinks passed everywhere, and are still not the default: a hardlink is the
Store's inode, so any writer (a person with an editor, a future harness)
writes the Store. pnpm's refetch loop (#6072) is that failure. Part 2 may
offer them from a read-only copy in the generated tier, where a write costs a
rebuild, never from `store/`. Nothing measured today needs them: time is
spent in vendor CLIs (Claude ~265 ms per `plugin install`), not in copying.

#### `GeneratedTree` carries a digest, not the bytes

`GeneratedFile` carries its content because an agent file is small and a
receipt that can repair itself is worth the bytes. A skill tree can hold
megabytes of assets, and its content is reproducible from the Store and the
Engine, which is what the generated tier is. The receipt carries
`tree_sha256` of the directory as attached. Inspection recomputes it:
equal is Matched, different is Drifted, absent is Missing, a non-directory
at the path is Conflict. Detach removes only a Matched tree. Attach writes
when the path is absent, accepts when it already matches, and otherwise
refuses: an unmatched tree at a UZE name is not UZE's to replace. Repair is
re-attachment through the integration, never a copy out of the receipt.

The digest is computed from the staged tree, before the swap, so the value
recorded is the value of what was put there.

#### Global means each harness's own root; `.agents` means the project

Codex and OpenCode both read `~/.agents/skills`, and UZE has used that as one
entry for both: one wrapper carrying both vendors' encodings, the second
integration reusing the first one's artifact, and a retirement rule that
keeps an entry while the other integration still names it. Measured, the
sharing buys little and costs a lot:

- only two of the four harnesses read `~/.agents/skills`, Antigravity does
  not (its global root is `~/.gemini/config/skills`) and Claude does not;
- agents, MCP servers and hooks have no global shared root on any harness;
- the machinery spans the Core's resource model, the application's name
  resolution and retirement, and three integrations.

So a global delivery goes to each harness's own documented user root, one
owner per directory:

| harness | global skill root |
|---|---|
| Claude Code | `~/.claude/skills` (and the generated plugin) |
| Codex | `~/.agents/skills`, the user root Codex documents |
| OpenCode | `~/.config/opencode/skills` |
| Antigravity | `~/.gemini/antigravity-cli/skills`, which agy 1.2 moves to `~/.gemini/config/skills` and links back (and the generated plugin) |

OpenCode also reads `~/.agents/skills`, and resolves a name found in both by
its own precedence, in which `~/.config/opencode` wins. The skill Codex was
given is therefore shadowed by OpenCode's own copy, never shown twice. The
residue is a Codex-delivered skill visible to an OpenCode that UZE does not
deliver to; it carries Codex's encoding of the invocation policy, and that
is reported, not engineered around.

`.agents/` is where the harnesses actually converged, and it converged at
**project** scope: `./.agents/skills` is read by Codex, OpenCode and
Antigravity, and Antigravity also reads agents, MCP and hooks there.
The project authors it, as it does `AGENTS.md`: UZE writes nothing there,
and mounts what a harness does not read natively into that harness's
runtime projection. That is part 3, now `harness-dialect-table`.

#### Replace, never edit

`persistence::replace_dir(dest, build)` builds into
`<dest>.uze-staging-<pid>` beside the destination (same filesystem, so the
rename cannot cross devices), then exchanges. On Linux
`renameat2(RENAME_EXCHANGE)` swaps two directories in one step and the old
tree is removed after; elsewhere the old tree is renamed aside and the new
one renamed in, which leaves a window of one missing directory and never one
of a half-written one. A failed build removes its staging directory and
leaves the destination untouched. Stale staging directories from a killed
process are removed by the next build of the same destination.

#### Retiring the earlier shape

A loose skill attached by beta.3 has a `SymlinkReference` receipt at the same
path this build plans a `GeneratedTree` for. The capability's identity is the
same and its name is the same, so neither the renamed-receipt rule nor the
package-route rule reaches it. The attach flow gains one more retirement:
a receipt for this resource and integration whose artifact is a different
variant from the one now planned is retired first (Matched: detached;
Missing: forgotten; anything else: held back with the reason). This is the
same rule, one axis wider, rather than a compatibility branch.

The ledger stays at shape 3: shape 3 is not released yet (beta.3 writes
shape 2), so the rung that introduced `GeneratedFile` covers `GeneratedTree`
too.

#### Parts 2 and 3 in brief

- **Explicit envelopes leave the Store.** Codex requires a catalogue entry's
  `source.path` relative to, and inside, the marketplace root, which is why
  the explicit catalogue is rooted at `store/`. Moving it means mirroring
  explicit envelopes into the generated tier like generated ones, at one
  more copy per explicit plugin. Worth it for the invariant ("no harness
  reads `store/`"); measured before it lands.
- **Pruning** walks the generated tier and removes what no receipt and no
  catalogue references. The Store is only ever reported on: a marketplace
  commit that was force-pushed away cannot be acquired again.
- **Project `.agents/`**: read, never written; mounted into the runtime
  projection of a harness that does not read it natively.
- **The dialect table** turns the tables above into data: one entry per
  (harness, artifact kind, fact), version-stamped, each with the Lab check
  that proves it. A new harness is a new column filled by running the
  contract, and an entry that stops holding fails the nightly.
- **`harness:` frontmatter**, canonical for skills and agents, keyed by
  harness id and rendered through the table; never delivered. Unknown keys
  proved harmless on all four harnesses, so the block is for control, not
  for survival.

### Risks / Trade-offs

- **More bytes on ext4/NTFS for loose skills**: one copy of the extras per
  harness that takes the loose route, instead of one link. Bounded by the
  extras of loose skills, which today is OpenCode's route and fallbacks
  elsewhere, and paid back in the removal of the shared-root machinery.
- **An operator who edited a delivered skill** now sees it Drifted rather
  than silently served from the Store. That is the intended behavior.
- **`RENAME_EXCHANGE` is Linux-only**; macOS and Windows take the two-rename
  path. Both are covered by the same tests.
