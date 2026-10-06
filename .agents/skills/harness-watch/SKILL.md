---
name: harness-watch
description: Surveying what the supported harnesses (Claude Code, Codex, OpenCode, Antigravity CLI) shipped since UZE last looked, and judging each change against UZE's own integrations — use whenever someone asks what is new in a harness, to read the vendors' changelogs or release notes, whether a vendor release breaks or improves an integration, whether a declared limitation or an Unsupported delivery can become native, or before planning integration work. Covers the baseline each survey starts from, the primary sources per vendor, where UZE keeps what it believes about each harness, the four verdicts a change can get, and the fixed shape of the report. Read-only: it ends in a report, never in an edit.
---

# Watching the harnesses

UZE's integrations encode beliefs about four products that move every week:
which mechanism is native, which control is missing, which file a harness
reads. A belief written down months ago is the most likely thing to be wrong
in `crates/uze-integrations/`. This survey re-asks those beliefs against what
the vendors shipped, and reports which ones changed.

It is research. It produces a report and stops: no code, no ADR, no
`expected.json` edit until the operator picks a finding to act on.

## Step 1 — the baseline: since when

Each survey starts from the version UZE last proved, not from a date in
memory. `conformance/evidence/<harness>.json` records it:

```bash
for h in claude codex opencode antigravity; do
  python3 -c "import json;d=json.load(open('conformance/evidence/$h.json'));print('$h',d['harness_version'],d['recorded_at'])"
done
```

Everything released after `harness_version` is in scope. If the evidence is
older than about two months, say so in the report: the gap itself is a
finding.

The tool snapshots are the second baseline, and the sharper one:
`conformance/evidence/tools/<harness>.json` holds, for the version it names,
every tool the harness declared to the model (`tools`, with input fields)
and every tool name and input a hook received (`hook_tools`). A release
note that renames or reshapes a tool is checked against that file, never
against memory: diff the vendor's new tool list against `tools`, and the
fields its hooks document against `hook_tools`.

```bash
for h in claude codex opencode antigravity; do
  python3 -c "import json;d=json.load(open('conformance/evidence/tools/$h.json'));print('$h',d['harness_version'],sorted(d['hook_tools']))"
done
```

## Step 2 — what UZE believes, per harness

Read these before the changelog, so each vendor change is read as an answer
to a question UZE already asked:

| belief | where |
|---|---|
| how each capability is delivered, and at which rung | `crates/uze-integrations/src/<vendor>.rs` and `src/<vendor>/` |
| what the harness could not do when last checked | `conformance/evidence/expected.json` (`adaptive`: every declared limitation, with its measurement and the versions it was measured on) |
| how the Lab drives it, and what it declares unsupported | `conformance/harnesses/<vendor>/bindings.py` (`unsupported`) |
| which native tool and input field each portable hook alias names | the `ToolBinding` tables in `crates/uze-integrations/src/<vendor>/hooks.rs`, and the Lab's independent expectation in `conformance/harnesses/<vendor>/vocabulary.json`; both are held against the snapshot, so a vendor rename is a red `vocabulary-*` check the night it ships, and the survey's job is to say what the rename *means* |
| what the harness was measured to do, with the Lab check behind each | the `FACTS` table at the end of `crates/uze-integrations/src/<vendor>.rs` (`measured_on` per fact) |
| why a delivery was chosen | `docs/adr/` (grep the vendor name) |
| what must keep holding | `docs/architecture/invariants.md` |

A prompt the Lab answers for the person is a belief too: it says the gate
does not matter to an operator, and a vendor tightening that gate breaks
real machines while the Lab stays green. The Lab answers prompts on screen
the way a person does, and only the ones listed under "Prompts the Lab
answers" in `conformance/DECISIONS.md` any other way; a release that adds
or changes a prompt (a trust screen, a review, a permission mode) is
checked against that list.

