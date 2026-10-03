## Context

Every package id is built through one constructor, and the plugin name, the
marketplace name and the install alias were already held to one shared
predicate there. What was missing was the rule itself, a check on the
marketplace name before it is recorded, and anything forgiving the case a
person types.

## Goals / Non-Goals

**Goals:**
- A name UZE accepts is a name every harness accepts.
- Two spellings of one name can never become two packages.
- A refusal is actionable: it says the name that was meant.

**Non-Goals:**
- Display casing. A plugin that wants `PDF Processing` on screen declares
  `displayName`; showing it is a follow-up.
- Rewriting names already on record. None exist outside the rule.

## Decisions

**The rule is the intersection of the harnesses' rules**, not any one of
them. The strictest bound (64 characters, from the Agent Skills
specification and OpenCode) and the strictest charset (lowercase, from all
of them) together make a name that no harness refuses. A plugin's name
becomes a skill namespace and a Gemini extension name, so the plugin is
held to the skill's rule rather than to Claude's looser plugin rule.

**The marketplace name is checked where `marketplace.json` is read**, and
again where a marketplace is recorded. The reader is the earlier point: a
Git marketplace's mirror is keyed by the name before the record is written.
The record is the one every source of a name (a registration, a project's
`agents.yaml`) passes through.

**Case is forgiven at the edge, never in resolution.** Lowercasing happens
where a person's text becomes a name (argument parsing, the collision
prompt, the `name@marketplace` parser). Everything behind that compares
exactly, because every name on record is lowercase by the rule, so there is
no second, case-insensitive comparison to keep consistent with the first.
Authored files are not forgiven: `plugin.json` naming `Git` is refused with
`try \`git\``, because the harness reading the same file will not forgive it
either.

**A marketplace's plugins directory is a path, not a name**, and keeps the
older rule (one segment, not starting with `-`). No harness reads it as an
id.

**A skill's `name` is checked by `plugin check`, not by install.** The
install-side reader keeps a `SKILL.md` verbatim and extracts only the
invocation policy; the check is where the frontmatter is read as a harness
reads it.

## Candidate ADRs

- Names are lowercase kebab-case, the intersection of every harness's rule:
  it bounds what a plugin, marketplace and alias may be called for as long
  as UZE delivers to these harnesses, and amends ADR-036's charset.

## Risks / Trade-offs

- A marketplace published with an uppercase or underscored name can no
  longer be registered. That is the point: its plugins would be refused by
  a harness on delivery. The refusal says the name to publish under.
