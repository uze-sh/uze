## Context

See [proposal.md](proposal.md). Evidence for every vendor claim below is in
the study runs (`conformance/experiments/<vendor>/study_mechanics*.py` and
`study_discovery*.py`); versions are those of image `beta4-final`.

### What each harness does with each mechanic

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

### Where each harness discovers skills

| root | Claude | Codex | OpenCode | Antigravity |
|---|---|---|---|---|
| `~/.agents/skills` | no | yes | yes | no |
| `./.agents/skills` | no | yes | yes | yes |
| vendor root | `~/.claude/skills` | `~/.codex/skills` | `~/.config/opencode/skills` | `~/.gemini/config/skills` |

Codex lists a skill once per root it finds it in; OpenCode and Antigravity
deduplicate by their own precedence. Agents, MCP servers and hooks have no
shared root at all.

## Decisions

### The mechanism follows the file kind

| file kind | mechanism | reason |
|---|---|---|
| rendered by UZE (`SKILL.md`, agent, manifest, MCP config, policy sidecar) | written | it has no Store twin to point at; a link to it is refused by Codex and by Claude plugins |
| carried unchanged (scripts, references, assets) | copied | the only mechanism every harness and every cache keeps; `fs::copy` clones where the filesystem can |
| a skill directory in a discovery root | a real directory, `GeneratedTree` | OpenCode does not walk a linked skill root |
| a whole plugin | a generated directory registered with the vendor CLI | unchanged |

Hardlinks passed everywhere, and are still not the default: a hardlink is the
Store's inode, so any writer (a person with an editor, a future harness)
writes the Store. pnpm's refetch loop (#6072) is that failure. Phase 2 may
offer them from a read-only copy in the generated tier, where a write costs a
rebuild, never from `store/`. Nothing measured today needs them: time is
spent in vendor CLIs (Claude ~265 ms per `plugin install`), not in copying.

### `GeneratedTree` carries a digest, not the bytes

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

### Global means each harness's own root; `.agents` means the project

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
Delivering into it means UZE writing into the repository beside
`AGENTS.md`, which is a product decision (committed or ignored, and who
owns it), not a delivery detail. It is Phase 3.

### Replace, never edit

`persistence::replace_dir(dest, build)` builds into
`<dest>.uze-staging-<pid>` beside the destination (same filesystem, so the
rename cannot cross devices), then exchanges. On Linux
`renameat2(RENAME_EXCHANGE)` swaps two directories in one step and the old
tree is removed after; elsewhere the old tree is renamed aside and the new
one renamed in, which leaves a window of one missing directory and never one
of a half-written one. A failed build removes its staging directory and
leaves the destination untouched. Stale staging directories from a killed
process are removed by the next build of the same destination.

### Retiring the earlier shape

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

### Phase 2 and 3 in brief

- **Explicit envelopes leave the Store.** Codex requires a catalogue entry's
  `source.path` relative to, and inside, the marketplace root, which is why
  the explicit catalogue is rooted at `store/`. Moving it means mirroring
  explicit envelopes into the generated tier like generated ones, at one
  more copy per explicit plugin. Worth it for the invariant ("no harness
  reads `store/`"); measured before it lands.
- **Pruning** walks the generated tier and removes what no receipt and no
  catalogue references. The Store is only ever reported on: a marketplace
  commit that was force-pushed away cannot be acquired again.
- **Project `.agents/`**: project-scoped plugins delivered into
  `./.agents/` for the harnesses that read it, with the ownership rule for a
  UZE-written file inside the repository decided first.
- **The dialect table** turns the tables above into data: one entry per
  (harness, artifact kind, fact), version-stamped, each with the Lab check
  that proves it. A new harness is a new column filled by running the
  contract, and an entry that stops holding fails the nightly.
- **`harness:` frontmatter**, canonical for skills and agents, keyed by
  harness id and rendered through the table; never delivered. Unknown keys
  proved harmless on all four harnesses, so the block is for control, not
  for survival.

## Risks / Trade-offs

- **More bytes on ext4/NTFS for loose skills**: one copy of the extras per
  harness that takes the loose route, instead of one link. Bounded by the
  extras of loose skills, which today is OpenCode's route and fallbacks
  elsewhere, and paid back in the removal of the shared-root machinery.
- **An operator who edited a delivered skill** now sees it Drifted rather
  than silently served from the Store. That is the intended behavior.
- **`RENAME_EXCHANGE` is Linux-only**; macOS and Windows take the two-rename
  path. Both are covered by the same tests.