Every declared limitation and every `unsupported` reason is a standing
question: *does the harness have this control yet?* Those are the
highest-value answers a survey can bring back. A finding about rendering
or prompting is settled by a probe in the vendor's own format with no UZE
in it (see `conformance-debug`, Step 4) before it is called a UZE defect:
Claude 2.1.290 shows every hook denial as "hook error", whatever dialect
answers it.

## Step 3 — primary sources only

| harness | changelog | docs |
|---|---|---|
| Claude Code | `github.com/anthropics/claude-code` `CHANGELOG.md` | `code.claude.com/docs` (plugins, plugin marketplaces, skills, hooks, memory, settings, mcp) |
| Codex | `github.com/openai/codex` releases; the source at the release tag when the docs are silent | `learn.chatgpt.com/docs` (config, AGENTS.md, skills, hooks, plugins, mcp; moved from `developers.openai.com/codex`) |
| OpenCode | V2 has no release notes: the commit log of the `v2` branch of `github.com/anomalyco/opencode`, current version from `opencode.ai/update/api/latest/cli/npm` (GitHub Releases and `opencode.ai/changelog` are V1 only) | the docs on that branch, `services/www/src/docs/content/*.mdx`; the tool sources under `packages/core/src/tool/` for names and input fields |
| Antigravity CLI | `agy changelog` (ships in the binary) | in the binary: `~/.gemini/antigravity-cli/builtin/skills/agy-customizations/docs/`; Google's developer blog |

The Antigravity docs and changelog are only reliably current inside the
binary; when it is not installed locally, read them through the Lab
sandbox (`python3 conformance/lab.py --harness antigravity --sandbox -- "agy changelog | head -120"`,
see the `conformance-debug` skill). A blog post, a third-party summary or a
search snippet is a lead to a primary source, never the source. A claim
that could not be traced to one is reported as **unverified**.

Read only for what touches UZE's surface: plugin/extension format and
install CLI, marketplace format, skills (location, frontmatter, invocation
control), hooks (events, matcher, output and exit-code contract, plugin
hooks), MCP configuration, project context files (AGENTS.md handling,
fallback names, nesting, size limits), settings layering and scopes,
session ids and resume, headless mode, plugin-root environment variables,
deprecations and removals. Model launches, pricing, UI polish and IDE
features are out of scope unless they change one of those.

## Step 4 — fan out

The four harnesses are independent: run one subagent per harness, in
parallel, each given steps 1-3 for its vendor and the verdicts below, each
read-only and reporting in the shape of step 6. Consolidate yourself; do
not let one subagent rank another's findings.

## Step 5 — the four verdicts

Every change that survives the scope filter gets exactly one:

- **break** — something UZE writes or relies on stops working or changes
  meaning (a renamed file, a removed field, a hook contract that moved).
  Highest priority, whatever its size.
- **nativize** — UZE climbs a rung on Native > Generated Native > Safe
  Adaptation > Unsupported: a bridge, a generated artifact, a declared limitation or
  an `unsupported` can give way to the vendor's own mechanism.
- **capability** — something UZE could deliver or use that it does not
  today, with no current code to replace.
- **watch** — relevant, but nothing to do yet (behind a flag, announced
  without a release, undocumented).

A change that earns none of these is dropped from the report, not listed
as irrelevant.

## Step 6 — the report

Fixed shape, so two surveys can be compared. Write it in the language of
the conversation; keep code identifiers and quotes from sources verbatim.

```markdown
## Baseline
| harness | last proved (evidence) | current release | gap |

## Findings
Ordered: break, then nativize, then capability, then watch; within each,
by how much of UZE it touches.

### <n>. <harness> — <one-line claim> · <verdict> · confidence <high|medium|low>
- **Change:** what shipped, the version, the primary-source URL.
- **UZE today:** what it does, with `file:line`.
- **Action:** the concrete next step (and whether it needs an ADR, an
  `expected.json` promotion, a Lab check, or only code).

## Standing questions answered
Declared limitations and `unsupported` entries the survey re-asked: still true, now
false (and which finding covers it), or could not tell.

## Unverified
Leads that could not be traced to a primary source.
```

End with the one finding you would act on first and why, in a sentence.
